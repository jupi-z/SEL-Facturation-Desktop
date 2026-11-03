use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    solar_schedule_id, utility_snapshot_sha256, ActivateSolarContractRequest, CreateCustomer,
    CreateSolarSaleRequest, Customer, InvoiceLineResponse, InvoiceVoidSyncPayload,
    PaymentReversalSyncPayload, PaymentSyncPayload, RecordPaymentRequest, ReversePaymentRequest,
    SolarContractSyncPayload, SolarSaleResponse, SolarSaleSyncPayload, SyncEnvelope,
    SyncEventResult, SyncPullResponse, UpdateCustomer, UtilityBillingRequest,
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

#[derive(Debug, Error)]
pub enum LocalPaymentReverseError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("payment was not found in the local database")]
    PaymentNotFound,
    #[error("payment is not accepted or pending locally")]
    PaymentNotReversible,
    #[error("reversal amount is invalid")]
    InvalidAmount,
    #[error("reversal exceeds the payment amount still reversible")]
    AmountExceedsRemaining,
    #[error("reversal reason is required")]
    ReasonRequired,
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
        sqlx::query(
            "INSERT INTO users (id, organization_id, role, status, identity_ref, created_at)
             VALUES ($1, $2, 'BILLING_AGENT', 'ACTIVE', $3, datetime('now'))
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(actor_id.to_string())
        .bind(self.organization_id.to_string())
        .bind(actor_id.to_string())
        .execute(&mut *tx)
        .await?;
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

    pub async fn void_invoice(
        &self,
        invoice_id: Uuid,
        reason: &str,
    ) -> Result<SyncEnvelope, sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        if reason.trim().is_empty() {
            return Err(sqlx::Error::Protocol(
                "invoice void reason is required".into(),
            ));
        }
        let void_id = Uuid::new_v4();
        let payload = serde_json::to_value(InvoiceVoidSyncPayload {
            invoice_id,
            void_id,
            reason: reason.trim().to_owned(),
            actor_id: self.actor_id,
        })
        .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice void JSON: {error}")))?;
        let event = local_event(self, Uuid::new_v4(), invoice_id, "INVOICE_VOIDED", payload)?;
        let mut tx = self.pool.begin().await?;
        let Some(invoice) = sqlx::query(
            "SELECT status FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(invoice_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(sqlx::Error::Protocol("invoice not found".into()));
        };
        let status: String = invoice.try_get("status")?;
        if status == "VOID" {
            return Err(sqlx::Error::Protocol("invoice is already void".into()));
        }
        let has_paid: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM payment_allocations pa
             JOIN payments p ON p.id = pa.payment_id
             JOIN receivables r ON r.id = pa.receivable_id
             WHERE r.source_type = 'INVOICE' AND r.source_id = $1
               AND p.status IN ('LOCAL_PENDING', 'CENTRAL_ACCEPTED')
               AND CAST(pa.allocated_amount AS NUMERIC) >
                   CAST(COALESCE((SELECT SUM(amount) FROM payment_reversals
                                  WHERE payment_id = p.id), '0') AS NUMERIC)",
        )
        .bind(invoice_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if has_paid > 0 {
            return Err(sqlx::Error::Protocol(
                "invoice has accepted payments".into(),
            ));
        }
        sqlx::query("UPDATE invoices SET status = 'VOID' WHERE id = $1")
            .bind(invoice_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE receivables SET status = 'VOID'
             WHERE source_type = 'INVOICE' AND source_id = $1",
        )
        .bind(invoice_id.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO invoice_voids (id, invoice_id, reason, actor_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(void_id.to_string())
        .bind(invoice_id.to_string())
        .bind(reason.trim())
        .bind(self.actor_id.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, device, event_type, subject_id, occurred_at, redacted_metadata_json)
             VALUES ($1, $2, $3, 'INVOICE_VOIDED', $4, datetime('now'), $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(self.actor_id.to_string())
        .bind(self.device_id.to_string())
        .bind(invoice_id.to_string())
        .bind(json!({"void_id": void_id, "reason": reason.trim()}).to_string())
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(event)
    }

    pub async fn create_solar_sale_offline(
        &self,
        input: &CreateSolarSaleRequest,
    ) -> Result<(SolarSaleResponse, SyncEnvelope), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        if input.principal <= Decimal::ZERO {
            return Err(sqlx::Error::Protocol(
                "solar sale principal must be strictly positive".into(),
            ));
        }
        let customer_exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
                .bind(input.customer_id.to_string())
                .bind(self.organization_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        if customer_exists.is_none() {
            return Err(sqlx::Error::Protocol(
                "solar sale customer is not present locally".into(),
            ));
        }
        let product_exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT' AND currency = $2 AND active = 1",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_optional(&self.pool)
        .await?;
        if product_exists.is_none() {
            return Err(sqlx::Error::Protocol(
                "solar product is not active in the local catalog".into(),
            ));
        }
        let sale_id = Uuid::new_v4();
        let receivable_id = Uuid::new_v4();
        let response = SolarSaleResponse {
            sale_id,
            receivable_id,
            customer_id: input.customer_id,
            principal: input.principal.round_dp(2),
            currency: input.currency.clone(),
            status: "LOCAL_PENDING".into(),
            product_code: Some(input.product_code.trim().to_owned()),
        };
        let payload = serde_json::to_value(SolarSaleSyncPayload {
            request: input.clone(),
            sale_id,
            receivable_id,
        })
        .map_err(|error| sqlx::Error::Protocol(format!("invalid solar sale JSON: {error}")))?;
        let event = local_event(self, Uuid::new_v4(), sale_id, "SOLAR_SALE_CREATED", payload)?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO sales
             (id, customer_id, currency, sold_at, sale_type, status)
             VALUES ($1, $2, $3, datetime('now'), 'SOLAR_KIT', 'LOCAL_PENDING')",
        )
        .bind(sale_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(&input.currency)
        .execute(&mut *tx)
        .await?;
        let line_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "SEL:SOLAR_SALE_LINE:{sale_id}:{}",
                input.product_code.trim()
            )
            .as_bytes(),
        );
        sqlx::query(
            "INSERT INTO sale_lines
             (id, sale_id, product_ref, quantity, unit_price, tax_snapshot_json, total)
             VALUES ($1, $2, $3, '1', $4, '{}', $5)",
        )
        .bind(line_id.to_string())
        .bind(sale_id.to_string())
        .bind(input.product_code.trim())
        .bind(response.principal.to_string())
        .bind(response.principal.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'SALE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id.to_string())
        .bind(sale_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(&input.currency)
        .bind(response.principal.to_string())
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok((response, event))
    }

    pub async fn activate_solar_contract_offline(
        &self,
        contract_id: Uuid,
        input: &ActivateSolarContractRequest,
    ) -> Result<(serde_json::Value, SyncEnvelope), sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        let row = sqlx::query(
            "SELECT r.customer_id, r.currency, r.principal_total, s.contract_id
             FROM receivables r JOIN sales s ON s.id = r.source_id
             WHERE r.id = $1 AND r.source_type = 'SALE' AND r.source_id = $2
               AND EXISTS (SELECT 1 FROM customers c WHERE c.id = r.customer_id AND c.organization_id = $3)",
        )
        .bind(input.receivable_id.to_string())
        .bind(input.sale_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("solar sale is not present locally".into()))?;
        let currency: String = row.try_get("currency")?;
        let principal = decimal_text(&row, "principal_total")?.round_dp(2);
        let existing_contract: Option<String> = row.try_get("contract_id")?;
        if existing_contract.is_some() {
            return Err(sqlx::Error::Protocol(
                "solar sale already has an installment contract".into(),
            ));
        }
        if currency != input.currency
            || input.schedule.is_empty()
            || input
                .schedule
                .iter()
                .any(|item| item.amount.is_sign_negative())
            || input
                .schedule
                .iter()
                .map(|item| item.amount)
                .sum::<Decimal>()
                .round_dp(2)
                != principal
        {
            return Err(sqlx::Error::Protocol(
                "solar schedule is invalid or does not equal the principal".into(),
            ));
        }
        let plan_id = Uuid::new_v4();
        let payload = serde_json::to_value(SolarContractSyncPayload {
            request: input.clone(),
            contract_id,
            installment_plan_id: plan_id,
        })
        .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contract JSON: {error}")))?;
        let event = local_event(
            self,
            Uuid::new_v4(),
            plan_id,
            "SOLAR_CONTRACT_ACTIVATED",
            payload,
        )?;
        let mut tx = self.pool.begin().await?;
        let customer_id: String = row.try_get("customer_id")?;
        sqlx::query(
            "INSERT INTO contracts
             (id, customer_id, contract_type, status, terms_revision, signed_at)
             VALUES ($1, $2, 'SOLAR_INSTALLMENT', 'LOCAL_PENDING', $3, date('now'))",
        )
        .bind(contract_id.to_string())
        .bind(customer_id)
        .bind(&input.approved_terms_revision)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE sales SET contract_id = $1 WHERE id = $2")
            .bind(contract_id.to_string())
            .bind(input.sale_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO installment_plans
             (id, receivable_id, contract_id, principal_amount, currency, start_date, status, terms_snapshot_json)
             VALUES ($1, $2, $3, $4, $5, date('now'), 'LOCAL_PENDING', $6)",
        )
        .bind(plan_id.to_string())
        .bind(input.receivable_id.to_string())
        .bind(contract_id.to_string())
        .bind(principal.to_string())
        .bind(&currency)
        .bind(json!({"terms_revision": &input.approved_terms_revision}).to_string())
        .execute(&mut *tx)
        .await?;
        for installment in &input.schedule {
            sqlx::query(
                "INSERT INTO installment_schedule
                 (id, plan_id, sequence, due_date, amount_due, schedule_status)
                 VALUES ($1, $2, $3, $4, $5, 'OPEN')",
            )
            .bind(solar_schedule_id(plan_id, installment.sequence).to_string())
            .bind(plan_id.to_string())
            .bind(installment.sequence as i64)
            .bind(installment.due_date.to_string())
            .bind(installment.amount.round_dp(2).to_string())
            .execute(&mut *tx)
            .await?;
        }
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok((
            json!({
                "status": "LOCAL_PENDING",
                "sale_id": input.sale_id,
                "receivable_id": input.receivable_id,
                "installment_plan_id": plan_id,
                "contract_id": contract_id,
                "principal": principal,
                "currency": currency,
                "terms_revision": &input.approved_terms_revision,
                "schedule": &input.schedule,
            }),
            event,
        ))
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
            "SELECT r.id, r.customer_id, r.currency, r.principal_total, r.status
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
        let receivable_status: String = row.try_get("status")?;
        let paid_rows = sqlx::query(
            "SELECT pa.allocated_amount, p.amount_received,
                    COALESCE((SELECT GROUP_CONCAT(pr.amount, ',') FROM payment_reversals pr
                              WHERE pr.payment_id = p.id), '') AS reversed_amounts
             FROM payment_allocations pa
             JOIN payments p ON p.id = pa.payment_id
             WHERE pa.receivable_id = $1 AND p.status IN ('LOCAL_PENDING', 'CENTRAL_ACCEPTED')",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let paid = paid_rows
            .into_iter()
            .try_fold(Decimal::ZERO, |total, row| {
                let allocation = decimal_text(&row, "allocated_amount")?;
                let received = decimal_text(&row, "amount_received")?;
                let reversed_text: String = row.try_get("reversed_amounts")?;
                let reversed = reversed_text
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .try_fold(Decimal::ZERO, |total, value| {
                        let amount = Decimal::from_str(value).map_err(|error| {
                            sqlx::Error::Protocol(format!("invalid reversed amount: {error}"))
                        })?;
                        Ok::<Decimal, sqlx::Error>(total + amount)
                    })?;
                let net_received = (received - reversed).max(Decimal::ZERO);
                let effective = if received.is_zero() {
                    Decimal::ZERO
                } else {
                    allocation * net_received / received
                };
                Ok::<Decimal, sqlx::Error>(total + effective)
            })?;
        let rejected = receivable_status == "REJECTED";
        let balance = if rejected {
            Decimal::ZERO
        } else {
            principal - paid
        };
        let overdue_schedule_rows = sqlx::query(
            "SELECT s.id, s.amount_due
             FROM installment_schedule s
             JOIN installment_plans ip ON ip.id = s.plan_id
             WHERE ip.receivable_id = $1 AND s.due_date < date('now')",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut overdue = Decimal::ZERO;
        for schedule_row in overdue_schedule_rows {
            let schedule_id: String = schedule_row.try_get("id")?;
            let amount_due = decimal_text(&schedule_row, "amount_due")?;
            let schedule_paid_rows = sqlx::query(
                "SELECT pa.allocated_amount, p.amount_received,
                        COALESCE((SELECT GROUP_CONCAT(pr.amount, ',')
                                  FROM payment_reversals pr WHERE pr.payment_id = p.id), '')
                         AS reversed_amounts
                 FROM payment_allocations pa
                 JOIN payments p ON p.id = pa.payment_id
                 WHERE pa.schedule_id = $1 AND p.status IN ('LOCAL_PENDING', 'CENTRAL_ACCEPTED')",
            )
            .bind(schedule_id)
            .fetch_all(&self.pool)
            .await?;
            let schedule_paid =
                schedule_paid_rows
                    .into_iter()
                    .try_fold(Decimal::ZERO, |total, payment_row| {
                        let allocation = decimal_text(&payment_row, "allocated_amount")?;
                        let received = decimal_text(&payment_row, "amount_received")?;
                        let reversed_text: String = payment_row.try_get("reversed_amounts")?;
                        let reversed = reversed_text
                            .split(',')
                            .filter(|value| !value.is_empty())
                            .try_fold(Decimal::ZERO, |total, value| {
                                let amount = Decimal::from_str(value).map_err(|error| {
                                    sqlx::Error::Protocol(format!(
                                        "invalid reversed amount: {error}"
                                    ))
                                })?;
                                Ok::<Decimal, sqlx::Error>(total + amount)
                            })?;
                        let net_received = (received - reversed).max(Decimal::ZERO);
                        let effective = if received.is_zero() {
                            Decimal::ZERO
                        } else {
                            allocation * net_received / received
                        };
                        Ok::<Decimal, sqlx::Error>(total + effective)
                    })?;
            overdue += (amount_due - schedule_paid).max(Decimal::ZERO);
        }
        let overdue = overdue.round_dp(2);
        Ok(Some(json!({
            "id": row.try_get::<String, _>("id")?,
            "customer_id": row.try_get::<String, _>("customer_id")?,
            "currency": row.try_get::<String, _>("currency")?,
            "principal": principal,
            "paid": paid,
            "balance": balance,
            "overdue_amount": overdue,
            "overdue": overdue > Decimal::ZERO,
            "status": if rejected {
                "REJECTED"
            } else if paid >= principal {
                "SETTLED"
            } else if overdue > Decimal::ZERO {
                "OVERDUE"
            } else {
                "OPEN"
            }
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
        let mut persisted = std::collections::BTreeMap::<(Uuid, Option<Uuid>), Decimal>::new();
        for allocation in &input.allocations {
            if allocation.amount <= Decimal::ZERO {
                return Err(LocalPaymentError::InvalidAllocation);
            }
            *grouped
                .entry(allocation.receivable_id)
                .or_insert(Decimal::ZERO) += allocation.amount;
            *persisted
                .entry((allocation.receivable_id, allocation.schedule_id))
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
            for ((_, schedule_id), _) in persisted.iter().filter(|((id, _), _)| id == receivable_id)
            {
                if let Some(schedule_id) = schedule_id {
                    let schedule_exists: Option<String> = sqlx::query_scalar(
                        "SELECT s.id FROM installment_schedule s
                         JOIN installment_plans p ON p.id = s.plan_id
                         WHERE s.id = $1 AND p.receivable_id = $2",
                    )
                    .bind(schedule_id.to_string())
                    .bind(receivable_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await?;
                    if schedule_exists.is_none() {
                        return Err(LocalPaymentError::ContextInvalid);
                    }
                }
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
        for ((receivable_id, schedule_id), amount) in persisted {
            sqlx::query(
                "INSERT INTO payment_allocations
                 (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(payment_id.to_string())
            .bind(receivable_id.to_string())
            .bind(schedule_id.map(|id| id.to_string()))
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

    pub async fn reverse_payment(
        &self,
        payment_id: Uuid,
        input: &ReversePaymentRequest,
    ) -> Result<(serde_json::Value, SyncEnvelope), LocalPaymentReverseError> {
        let _guard = self.write_lock.lock().await;
        if input.amount <= Decimal::ZERO {
            return Err(LocalPaymentReverseError::InvalidAmount);
        }
        if input.reason.trim().is_empty() {
            return Err(LocalPaymentReverseError::ReasonRequired);
        }
        let reversal_id = Uuid::new_v4();
        let payload = serde_json::to_value(PaymentReversalSyncPayload {
            payment_id,
            reversal_id,
            amount: input.amount,
            reason: input.reason.trim().to_owned(),
            actor_id: self.actor_id,
        })
        .map_err(|error| {
            LocalPaymentReverseError::Database(sqlx::Error::Protocol(format!(
                "invalid payment reversal JSON: {error}"
            )))
        })?;
        let event = local_event(
            self,
            Uuid::new_v4(),
            payment_id,
            "PAYMENT_REVERSED",
            payload,
        )?;
        let mut tx = self.pool.begin().await?;
        let payment = sqlx::query("SELECT amount_received, status FROM payments WHERE id = $1")
            .bind(payment_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(LocalPaymentReverseError::PaymentNotFound)?;
        let status: String = payment.try_get("status")?;
        if status != "LOCAL_PENDING" && status != "CENTRAL_ACCEPTED" {
            return Err(LocalPaymentReverseError::PaymentNotReversible);
        }
        let amount_received = decimal_text(&payment, "amount_received")?;
        let reversed_text: String = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), '0') FROM payment_reversals WHERE payment_id = $1",
        )
        .bind(payment_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        let reversed = Decimal::from_str(&reversed_text)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid reversed amount: {error}")))?;
        let remaining = amount_received - reversed;
        if input.amount > remaining {
            return Err(LocalPaymentReverseError::AmountExceedsRemaining);
        }
        sqlx::query(
            "INSERT INTO payment_reversals
             (id, payment_id, amount, reason, initiated_by, created_at)
             VALUES ($1, $2, $3, $4, $5, datetime('now'))",
        )
        .bind(reversal_id.to_string())
        .bind(payment_id.to_string())
        .bind(input.amount.to_string())
        .bind(input.reason.trim())
        .bind(self.actor_id.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, device, event_type, subject_id, occurred_at, redacted_metadata_json)
             VALUES ($1, $2, $3, 'PAYMENT_REVERSED', $4, datetime('now'), $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(self.actor_id.to_string())
        .bind(self.device_id.to_string())
        .bind(payment_id.to_string())
        .bind(
            json!({
                "reversal_id": reversal_id,
                "amount": input.amount,
                "reason": input.reason.trim()
            })
            .to_string(),
        )
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok((
            json!({
                "reversal_id": reversal_id,
                "payment_id": payment_id,
                "amount": format!("{:.2}", input.amount),
                "remaining_reversible_amount": format!("{:.2}", remaining - input.amount),
                "status": "LOCAL_PENDING",
                "reversal_status": "RECORDED"
            }),
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
                        if change_json
                            .get("status")
                            .and_then(serde_json::Value::as_str)
                            == Some("VOID")
                        {
                            apply_invoice_void_change(
                                &mut tx,
                                self.organization_id,
                                entity_id,
                                revision,
                                &change_json,
                            )
                            .await?;
                        } else {
                            apply_invoice_change(
                                &mut tx,
                                self.organization_id,
                                entity_id,
                                revision,
                                &change_json,
                            )
                            .await?;
                        }
                    }
                    "payments" => {
                        apply_payment_change(&mut tx, self.organization_id, &change_json).await?;
                    }
                    "payment_reversals" => {
                        apply_payment_reversal_change(&mut tx, self.organization_id, &change_json)
                            .await?;
                    }
                    "sales" => {
                        apply_solar_sale_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    "installment_plans" => {
                        apply_solar_contract_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    _ => {
                        record_unsupported_pull_conflict(
                            &mut tx,
                            sequence,
                            entity,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
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
        if state == "CONFLICT" || state == "REJECTED" {
            mark_local_effect_rejected(&mut tx, event_id.to_string()).await?;
        }
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

async fn mark_local_effect_rejected(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event_id: String,
) -> Result<(), sqlx::Error> {
    let journal =
        sqlx::query("SELECT aggregate_id, command_type FROM command_journal WHERE event_id = $1")
            .bind(&event_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(journal) = journal else {
        return Ok(());
    };
    let aggregate_id: String = journal.try_get("aggregate_id")?;
    let command_type: String = journal.try_get("command_type")?;
    match command_type.as_str() {
        "PAYMENT_RECORDED" => {
            sqlx::query("UPDATE payments SET status = 'REJECTED' WHERE id = $1")
                .bind(&aggregate_id)
                .execute(&mut **tx)
                .await?;
        }
        "UTILITY_INVOICE_ISSUED" => {
            sqlx::query("UPDATE invoices SET status = 'REJECTED' WHERE id = $1")
                .bind(&aggregate_id)
                .execute(&mut **tx)
                .await?;
            sqlx::query(
                "UPDATE receivables SET status = 'REJECTED'
                 WHERE source_type = 'INVOICE' AND source_id = $1",
            )
            .bind(&aggregate_id)
            .execute(&mut **tx)
            .await?;
        }
        "SOLAR_SALE_CREATED" => {
            sqlx::query("UPDATE sales SET status = 'REJECTED' WHERE id = $1")
                .bind(&aggregate_id)
                .execute(&mut **tx)
                .await?;
            sqlx::query(
                "UPDATE receivables SET status = 'REJECTED'
                 WHERE source_type = 'SALE' AND source_id = $1",
            )
            .bind(&aggregate_id)
            .execute(&mut **tx)
            .await?;
        }
        "SOLAR_CONTRACT_ACTIVATED" => {
            sqlx::query("UPDATE installment_plans SET status = 'REJECTED' WHERE id = $1")
                .bind(&aggregate_id)
                .execute(&mut **tx)
                .await?;
        }
        _ => {}
    }
    Ok(())
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
    let expected_hash = utility_snapshot_sha256(&response)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice response: {error}")))?;
    if response.snapshot_sha256 != expected_hash {
        return Err(sqlx::Error::Protocol(
            "invoice sync snapshot hash is invalid".into(),
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
        let existing_response: UtilityBillingResponse = serde_json::from_str(&existing_snapshot)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}")))?;
        let existing_hash = utility_snapshot_sha256(&existing_response)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}")))?;
        if existing_customer != request.customer_id.to_string() || existing_hash != expected_hash {
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

async fn apply_invoice_void_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let invoice_id = change
        .get("invoice_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(entity_id);
    if invoice_id != entity_id {
        return Err(sqlx::Error::Protocol(
            "invoice void entity id does not match invoice id".into(),
        ));
    }
    let void_id = change
        .get("void_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("invoice void id is missing".into()))?;
    let actor_id = change
        .get("actor_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("invoice void actor is missing".into()))?;
    let reason = change
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .ok_or_else(|| sqlx::Error::Protocol("invoice void reason is missing".into()))?;
    let Some(invoice) = sqlx::query(
        "SELECT status FROM invoices WHERE id = $1 AND customer_id IN
         (SELECT id FROM customers WHERE organization_id = $2)",
    )
    .bind(invoice_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Err(sqlx::Error::Protocol(
            "invoice void projection has no local invoice".into(),
        ));
    };
    let current_status: String = invoice.try_get("status")?;
    if current_status == "VOID" {
        return Ok(());
    }
    sqlx::query("UPDATE invoices SET status = 'VOID', version = MAX(version, $1) WHERE id = $2")
        .bind(revision)
        .bind(invoice_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "UPDATE receivables SET status = 'VOID'
         WHERE source_type = 'INVOICE' AND source_id = $1",
    )
    .bind(invoice_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO invoice_voids (id, invoice_id, reason, actor_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT(invoice_id) DO NOTHING",
    )
    .bind(void_id)
    .bind(invoice_id)
    .bind(reason.trim())
    .bind(actor_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_solar_sale_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let response: SolarSaleResponse = serde_json::from_value(change.clone()).map_err(|error| {
        sqlx::Error::Protocol(format!("invalid solar sale projection: {error}"))
    })?;
    if response.sale_id.to_string() != entity_id {
        return Err(sqlx::Error::Protocol(
            "solar sale entity id does not match response id".into(),
        ));
    }
    let customer_exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
            .bind(response.customer_id.to_string())
            .bind(organization_id.to_string())
            .fetch_optional(&mut **tx)
            .await?;
    if customer_exists.is_none() {
        return Err(sqlx::Error::Protocol(
            "solar sale customer is not present locally".into(),
        ));
    }
    if let Some(existing) =
        sqlx::query("SELECT customer_id, currency, status FROM sales WHERE id = $1")
            .bind(entity_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        let customer_id: String = existing.try_get("customer_id")?;
        let currency: String = existing.try_get("currency")?;
        if customer_id != response.customer_id.to_string() || currency != response.currency {
            return Err(sqlx::Error::Protocol(
                "conflicting local solar sale projection".into(),
            ));
        }
        sqlx::query("UPDATE sales SET status = $1 WHERE id = $2")
            .bind(&response.status)
            .bind(entity_id)
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO sales
             (id, customer_id, currency, sold_at, sale_type, status)
             VALUES ($1, $2, $3, datetime('now'), 'SOLAR_KIT', $4)",
        )
        .bind(entity_id)
        .bind(response.customer_id.to_string())
        .bind(&response.currency)
        .bind(&response.status)
        .execute(&mut **tx)
        .await?;
    }

    if let Some(existing) = sqlx::query(
        "SELECT id, customer_id, currency, principal_total FROM receivables
         WHERE source_type = 'SALE' AND source_id = $1",
    )
    .bind(entity_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let existing_id: String = existing.try_get("id")?;
        let existing_customer: String = existing.try_get("customer_id")?;
        let existing_currency: String = existing.try_get("currency")?;
        let existing_principal: String = existing.try_get("principal_total")?;
        if existing_id != response.receivable_id.to_string()
            || existing_customer != response.customer_id.to_string()
            || existing_currency != response.currency
            || Decimal::from_str(&existing_principal)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
                != response.principal
        {
            return Err(sqlx::Error::Protocol(
                "conflicting local solar receivable projection".into(),
            ));
        }
    } else {
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'SALE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(response.receivable_id.to_string())
        .bind(entity_id)
        .bind(response.customer_id.to_string())
        .bind(&response.currency)
        .bind(response.principal.to_string())
        .execute(&mut **tx)
        .await?;
    }

    if let Some(product_code) = response.product_code.as_deref() {
        if let Some(existing) = sqlx::query(
            "SELECT quantity, unit_price, total FROM sale_lines
             WHERE sale_id = $1 AND product_ref = $2",
        )
        .bind(entity_id)
        .bind(product_code)
        .fetch_optional(&mut **tx)
        .await?
        {
            let quantity: String = existing.try_get("quantity")?;
            let unit_price: String = existing.try_get("unit_price")?;
            let total: String = existing.try_get("total")?;
            if quantity != "1"
                || Decimal::from_str(&unit_price)
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
                    != response.principal
                || Decimal::from_str(&total)
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
                    != response.principal
            {
                return Err(sqlx::Error::Protocol(
                    "conflicting local solar sale line projection".into(),
                ));
            }
        } else {
            let line_id = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("SEL:SOLAR_SALE_LINE:{entity_id}:{product_code}").as_bytes(),
            );
            sqlx::query(
                "INSERT INTO sale_lines
                 (id, sale_id, product_ref, quantity, unit_price, tax_snapshot_json, total)
                 VALUES ($1, $2, $3, '1', $4, '{}', $5)",
            )
            .bind(line_id.to_string())
            .bind(entity_id)
            .bind(product_code)
            .bind(response.principal.to_string())
            .bind(response.principal.to_string())
            .execute(&mut **tx)
            .await?;
        }
    }
    let _ = revision;
    Ok(())
}

async fn apply_solar_contract_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let plan_id: Uuid = entity_id
        .parse()
        .map_err(|error| sqlx::Error::Protocol(format!("invalid solar plan id: {error}")))?;
    let sale_id: Uuid = serde_json::from_value(
        change
            .get("sale_id")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("solar contract sale id is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contract sale id: {error}")))?;
    let receivable_id: Uuid =
        serde_json::from_value(change.get("receivable_id").cloned().ok_or_else(|| {
            sqlx::Error::Protocol("solar contract receivable id is missing".into())
        })?)
        .map_err(|error| {
            sqlx::Error::Protocol(format!("invalid solar contract receivable id: {error}"))
        })?;
    let contract_id: Uuid = serde_json::from_value(
        change
            .get("contract_id")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("solar contract id is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contract id: {error}")))?;
    let response_plan_id: Uuid = serde_json::from_value(
        change
            .get("installment_plan_id")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("solar plan id is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid solar plan id: {error}")))?;
    if response_plan_id != plan_id {
        return Err(sqlx::Error::Protocol(
            "solar contract entity id does not match plan id".into(),
        ));
    }
    let principal: Decimal = serde_json::from_value(
        change
            .get("principal")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("solar contract principal is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contract principal: {error}")))?;
    let currency = change
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("solar contract currency is missing".into()))?;
    let terms_revision = change
        .get("terms_revision")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("LEGACY_UNSPECIFIED");
    let installments: Vec<sel_contracts::InstallmentRequest> = serde_json::from_value(
        change
            .get("schedule")
            .cloned()
            .ok_or_else(|| sqlx::Error::Protocol("solar contract schedule is missing".into()))?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contract schedule: {error}")))?;
    let context = sqlx::query(
        "SELECT r.customer_id, r.currency, r.principal_total, s.contract_id
         FROM receivables r JOIN sales s ON s.id = r.source_id
         JOIN customers c ON c.id = r.customer_id
         WHERE r.id = $1 AND r.source_type = 'SALE' AND r.source_id = $2
           AND c.organization_id = $3",
    )
    .bind(receivable_id.to_string())
    .bind(sale_id.to_string())
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("solar contract sale is not projected".into()))?;
    let customer_id: String = context.try_get("customer_id")?;
    let stored_currency: String = context.try_get("currency")?;
    let stored_principal: String = context.try_get("principal_total")?;
    let existing_contract: Option<String> = context.try_get("contract_id")?;
    if stored_currency != currency
        || Decimal::from_str(&stored_principal)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
            != principal
    {
        return Err(sqlx::Error::Protocol(
            "conflicting local solar contract financial context".into(),
        ));
    }
    if existing_contract
        .as_deref()
        .is_some_and(|id| id != contract_id.to_string())
    {
        return Err(sqlx::Error::Protocol(
            "conflicting local solar contract id".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO contracts (id, customer_id, contract_type, status, terms_revision, signed_at)
         VALUES ($1, $2, 'SOLAR_INSTALLMENT', 'ACTIVE', $3, date('now'))
         ON CONFLICT(id) DO UPDATE SET status = excluded.status,
           terms_revision = excluded.terms_revision",
    )
    .bind(contract_id.to_string())
    .bind(&customer_id)
    .bind(terms_revision)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE sales SET contract_id = $1
         WHERE id = $2 AND (contract_id IS NULL OR contract_id = $1)",
    )
    .bind(contract_id.to_string())
    .bind(sale_id.to_string())
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO installment_plans
         (id, receivable_id, contract_id, principal_amount, currency, start_date, status, terms_snapshot_json)
         VALUES ($1, $2, $3, $4, $5, date('now'), 'ACTIVE', $6)
         ON CONFLICT(id) DO UPDATE SET status = excluded.status,
           terms_snapshot_json = excluded.terms_snapshot_json",
    )
    .bind(plan_id.to_string())
    .bind(receivable_id.to_string())
    .bind(contract_id.to_string())
    .bind(principal.to_string())
    .bind(currency)
    .bind(json!({"terms_revision": terms_revision}).to_string())
    .execute(&mut **tx)
    .await?;
    for installment in installments {
        let schedule_id = solar_schedule_id(plan_id, installment.sequence).to_string();
        if let Some(existing) = sqlx::query(
            "SELECT id, due_date, amount_due FROM installment_schedule
             WHERE plan_id = $1 AND sequence = $2",
        )
        .bind(plan_id.to_string())
        .bind(installment.sequence as i64)
        .fetch_optional(&mut **tx)
        .await?
        {
            let existing_id: String = existing.try_get("id")?;
            let due_date: String = existing.try_get("due_date")?;
            let amount_due: String = existing.try_get("amount_due")?;
            if existing_id != schedule_id
                || due_date != installment.due_date.to_string()
                || Decimal::from_str(&amount_due)
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
                    != installment.amount
            {
                return Err(sqlx::Error::Protocol(
                    "conflicting local solar schedule projection".into(),
                ));
            }
        } else {
            sqlx::query(
                "INSERT INTO installment_schedule
                 (id, plan_id, sequence, due_date, amount_due, schedule_status)
                 VALUES ($1, $2, $3, $4, $5, 'OPEN')",
            )
            .bind(schedule_id)
            .bind(plan_id.to_string())
            .bind(installment.sequence as i64)
            .bind(installment.due_date.to_string())
            .bind(installment.amount.to_string())
            .execute(&mut **tx)
            .await?;
        }
    }
    let _ = revision;
    Ok(())
}

async fn record_unsupported_pull_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sequence: i64,
    entity: &str,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let conflict_id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("SEL:UNSUPPORTED_PULL:{sequence}:{entity}:{entity_id}").as_bytes(),
    );
    sqlx::query(
        "INSERT INTO sync_conflicts
         (id, event_id, reason, central_revision, payload_json, created_at)
         VALUES ($1, $2, $3, $4, $5, datetime('now'))
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(conflict_id.to_string())
    .bind(format!("CENTRAL_CHANGE:{sequence}"))
    .bind(format!("UNSUPPORTED_CENTRAL_ENTITY:{entity}:{entity_id}"))
    .bind(revision)
    .bind(change.to_string())
    .execute(&mut **tx)
    .await?;
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
            let schedule_id = allocation
                .get("schedule_id")
                .and_then(serde_json::Value::as_str);
            let already_present: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM payment_allocations
                 WHERE payment_id = $1 AND receivable_id = $2 AND allocated_amount = $3
                   AND currency = $4 AND (schedule_id = $5 OR (schedule_id IS NULL AND $5 IS NULL))",
            )
            .bind(payment_id)
            .bind(receivable_id)
            .bind(allocated_amount)
            .bind(currency)
            .bind(schedule_id)
            .fetch_one(&mut **tx)
            .await?;
            if already_present == 0 {
                sqlx::query(
                    "INSERT INTO payment_allocations
                     (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(payment_id)
                .bind(receivable_id)
                .bind(schedule_id)
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
        let schedule_id = allocation
            .get("schedule_id")
            .and_then(serde_json::Value::as_str);
        sqlx::query(
            "INSERT INTO payment_allocations
             (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(payment_id)
        .bind(receivable_id)
        .bind(schedule_id)
        .bind(allocated_amount)
        .bind(currency)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn apply_payment_reversal_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let reversal_id = change
        .get("reversal_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment reversal id is missing".into()))?;
    let payment_id = change
        .get("payment_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment reversal payment id is missing".into()))?;
    let amount = change
        .get("amount")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment reversal amount is missing".into()))?;
    let reason = change
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("payment reversal reason is missing".into()))?;
    let actor_id = change
        .get("actor_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("CENTRAL");
    let payment_exists: Option<String> = sqlx::query_scalar(
        "SELECT p.id FROM payments p
         JOIN customers c ON c.id = p.customer_id
         WHERE p.id = $1 AND c.organization_id = $2",
    )
    .bind(payment_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    if payment_exists.is_none() {
        return Err(sqlx::Error::Protocol(
            "payment reversal references a payment not present locally".into(),
        ));
    }
    if let Some(existing) =
        sqlx::query("SELECT amount, reason FROM payment_reversals WHERE id = $1")
            .bind(reversal_id)
            .fetch_optional(&mut **tx)
            .await?
    {
        let existing_amount: String = existing.try_get("amount")?;
        let existing_reason: String = existing.try_get("reason")?;
        if existing_amount != amount || existing_reason != reason {
            return Err(sqlx::Error::Protocol(
                "conflicting local payment reversal projection".into(),
            ));
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO payment_reversals
         (id, payment_id, amount, reason, initiated_by, created_at)
         VALUES ($1, $2, $3, $4, $5, datetime('now'))",
    )
    .bind(reversal_id)
    .bind(payment_id)
    .bind(amount)
    .bind(reason)
    .bind(actor_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO audit_events
         (id, actor, event_type, subject_id, occurred_at, redacted_metadata_json)
         VALUES ($1, $2, 'PAYMENT_REVERSED', $3, datetime('now'), $4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(actor_id)
    .bind(payment_id)
    .bind(change.to_string())
    .execute(&mut **tx)
    .await?;
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
