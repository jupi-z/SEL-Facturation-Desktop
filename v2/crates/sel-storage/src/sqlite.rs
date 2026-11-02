use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    CreateCustomer, Customer, InvoiceLineResponse, PaymentSyncPayload, RecordPaymentRequest,
    SyncEnvelope, SyncEventResult, SyncPullResponse, UpdateCustomer, UtilityBillingRequest,
    UtilityBillingResponse, UtilityInvoiceSyncPayload,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
    Row, SqlitePool,
};
use std::{str::FromStr, sync::Arc};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct LocalStore {
    pub pool: SqlitePool,
    pub organization_id: Uuid,
    pub device_id: Uuid,
    pub actor_id: Uuid,
    pub offline_grant_id: Uuid,
    write_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Error)]
pub enum LocalPaymentError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("payment amount and allocations must be strictly positive")]
    InvalidAllocation,
    #[error("receivable context is invalid")]
    ContextInvalid,
    #[error("receivable balance would be overallocated")]
    Overallocated,
    #[error("payment allocations exceed the received amount")]
    AllocationExceedsReceipt,
}

#[derive(Clone, Debug, Default)]
pub struct LocalTariffSnapshot {
    pub electricity_unit_price: Option<Decimal>,
    pub electricity_tax_rate: Option<Decimal>,
    pub water_unit_price: Option<Decimal>,
    pub water_tax_rate: Option<Decimal>,
}

impl LocalStore {
    pub async fn connect(
        database_url: &str,
        organization_id: Uuid,
        device_id: Uuid,
    ) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        crate::migrate_sqlite(&pool).await?;
        ensure_identity(&pool, organization_id, device_id).await?;
        let actor_id = load_optional_identity(&pool, "actor_id").await?;
        let offline_grant_id = load_optional_identity(&pool, "offline_grant_id").await?;
        Ok(Self {
            pool,
            organization_id,
            device_id,
            actor_id,
            offline_grant_id,
            write_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn configure_offline_identity(
        &mut self,
        actor_id: Uuid,
        offline_grant_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        for (key, expected, label) in [
            ("actor_id", actor_id.to_string(), "actor identity"),
            (
                "offline_grant_id",
                offline_grant_id.to_string(),
                "offline grant identity",
            ),
        ] {
            let stored: Option<String> =
                sqlx::query_scalar("SELECT value FROM sync_state WHERE key = $1")
                    .bind(key)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(stored) = stored {
                if stored != expected {
                    return Err(sqlx::Error::Protocol(format!(
                        "{label} changed in the existing local database"
                    )));
                }
            } else {
                sqlx::query("INSERT INTO sync_state (key, value) VALUES ($1, $2)")
                    .bind(key)
                    .bind(&expected)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        self.actor_id = actor_id;
        self.offline_grant_id = offline_grant_id;
        Ok(())
    }

    pub async fn record_command(&self, event: &SyncEnvelope) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let payload_sha256 = payload_digest(&event.payload);
        if payload_sha256 != event.payload_sha256 {
            return Err(sqlx::Error::Protocol(
                "sync event payload hash does not match".into(),
            ));
        }
        if let Some(existing) =
            sqlx::query("SELECT payload_sha256 FROM command_journal WHERE event_id = $1")
                .bind(event.event_id.to_string())
                .fetch_optional(&mut *tx)
                .await?
        {
            let existing_hash: String = existing.try_get("payload_sha256")?;
            if existing_hash != event.payload_sha256 {
                return Err(sqlx::Error::Protocol(
                    "event id was reused with another payload".into(),
                ));
            }
            tx.commit().await?;
            return Ok(());
        }
        insert_command_in_tx(&mut tx, event).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_customer(
        &self,
        input: &CreateCustomer,
    ) -> Result<(Customer, SyncEnvelope), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let event_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();
        let payload = serde_json::to_value(input)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid customer JSON: {error}")))?;
        let event = local_event(self, event_id, customer_id, "CUSTOMER_CREATED", payload)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO customers
             (id, organization_id, customer_code, legal_name, contact_fields_json, status, revision, created_at)
             VALUES ($1, $2, $3, $4, $5, 'ACTIVE', 1, datetime('now'))",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .bind(&input.customer_code)
        .bind(&input.legal_name)
        .bind(json!({"address": input.address, "phone": input.phone}).to_string())
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        let customer = load_customer_from_tx(&mut tx, customer_id, self.organization_id).await?;
        tx.commit().await?;
        Ok((customer, event))
    }

    pub async fn get_customer(&self, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, contact_fields_json, status, revision
             FROM customers WHERE id = $1 AND organization_id = $2",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| customer_from_row(&row)).transpose()
    }

    pub async fn update_customer(
        &self,
        id: Uuid,
        expected_revision: i64,
        input: &UpdateCustomer,
    ) -> Result<Option<(Customer, SyncEnvelope)>, sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let Some(current) = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, contact_fields_json, status, revision
             FROM customers WHERE id = $1 AND organization_id = $2 AND revision = $3",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .bind(expected_revision)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let current_customer = customer_from_row(&current)?;
        let legal_name = input
            .legal_name
            .clone()
            .unwrap_or(current_customer.legal_name.clone());
        let address = input.address.clone().or(current_customer.address);
        let phone = input.phone.clone().or(current_customer.phone);
        let status = input
            .status
            .clone()
            .unwrap_or(current_customer.status.clone());
        sqlx::query(
            "UPDATE customers SET legal_name = $1, contact_fields_json = $2,
             status = $3, revision = revision + 1
             WHERE id = $4 AND organization_id = $5 AND revision = $6",
        )
        .bind(&legal_name)
        .bind(json!({"address": address, "phone": phone}).to_string())
        .bind(&status)
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .bind(expected_revision)
        .execute(&mut *tx)
        .await?;
        let payload = serde_json::to_value(input)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid customer JSON: {error}")))?;
        let event = local_event(self, Uuid::new_v4(), id, "CUSTOMER_UPDATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        let customer = load_customer_from_tx(&mut tx, id, self.organization_id).await?;
        tx.commit().await?;
        Ok(Some((customer, event)))
    }

    pub async fn list_customers(&self) -> Result<Vec<Customer>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, contact_fields_json, status, revision
             FROM customers WHERE organization_id = $1 ORDER BY customer_code",
        )
        .bind(self.organization_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(customer_from_row).collect()
    }

    pub async fn load_tariffs(
        &self,
        revision: &str,
        currency: &str,
        period: &str,
    ) -> Result<LocalTariffSnapshot, sqlx::Error> {
        let effective_date = NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
            .map_err(|error| sqlx::Error::Protocol(format!("invalid billing period: {error}")))?;
        let date = effective_date.to_string();
        let electricity = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'ELECTRICITY' AND unit = 'KWH' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED'
               AND effective_from <= $3 AND (effective_to IS NULL OR effective_to >= $3)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(currency)
        .bind(revision)
        .bind(&date)
        .fetch_optional(&self.pool)
        .await?;
        let electricity_tax = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'ELECTRICITY' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(&date)
        .fetch_optional(&self.pool)
        .await?;
        let water = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'WATER' AND unit = 'M3' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED'
               AND effective_from <= $3 AND (effective_to IS NULL OR effective_to >= $3)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(currency)
        .bind(revision)
        .bind(&date)
        .fetch_optional(&self.pool)
        .await?;
        let water_tax = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'WATER' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(&date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(LocalTariffSnapshot {
            electricity_unit_price: sqlite_decimal_column(electricity.as_ref(), "unit_price")?,
            electricity_tax_rate: sqlite_decimal_column(electricity_tax.as_ref(), "rate")?,
            water_unit_price: sqlite_decimal_column(water.as_ref(), "unit_price")?,
            water_tax_rate: sqlite_decimal_column(water_tax.as_ref(), "rate")?,
        })
    }

    pub async fn issue_utility_invoice(
        &self,
        input: &UtilityBillingRequest,
        response: &UtilityBillingResponse,
    ) -> Result<SyncEnvelope, sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let receivable_id = response.receivable_id.ok_or_else(|| {
            sqlx::Error::Protocol("local utility invoice requires a receivable id".into())
        })?;
        let payload = serde_json::to_value(UtilityInvoiceSyncPayload {
            request: input.clone(),
            invoice_id: response.invoice_id,
            receivable_id,
        })
        .map_err(|error| sqlx::Error::Protocol(format!("invalid utility JSON: {error}")))?;
        let event = local_event(
            self,
            Uuid::new_v4(),
            response.invoice_id,
            "UTILITY_INVOICE_ISSUED",
            payload,
        )?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO invoices
             (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot_json, status, version)
             VALUES ($1, $2, $3, datetime('now'), $4, $5, 'v2-a4-1', $6, 'LOCAL_PENDING', 1)",
        )
        .bind(response.invoice_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(&input.billing_period)
        .bind(&response.currency)
        .bind(response.total.to_string())
        .bind(serde_json::to_string(response).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid invoice snapshot JSON: {error}"))
        })?)
        .execute(&mut *tx)
        .await?;
        for line in &response.lines {
            insert_invoice_line(&mut tx, response.invoice_id, line).await?;
        }
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'INVOICE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id.to_string())
        .bind(response.invoice_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(&response.currency)
        .bind(response.total.to_string())
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(event)
    }

    pub async fn get_invoice_response(
        &self,
        id: Uuid,
    ) -> Result<Option<UtilityBillingResponse>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT immutable_snapshot_json, status FROM invoices
             WHERE id = $1 AND customer_id IN (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let snapshot: String = row.try_get("immutable_snapshot_json")?;
            let mut response: UtilityBillingResponse =
                serde_json::from_str(&snapshot).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
                })?;
            let status: String = row.try_get("status")?;
            response.status = status;
            Ok(response)
        })
        .transpose()
    }

    pub async fn get_receivable_summary(
        &self,
        id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT r.id, r.customer_id, r.currency, r.principal_total
             FROM receivables r
             WHERE r.id = $1 AND r.customer_id IN (SELECT id FROM customers WHERE organization_id = $2)
             LIMIT 1",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let principal = decimal_text(&row, "principal_total")?;
        let paid_values: Vec<String> = sqlx::query_scalar(
            "SELECT allocated_amount FROM payment_allocations WHERE receivable_id = $1",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let paid = paid_values
            .into_iter()
            .try_fold(Decimal::ZERO, |total, value| {
                let amount = Decimal::from_str(&value).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid paid amount: {error}"))
                })?;
                Ok::<Decimal, sqlx::Error>(total + amount)
            })?;
        Ok(Some(json!({
            "id": row.try_get::<String, _>("id")?,
            "customer_id": row.try_get::<String, _>("customer_id")?,
            "currency": row.try_get::<String, _>("currency")?,
            "principal": principal,
            "paid": paid,
            "balance": principal - paid,
            "status": if paid >= principal { "SETTLED" } else { "OPEN" }
        })))
    }

    pub async fn record_payment(
        &self,
        input: &RecordPaymentRequest,
    ) -> Result<(serde_json::Value, SyncEnvelope), LocalPaymentError> {
        let _guard = self.write_lock.lock().await;
        if input.received_amount <= Decimal::ZERO || input.allocations.is_empty() {
            return Err(LocalPaymentError::InvalidAllocation);
        }
        let mut grouped = std::collections::BTreeMap::<Uuid, Decimal>::new();
        for allocation in &input.allocations {
            if allocation.amount <= Decimal::ZERO {
                return Err(LocalPaymentError::InvalidAllocation);
            }
            *grouped
                .entry(allocation.receivable_id)
                .or_insert(Decimal::ZERO) += allocation.amount;
        }
        let allocated_total: Decimal = grouped.values().copied().sum();
        if allocated_total > input.received_amount {
            return Err(LocalPaymentError::AllocationExceedsReceipt);
        }
        let payment_id = Uuid::new_v4();
        let payload = serde_json::to_value(PaymentSyncPayload {
            request: input.clone(),
            payment_id,
        })
        .map_err(|error| {
            LocalPaymentError::Database(sqlx::Error::Protocol(format!(
                "invalid payment JSON: {error}"
            )))
        })?;
        let event = local_event(self, payment_id, payment_id, "PAYMENT_RECORDED", payload)
            .map_err(LocalPaymentError::Database)?;
        let mut tx = self.pool.begin().await?;
        for (receivable_id, amount) in &grouped {
            let row = sqlx::query(
                "SELECT customer_id, currency, principal_total FROM receivables WHERE id = $1",
            )
            .bind(receivable_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(LocalPaymentError::ContextInvalid)?;
            if row.try_get::<String, _>("customer_id")? != input.customer_id.to_string()
                || row.try_get::<String, _>("currency")? != input.currency
            {
                return Err(LocalPaymentError::ContextInvalid);
            }
            let principal = decimal_text(&row, "principal_total")?;
            let paid: String = sqlx::query_scalar(
                "SELECT COALESCE(SUM(allocated_amount), '0') FROM payment_allocations WHERE receivable_id = $1",
            )
            .bind(receivable_id.to_string())
            .fetch_one(&mut *tx)
            .await?;
            let paid = Decimal::from_str(&paid)
                .map_err(|error| sqlx::Error::Protocol(format!("invalid paid amount: {error}")))?;
            if paid + amount > principal {
                return Err(LocalPaymentError::Overallocated);
            }
        }
        sqlx::query(
            "INSERT INTO payments (id, customer_id, amount_received, currency, method, paid_at, status)
             VALUES ($1, $2, $3, $4, $5, $6, 'LOCAL_PENDING')",
        )
        .bind(payment_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(input.received_amount.to_string())
        .bind(&input.currency)
        .bind(&input.method)
        .bind(input.occurred_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;
        for (receivable_id, amount) in grouped {
            sqlx::query(
                "INSERT INTO payment_allocations (id, payment_id, receivable_id, allocated_amount, currency)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(payment_id.to_string())
            .bind(receivable_id.to_string())
            .bind(amount.to_string())
            .bind(&input.currency)
            .execute(&mut *tx)
            .await?;
        }
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok((
            json!({"payment_id": payment_id, "status": "LOCAL_PENDING", "amount": input.received_amount, "currency": input.currency}),
            event,
        ))
    }

    pub async fn pending_count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE state = 'PENDING'")
            .fetch_one(&self.pool)
            .await
    }

    pub async fn central_cursor(&self) -> Result<i64, sqlx::Error> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT value FROM sync_state WHERE key = 'central_cursor'",
        )
        .fetch_optional(&self.pool)
        .await?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0))
    }

    pub async fn apply_pulled_changes(
        &self,
        response: &SyncPullResponse,
    ) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        for change in &response.changes {
            let sequence = change
                .get("sequence")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| sqlx::Error::Protocol("sync change sequence is missing".into()))?;
            let entity = change
                .get("entity")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| sqlx::Error::Protocol("sync change entity is missing".into()))?;
            let entity_id = change
                .get("entity_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| sqlx::Error::Protocol("sync change entity id is missing".into()))?;
            let revision = change
                .get("revision")
                .and_then(serde_json::Value::as_i64)
                .ok_or_else(|| sqlx::Error::Protocol("sync change revision is missing".into()))?;
            let change_json = change
                .get("change")
                .cloned()
                .ok_or_else(|| sqlx::Error::Protocol("sync change payload is missing".into()))?;
            let inserted = sqlx::query(
                "INSERT INTO central_changes (sequence, entity, entity_id, revision, change_json)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT(sequence) DO NOTHING",
            )
            .bind(sequence)
            .bind(entity)
            .bind(entity_id)
            .bind(revision)
            .bind(change_json.clone().to_string())
            .execute(&mut *tx)
            .await?
            .rows_affected()
                > 0;
            if inserted {
                match entity {
                    "customers" => {
                        apply_customer_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    "invoices" => {
                        apply_invoice_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    "payments" => {
                        apply_payment_change(&mut tx, self.organization_id, &change_json).await?;
                    }
                    _ => {}
                }
            }
        }
        sqlx::query(
            "INSERT INTO sync_state (key, value) VALUES ('central_cursor', $1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(response.next_cursor.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    pub async fn claim_pending(&self, limit: i64) -> Result<Vec<SyncEnvelope>, sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT event_id, envelope_json FROM outbox
             WHERE state = 'PENDING' ORDER BY rowid LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let event_id: String = row.try_get("event_id")?;
            let envelope_json: String = row.try_get("envelope_json")?;
            let event: SyncEnvelope = serde_json::from_str(&envelope_json).map_err(|error| {
                sqlx::Error::Protocol(format!("invalid outbox envelope: {error}"))
            })?;
            sqlx::query(
                "UPDATE outbox SET state = 'IN_FLIGHT', attempts = attempts + 1 WHERE event_id = $1",
            )
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
            events.push(event);
        }
        tx.commit().await?;
        Ok(events)
    }

    pub async fn retry(&self, event_id: Uuid, reason: &str) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        sqlx::query("UPDATE outbox SET state = 'PENDING', last_error = $2 WHERE event_id = $1")
            .bind(event_id.to_string())
            .bind(reason)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn journal_count(&self, event_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM command_journal WHERE event_id = $1")
            .bind(event_id.to_string())
            .fetch_one(&self.pool)
            .await
    }

    pub async fn acknowledge(
        &self,
        event_id: Uuid,
        result: &SyncEventResult,
    ) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let result_json = serde_json::to_string(result)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid sync result: {error}")))?;
        let state = match result {
            SyncEventResult::Accepted { .. } | SyncEventResult::Duplicate { .. } => "ACKED",
            SyncEventResult::Conflict { .. } => "CONFLICT",
            SyncEventResult::Rejected { .. } => "REJECTED",
        };
        let updated = sqlx::query(
            "UPDATE outbox SET state = $2, last_error = NULL WHERE event_id = $1 AND state IN ('PENDING', 'IN_FLIGHT')",
        )
        .bind(event_id.to_string())
        .bind(state)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        sqlx::query("UPDATE command_journal SET status = $2 WHERE event_id = $1")
            .bind(event_id.to_string())
            .bind(state)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO sync_inbox (device_id, event_id, result_json, received_at)
             VALUES ($1, $2, $3, datetime('now'))
             ON CONFLICT(device_id, event_id) DO UPDATE SET result_json = excluded.result_json",
        )
        .bind(self.device_id.to_string())
        .bind(event_id.to_string())
        .bind(result_json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn conflict(&self, event_id: Uuid, reason: &str) -> Result<(), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE outbox SET state = 'CONFLICT', last_error = $2 WHERE event_id = $1")
            .bind(event_id.to_string())
            .bind(reason)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE command_journal SET status = 'CONFLICT' WHERE event_id = $1")
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn insert_command_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &SyncEnvelope,
) -> Result<(), sqlx::Error> {
    let envelope_json = serde_json::to_string(event)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid event JSON: {error}")))?;
    sqlx::query(
        "INSERT INTO command_journal
         (event_id, device_id, aggregate_id, command_type, payload_json, payload_sha256, status, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'LOCAL_PENDING', datetime('now'))",
    )
    .bind(event.event_id.to_string())
    .bind(event.device_id.to_string())
    .bind(event.aggregate_id.to_string())
    .bind(&event.command_type)
    .bind(event.payload.to_string())
    .bind(&event.payload_sha256)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO outbox (event_id, envelope_json, state, attempts)
         VALUES ($1, $2, 'PENDING', 0)",
    )
    .bind(event.event_id.to_string())
    .bind(envelope_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn local_event(
    store: &LocalStore,
    event_id: Uuid,
    aggregate_id: Uuid,
    command_type: &str,
    payload: serde_json::Value,
) -> Result<SyncEnvelope, sqlx::Error> {
    Ok(SyncEnvelope {
        protocol_version: sel_contracts::PROTOCOL_VERSION,
        event_id,
        device_id: store.device_id,
        organization_id: store.organization_id,
        actor_id: store.actor_id,
        command_type: command_type.into(),
        aggregate_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: store.offline_grant_id,
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: sel_contracts::DOCUMENT_SCHEMA_VERSION,
    })
}

fn customer_from_row(row: &SqliteRow) -> Result<Customer, sqlx::Error> {
    let contact: serde_json::Value = serde_json::from_str(
        &row.try_get::<String, _>("contact_fields_json")?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid customer contact JSON: {error}")))?;
    Ok(Customer {
        id: row
            .try_get::<String, _>("id")?
            .parse()
            .map_err(|error| sqlx::Error::Protocol(format!("invalid customer id: {error}")))?,
        organization_id: row
            .try_get::<String, _>("organization_id")?
            .parse()
            .map_err(|error| sqlx::Error::Protocol(format!("invalid organization id: {error}")))?,
        customer_code: row.try_get("customer_code")?,
        legal_name: row.try_get("legal_name")?,
        address: contact
            .get("address")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        phone: contact
            .get("phone")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
    })
}

async fn load_customer_from_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: Uuid,
    organization_id: Uuid,
) -> Result<Customer, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, organization_id, customer_code, legal_name, contact_fields_json, status, revision
         FROM customers WHERE id = $1 AND organization_id = $2",
    )
    .bind(id.to_string())
    .bind(organization_id.to_string())
    .fetch_one(&mut **tx)
    .await?;
    customer_from_row(&row)
}

async fn insert_invoice_line(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    invoice_id: Uuid,
    line: &InvoiceLineResponse,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO invoice_lines
         (id, invoice_id, service_code, quantity, unit, unit_price, subtotal, tax_snapshot_json, total)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(invoice_id.to_string())
    .bind(&line.service_code)
    .bind(line.quantity.to_string())
    .bind(&line.unit)
    .bind(line.unit_price.map(|value| value.to_string()))
    .bind(line.subtotal.to_string())
    .bind(json!({"tax": line.tax}).to_string())
    .bind(line.total.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_customer_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    customer_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM customers WHERE id = $1 AND organization_id = $2")
            .bind(customer_id)
            .bind(organization_id.to_string())
            .fetch_optional(&mut **tx)
            .await?;
    if existing.is_some_and(|current| current >= revision) {
        return Ok(());
    }
    let customer_code = change
        .get("customer_code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(customer_id);
    let legal_name = change
        .get("legal_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Client synchronisé");
    let status = change
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ACTIVE");
    let contact = json!({
        "address": change.get("address").cloned().unwrap_or(serde_json::Value::Null),
        "phone": change.get("phone").cloned().unwrap_or(serde_json::Value::Null)
    });
    sqlx::query(
        "INSERT INTO customers
         (id, organization_id, customer_code, legal_name, contact_fields_json, status, revision, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET legal_name = excluded.legal_name,
           contact_fields_json = excluded.contact_fields_json, status = excluded.status,
           revision = excluded.revision",
    )
    .bind(customer_id)
    .bind(organization_id.to_string())
    .bind(customer_code)
    .bind(legal_name)
    .bind(contact.to_string())
    .bind(status)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_invoice_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let request: UtilityBillingRequest = serde_json::from_value(
        change
            .get("request")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("invoice sync request is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice sync request: {error}")))?;
    let response: UtilityBillingResponse = serde_json::from_value(
        change
            .get("response")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("invoice sync response is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice sync response: {error}")))?;
    if response.invoice_id.to_string() != entity_id {
        return Err(sqlx::Error::Protocol(
            "invoice sync entity id does not match response id".into(),
        ));
    }
    let customer_exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
            .bind(request.customer_id.to_string())
            .bind(organization_id.to_string())
            .fetch_optional(&mut **tx)
            .await?;
    if customer_exists.is_none() {
        return Err(sqlx::Error::Protocol(
            "invoice sync customer is not present locally".into(),
        ));
    }
    if let Some(existing) =
        sqlx::query("SELECT customer_id, immutable_snapshot_json FROM invoices WHERE id = $1")
            .bind(entity_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        let existing_customer: String = existing.try_get("customer_id")?;
        let existing_snapshot: String = existing.try_get("immutable_snapshot_json")?;
        let mut existing_response: UtilityBillingResponse =
            serde_json::from_str(&existing_snapshot).map_err(|error| {
                sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
            })?;
        let mut expected_response = response.clone();
        existing_response.status.clear();
        expected_response.status.clear();
        let existing_immutable = serde_json::to_value(&existing_response)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}")))?;
        let expected_immutable = serde_json::to_value(&expected_response)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}")))?;
        if existing_customer != request.customer_id.to_string()
            || existing_immutable != expected_immutable
        {
            return Err(sqlx::Error::Protocol(
                "conflicting local invoice projection".into(),
            ));
        }
        sqlx::query(
            "UPDATE invoices SET status = $1, version = CASE WHEN version < $2 THEN $2 ELSE version END WHERE id = $3",
        )
        .bind(&response.status)
        .bind(revision)
        .bind(entity_id)
        .execute(&mut **tx)
        .await?;
        let receivable_id = response
            .receivable_id
            .ok_or_else(|| sqlx::Error::Protocol("invoice sync receivable id is missing".into()))?;
        if let Some(receivable) = sqlx::query(
            "SELECT id FROM receivables WHERE source_type = 'INVOICE' AND source_id = $1",
        )
        .bind(entity_id)
        .fetch_optional(&mut **tx)
        .await?
        {
            let existing_id: String = receivable.try_get("id")?;
            if existing_id != receivable_id.to_string() {
                return Err(sqlx::Error::Protocol(
                    "conflicting local receivable projection".into(),
                ));
            }
        } else {
            sqlx::query(
                "INSERT INTO receivables
                 (id, source_type, source_id, customer_id, currency, principal_total, status)
                 VALUES ($1, 'INVOICE', $2, $3, $4, $5, 'OPEN')",
            )
            .bind(receivable_id.to_string())
            .bind(entity_id)
            .bind(request.customer_id.to_string())
            .bind(&response.currency)
            .bind(response.total.to_string())
            .execute(&mut **tx)
            .await?;
        }
        let line_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM invoice_lines WHERE invoice_id = $1")
                .bind(entity_id)
                .fetch_one(&mut **tx)
                .await?;
        if line_count == 0 {
            for line in &response.lines {
                insert_invoice_line(tx, response.invoice_id, line).await?;
            }
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO invoices
         (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot_json, status, version)
         VALUES ($1, $2, $3, datetime('now'), $4, $5, 'v2-a4-1', $6, $7, $8)",
    )
    .bind(entity_id)
    .bind(request.customer_id.to_string())
    .bind(&request.billing_period)
    .bind(&response.currency)
    .bind(response.total.to_string())
    .bind(serde_json::to_string(&response).map_err(|error| {
        sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
    })?)
    .bind(&response.status)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    for line in &response.lines {
        insert_invoice_line(tx, response.invoice_id, line).await?;
    }
    let receivable_id = response
        .receivable_id
        .ok_or_else(|| sqlx::Error::Protocol("invoice sync receivable id is missing".into()))?;
    if let Some(existing) =
        sqlx::query("SELECT id FROM receivables WHERE source_type = 'INVOICE' AND source_id = $1")
            .bind(entity_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        let existing_id: String = existing.try_get("id")?;
        if existing_id != receivable_id.to_string() {
            return Err(sqlx::Error::Protocol(
                "conflicting local receivable projection".into(),
            ));
        }
    } else {
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'INVOICE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id.to_string())
        .bind(entity_id)
        .bind(request.customer_id.to_string())
        .bind(&response.currency)
        .bind(response.total.to_string())
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn apply_payment_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let customer_id = change
        .get("customer_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync customer id is missing".into()))?;
    let payment = change
        .get("payment")
        .ok_or_else(|| sqlx::Error::Protocol("payment sync body is missing".into()))?;
    let payment_id = payment
        .get("payment_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync id is missing".into()))?;
    let amount = payment
        .get("amount")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync amount is missing".into()))?;
    let currency = payment
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync currency is missing".into()))?;
    let status = payment
        .get("status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync status is missing".into()))?;
    let customer_exists: Option<String> = sqlx::query_scalar(
        "SELECT c.id FROM customers c WHERE c.id = $1 AND c.organization_id = $2",
    )
    .bind(customer_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    if customer_exists.is_none() {
        return Err(sqlx::Error::Protocol(
            "payment sync customer is not present locally".into(),
        ));
    }
    if let Some(existing) =
        sqlx::query("SELECT amount_received, currency FROM payments WHERE id = $1")
            .bind(payment_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        let existing_amount: String = existing.try_get("amount_received")?;
        let existing_currency: String = existing.try_get("currency")?;
        if existing_amount != amount || existing_currency != currency {
            return Err(sqlx::Error::Protocol(
                "conflicting local payment projection".into(),
            ));
        }
        sqlx::query("UPDATE payments SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(payment_id)
            .execute(&mut **tx)
            .await?;
        let allocations = change
            .get("allocations")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| sqlx::Error::Protocol("payment sync allocations are missing".into()))?;
        for allocation in allocations {
            let receivable_id = allocation
                .get("receivable_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    sqlx::Error::Protocol("payment sync receivable id is missing".into())
                })?;
            let allocated_amount = allocation
                .get("amount")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    sqlx::Error::Protocol("payment sync allocation amount is missing".into())
                })?;
            let already_present: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM payment_allocations
                 WHERE payment_id = $1 AND receivable_id = $2 AND allocated_amount = $3 AND currency = $4",
            )
            .bind(payment_id)
            .bind(receivable_id)
            .bind(allocated_amount)
            .bind(currency)
            .fetch_one(&mut **tx)
            .await?;
            if already_present == 0 {
                sqlx::query(
                    "INSERT INTO payment_allocations
                     (id, payment_id, receivable_id, allocated_amount, currency)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(payment_id)
                .bind(receivable_id)
                .bind(allocated_amount)
                .bind(currency)
                .execute(&mut **tx)
                .await?;
            }
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO payments
         (id, customer_id, amount_received, currency, method, paid_at, status)
         VALUES ($1, $2, $3, $4, 'SYNC', datetime('now'), 'CENTRAL_ACCEPTED')",
    )
    .bind(payment_id)
    .bind(customer_id)
    .bind(amount)
    .bind(currency)
    .execute(&mut **tx)
    .await?;
    let allocations = change
        .get("allocations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| sqlx::Error::Protocol("payment sync allocations are missing".into()))?;
    for allocation in allocations {
        let receivable_id = allocation
            .get("receivable_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| sqlx::Error::Protocol("payment sync receivable id is missing".into()))?;
        let allocated_amount = allocation
            .get("amount")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                sqlx::Error::Protocol("payment sync allocation amount is missing".into())
            })?;
        sqlx::query(
            "INSERT INTO payment_allocations
             (id, payment_id, receivable_id, allocated_amount, currency)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(payment_id)
        .bind(receivable_id)
        .bind(allocated_amount)
        .bind(currency)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn decimal_text(row: &SqliteRow, column: &str) -> Result<Decimal, sqlx::Error> {
    let value: String = row.try_get(column)?;
    Decimal::from_str(&value)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid decimal in {column}: {error}")))
}

fn sqlite_decimal_column(
    row: Option<&SqliteRow>,
    column: &str,
) -> Result<Option<Decimal>, sqlx::Error> {
    row.map(|row| decimal_text(row, column)).transpose()
}

async fn ensure_identity(
    pool: &SqlitePool,
    organization_id: Uuid,
    device_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for (key, expected, label) in [
        (
            "organization_id",
            organization_id.to_string(),
            "organization identity",
        ),
        ("device_id", device_id.to_string(), "device identity"),
    ] {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT value FROM sync_state WHERE key = $1")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(stored) = stored {
            if stored != expected {
                return Err(sqlx::Error::Protocol(format!(
                    "{label} changed in the existing local database"
                )));
            }
        } else {
            sqlx::query("INSERT INTO sync_state (key, value) VALUES ($1, $2)")
                .bind(key)
                .bind(expected)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query(
        "INSERT INTO organizations (id, legal_name, created_at)
         VALUES ($1, 'SEL V2 local organization', datetime('now'))
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(organization_id.to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO devices (id, organization_id, public_key_id, state)
         VALUES ($1, $2, 'LOCAL_DEVICE_PENDING_ENROLLMENT', 'ACTIVE')
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(device_id.to_string())
    .bind(organization_id.to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE outbox SET state = 'PENDING', last_error = 'recovered after local restart'
         WHERE state = 'IN_FLIGHT'",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

async fn load_optional_identity(pool: &SqlitePool, key: &str) -> Result<Uuid, sqlx::Error> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM sync_state WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    value
        .map(|value| {
            value
                .parse()
                .map_err(|error| sqlx::Error::Protocol(format!("invalid persisted {key}: {error}")))
        })
        .transpose()
        .map(|value| value.unwrap_or_else(Uuid::nil))
}

fn payload_digest(payload: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(payload).expect("JSON serialization cannot fail"));
    hex::encode(digest.finalize())
}
