use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    catalog_snapshot_sha256, solar_schedule_id, utility_snapshot_sha256,
    ActivateSolarContractRequest, CatalogSnapshot, CorrectMeterReading, CreateCustomer,
    CreateMeter, CreateMeterReading, CreateServiceAccount, CreateSolarQuoteRequest,
    CreateSolarSaleRequest, CreditNoteRequest, CreditNoteSyncPayload, Customer,
    InvoiceIdentitySnapshot, InvoiceLineResponse, InvoiceVoidSyncPayload, Meter, MeterReading,
    MeterReadingPreview, PaymentAllocationRequest, PaymentAllocationSyncPayload,
    PaymentReversalSyncPayload, PaymentSyncPayload, RecordPaymentRequest, ReversePaymentRequest,
    ServiceAccount, ServiceAccountUpdateSyncPayload, SolarContractSyncPayload, SolarQuoteResponse,
    SolarSaleResponse, SolarSaleSyncPayload, SyncEnvelope, SyncEventResult, SyncPullResponse,
    UpdateCustomer, UpdateServiceAccount, UtilityBillingRequest, UtilityBillingResponse,
    UtilityInvoiceSyncPayload, UtilityMeterReadingRefs, CUSTOMER_PATCH_FORMAT_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
    Row, SqlitePool,
};
use std::{collections::BTreeMap, str::FromStr, sync::Arc};
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
pub enum LocalPaymentAllocationError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("payment was not found in the local database")]
    PaymentNotFound,
    #[error("payment is not accepted or pending locally")]
    PaymentNotAllocatable,
    #[error("payment allocations must be strictly positive")]
    InvalidAllocation,
    #[error("payment allocations exceed the received amount")]
    AllocationExceedsPayment,
    #[error("receivable context is invalid")]
    ContextInvalid,
    #[error("receivable balance would be overallocated")]
    Overallocated,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
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

#[derive(Debug, Error)]
pub enum LocalCreditNoteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("receivable was not found in the local database")]
    ReceivableNotFound,
    #[error("receivable cannot receive a credit in its current status")]
    ReceivableNotCreditable,
    #[error("credit amount is invalid")]
    InvalidAmount,
    #[error("credit amount exceeds the outstanding receivable balance")]
    AmountExceedsBalance,
    #[error("credit note reason is required")]
    ReasonRequired,
}

#[derive(Debug, Error)]
pub enum LocalSolarSaleError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("solar sale principal must be strictly positive")]
    InvalidAmount,
    #[error("solar sale customer is not present locally")]
    CustomerNotFound,
    #[error("solar product is not active in the local catalog")]
    ProductNotFound,
    #[error("invalid solar product price: {0}")]
    InvalidProductPrice(String),
    #[error("solar sale principal does not match the approved product price")]
    PriceMismatch,
    #[error("exactly one active approved solar VAT profile is required")]
    TaxProfileUnavailable,
    #[error("invalid solar sale JSON: {0}")]
    InvalidPayload(String),
}

#[derive(Debug, Error)]
pub enum LocalSolarQuoteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("solar product price is ambiguous for the current date")]
    ProductAmbiguous,
    #[error("exactly one active approved solar VAT profile is required")]
    TaxProfileUnavailable,
}

#[derive(Debug, Error)]
pub enum LocalMeterError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("meter command could not be serialized: {0}")]
    InvalidPayload(String),
    #[error("customer was not found locally")]
    CustomerNotFound,
    #[error("service account was not found locally")]
    ServiceAccountNotFound,
    #[error("meter was not found locally")]
    MeterNotFound,
    #[error("service code, connection status, or meter unit is invalid")]
    InvalidConfiguration,
    #[error("meter serial, period, or reading source is invalid")]
    InvalidInput,
    #[error("meter index must be non-negative and current must not be below previous")]
    InvalidIndex,
    #[error("a reading already exists for this meter and period")]
    DuplicateReading,
    #[error("the previous index does not match the latest accepted reading")]
    PreviousIndexMismatch,
    #[error("only the latest reading can be corrected")]
    ReadingNotLatest,
    #[error("a meter reading already used by an issued invoice cannot be corrected")]
    ReadingAlreadyInvoiced,
    #[error("selected meter readings must be current, unbilled, and match the customer, period, and service")]
    ReadingUnavailable,
    #[error("a correction reason is required")]
    CorrectionReasonRequired,
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
             (id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, 'ACTIVE', 1, datetime('now'))",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .bind(&input.customer_code)
        .bind(&input.legal_name)
        .bind(input.customer_category.as_deref().map(str::trim).map(str::to_ascii_uppercase))
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
            "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision
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
            "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision
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
        let address = input.address.apply_to(current_customer.address);
        let phone = input.phone.apply_to(current_customer.phone);
        let customer_category = input
            .customer_category
            .apply_to(current_customer.customer_category)
            .map(|value| value.trim().to_ascii_uppercase());
        let status = input
            .status
            .clone()
            .unwrap_or(current_customer.status.clone());
        sqlx::query(
            "UPDATE customers SET legal_name = $1, customer_category = $2,
             contact_fields_json = $3, status = $4, revision = revision + 1
             WHERE id = $5 AND organization_id = $6 AND revision = $7",
        )
        .bind(&legal_name)
        .bind(customer_category)
        .bind(json!({"address": address, "phone": phone}).to_string())
        .bind(&status)
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .bind(expected_revision)
        .execute(&mut *tx)
        .await?;
        let mut payload = serde_json::to_value(input)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid customer JSON: {error}")))?;
        payload["patch_format_version"] = json!(CUSTOMER_PATCH_FORMAT_VERSION);
        let event = local_event(self, Uuid::new_v4(), id, "CUSTOMER_UPDATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        let customer = load_customer_from_tx(&mut tx, id, self.organization_id).await?;
        tx.commit().await?;
        Ok(Some((customer, event)))
    }

    pub async fn list_customers(&self) -> Result<Vec<Customer>, sqlx::Error> {
        Ok(self
            .list_customers_page(None, None, None, i64::MAX)
            .await?
            .0)
    }

    pub async fn list_customers_page(
        &self,
        after_customer_code: Option<&str>,
        status: Option<&str>,
        search: Option<&str>,
        limit: i64,
    ) -> Result<(Vec<Customer>, Option<String>), sqlx::Error> {
        let fetch_limit = limit.max(1).saturating_add(1);
        let rows = if search.is_some() {
            sqlx::query(
                "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision
                 FROM customers
                 WHERE organization_id = $1
                   AND ($2 IS NULL OR customer_code > $2)
                   AND ($3 IS NULL OR status = $3)
                 ORDER BY customer_code",
            )
            .bind(self.organization_id.to_string())
            .bind(after_customer_code)
            .bind(status)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision
                 FROM customers
                 WHERE organization_id = $1
                   AND ($2 IS NULL OR customer_code > $2)
                   AND ($3 IS NULL OR status = $3)
                 ORDER BY customer_code
                 LIMIT $4",
            )
            .bind(self.organization_id.to_string())
            .bind(after_customer_code)
            .bind(status)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await?
        };
        let mut customers = rows
            .iter()
            .map(customer_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(search) = search {
            let search = search.to_lowercase();
            customers.retain(|customer| {
                customer.customer_code.to_lowercase().contains(&search)
                    || customer.legal_name.to_lowercase().contains(&search)
            });
        }
        let next_cursor = if customers.len() as i64 > limit.max(1) {
            customers
                .get(limit.max(1) as usize - 1)
                .map(|customer| customer.customer_code.clone())
        } else {
            None
        };
        customers.truncate(limit.max(1) as usize);
        Ok((customers, next_cursor))
    }

    pub async fn apply_catalog_snapshot(
        &self,
        snapshot: &CatalogSnapshot,
    ) -> Result<(), sqlx::Error> {
        if snapshot.payload.schema_version != 1
            || catalog_snapshot_sha256(&snapshot.payload) != snapshot.sha256
        {
            return Err(sqlx::Error::Protocol(
                "catalog snapshot schema or SHA-256 is invalid".into(),
            ));
        }
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        for product in &snapshot.payload.products {
            let status = required_catalog_text(product, "status")?;
            if !matches!(status.as_str(), "APPROVED" | "REVOKED") {
                return Err(sqlx::Error::Protocol(
                    "catalog snapshot contains a draft product".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO products
                 (id, sku, product_type, label, price_reference, currency, active,
                  effective_from, effective_to, status, approval_ref)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT(id) DO UPDATE SET sku=excluded.sku,
                   product_type=excluded.product_type, label=excluded.label,
                   price_reference=excluded.price_reference, currency=excluded.currency,
                   active=excluded.active, effective_from=excluded.effective_from,
                   effective_to=excluded.effective_to, status=excluded.status,
                   approval_ref=excluded.approval_ref",
            )
            .bind(required_catalog_text(product, "id")?)
            .bind(required_catalog_text(product, "sku")?)
            .bind(required_catalog_text(product, "product_type")?)
            .bind(required_catalog_text(product, "label")?)
            .bind(required_catalog_text(product, "price_reference")?)
            .bind(required_catalog_text(product, "currency")?)
            .bind(status == "APPROVED")
            .bind(required_catalog_text(product, "effective_from")?)
            .bind(optional_catalog_text(product, "effective_to"))
            .bind(&status)
            .bind(optional_catalog_text(product, "approval_ref"))
            .execute(&mut *tx)
            .await?;
        }
        for tariff in &snapshot.payload.tariffs {
            let status = required_catalog_text(tariff, "status")?;
            if !matches!(status.as_str(), "APPROVED" | "REVOKED") {
                return Err(sqlx::Error::Protocol(
                    "catalog snapshot contains a draft tariff".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO tariff_versions
                 (id, service_code, customer_category, unit, unit_price, currency,
                  effective_from, effective_to, approval_ref, approved_by, status)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT(id) DO UPDATE SET service_code=excluded.service_code,
                   customer_category=excluded.customer_category, unit=excluded.unit,
                   unit_price=excluded.unit_price, currency=excluded.currency,
                   effective_from=excluded.effective_from, effective_to=excluded.effective_to,
                   approval_ref=excluded.approval_ref, approved_by=excluded.approved_by,
                   status=excluded.status",
            )
            .bind(required_catalog_text(tariff, "id")?)
            .bind(required_catalog_text(tariff, "service_code")?)
            .bind(required_catalog_text(tariff, "customer_category")?)
            .bind(required_catalog_text(tariff, "unit")?)
            .bind(required_catalog_text(tariff, "unit_price")?)
            .bind(required_catalog_text(tariff, "currency")?)
            .bind(required_catalog_text(tariff, "effective_from")?)
            .bind(optional_catalog_text(tariff, "effective_to"))
            .bind(optional_catalog_text(tariff, "approval_ref").unwrap_or_default())
            .bind(optional_catalog_text(tariff, "approved_by"))
            .bind(&status)
            .execute(&mut *tx)
            .await?;
        }
        for profile in &snapshot.payload.tax_profiles {
            let status = required_catalog_text(profile, "status")?;
            if !matches!(status.as_str(), "APPROVED" | "REVOKED") {
                return Err(sqlx::Error::Protocol(
                    "catalog snapshot contains a draft tax profile".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO tax_profile_versions
                 (id, service_code, jurisdiction, base, rate, source, effective_from,
                  effective_to, status, approval_ref, approved_by)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT(id) DO UPDATE SET service_code=excluded.service_code,
                   jurisdiction=excluded.jurisdiction, base=excluded.base, rate=excluded.rate,
                   source=excluded.source, effective_from=excluded.effective_from,
                   effective_to=excluded.effective_to, status=excluded.status,
                   approval_ref=excluded.approval_ref, approved_by=excluded.approved_by",
            )
            .bind(required_catalog_text(profile, "id")?)
            .bind(required_catalog_text(profile, "service_code")?)
            .bind(required_catalog_text(profile, "jurisdiction")?)
            .bind(required_catalog_text(profile, "base")?)
            .bind(required_catalog_text(profile, "rate")?)
            .bind(required_catalog_text(profile, "source")?)
            .bind(required_catalog_text(profile, "effective_from")?)
            .bind(optional_catalog_text(profile, "effective_to"))
            .bind(&status)
            .bind(optional_catalog_text(profile, "approval_ref"))
            .bind(optional_catalog_text(profile, "approved_by"))
            .execute(&mut *tx)
            .await?;
        }
        for rate in &snapshot.payload.fx_rates {
            let status = required_catalog_text(rate, "status")?;
            if !matches!(status.as_str(), "APPROVED" | "REVOKED") {
                return Err(sqlx::Error::Protocol(
                    "catalog snapshot contains a draft FX rate".into(),
                ));
            }
            sqlx::query(
                "INSERT INTO fx_rate_versions
                 (id, from_currency, to_currency, value, effective_from, source, approved,
                  effective_to, approval_ref, approved_by, status)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT(id) DO UPDATE SET from_currency=excluded.from_currency,
                   to_currency=excluded.to_currency, value=excluded.value,
                   effective_from=excluded.effective_from, source=excluded.source,
                   approved=excluded.approved, effective_to=excluded.effective_to,
                   approval_ref=excluded.approval_ref, approved_by=excluded.approved_by,
                   status=excluded.status",
            )
            .bind(required_catalog_text(rate, "id")?)
            .bind(required_catalog_text(rate, "from_currency")?)
            .bind(required_catalog_text(rate, "to_currency")?)
            .bind(required_catalog_text(rate, "value")?)
            .bind(required_catalog_text(rate, "effective_from")?)
            .bind(required_catalog_text(rate, "source")?)
            .bind(status == "APPROVED")
            .bind(optional_catalog_text(rate, "effective_to"))
            .bind(optional_catalog_text(rate, "approval_ref"))
            .bind(optional_catalog_text(rate, "approved_by"))
            .bind(&status)
            .execute(&mut *tx)
            .await?;
        }
        for charge in &snapshot.payload.communication_charges {
            let status = required_catalog_text(charge, "status")?;
            if !matches!(status.as_str(), "APPROVED" | "REVOKED") {
                return Err(sqlx::Error::Protocol(
                    "catalog snapshot contains a draft communication charge".into(),
                ));
            }
            let payload = charge.get("payload").ok_or_else(|| {
                sqlx::Error::Protocol("communication charge payload is missing".into())
            })?;
            let currency = required_catalog_text(payload, "currency")?;
            let label = required_catalog_text(payload, "label")?;
            let amount = required_catalog_text(payload, "amount")?;
            amount.parse::<Decimal>().map_err(|_| {
                sqlx::Error::Protocol("communication charge amount is invalid".into())
            })?;
            sqlx::query(
                "INSERT INTO communication_charge_versions
                 (id, currency, label, amount, effective_from, effective_to, status, approval_ref)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT(id) DO UPDATE SET currency=excluded.currency,
                   label=excluded.label, amount=excluded.amount,
                   effective_from=excluded.effective_from, effective_to=excluded.effective_to,
                   status=excluded.status, approval_ref=excluded.approval_ref",
            )
            .bind(required_catalog_text(charge, "id")?)
            .bind(currency.to_ascii_uppercase())
            .bind(label)
            .bind(amount)
            .bind(required_catalog_text(charge, "effective_from")?)
            .bind(optional_catalog_text(charge, "effective_to"))
            .bind(&status)
            .bind(optional_catalog_text(charge, "approval_ref"))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("INSERT INTO catalog_sync_state (key,value,updated_at) VALUES ('sha256',$1,datetime('now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
            .bind(&snapshot.sha256).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn load_communication_charge(
        &self,
        currency: &str,
        on_date: NaiveDate,
    ) -> Result<Option<Decimal>, sqlx::Error> {
        let amount = sqlx::query_scalar::<_, String>(
            "SELECT amount FROM communication_charge_versions
             WHERE currency = $1 AND status = 'APPROVED' AND effective_from <= $2
               AND (effective_to IS NULL OR effective_to >= $2)
             ORDER BY effective_from DESC, id DESC LIMIT 1",
        )
        .bind(currency.trim().to_ascii_uppercase())
        .bind(on_date.to_string())
        .fetch_optional(&self.pool)
        .await?;
        amount
            .map(|value| {
                value.parse::<Decimal>().map_err(|_| {
                    sqlx::Error::Decode(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid communication charge amount",
                    )))
                })
            })
            .transpose()
    }

    pub async fn catalog_snapshot_hash(&self) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT value FROM catalog_sync_state WHERE key = 'sha256'")
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn load_tariffs(
        &self,
        revision: &str,
        currency: &str,
        period: &str,
        customer_category: &str,
    ) -> Result<LocalTariffSnapshot, sqlx::Error> {
        let customer_category = customer_category.trim().to_ascii_uppercase();
        if customer_category.is_empty() {
            return Err(sqlx::Error::Protocol(
                "customer category is required for tariff selection".into(),
            ));
        }
        let effective_date = NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
            .map_err(|error| sqlx::Error::Protocol(format!("invalid billing period: {error}")))?;
        let date = effective_date.to_string();
        let electricity = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'ELECTRICITY' AND unit = 'KWH' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED' AND customer_category = $3
               AND effective_from <= $4 AND (effective_to IS NULL OR effective_to >= $4)
             ORDER BY effective_from DESC",
        )
        .bind(currency)
        .bind(revision)
        .bind(&customer_category)
        .bind(&date)
        .fetch_all(&self.pool)
        .await?;
        if electricity.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "multiple applicable electricity tariffs exist for this category".into(),
            ));
        }
        let electricity_taxes = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'ELECTRICITY' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC, jurisdiction, base LIMIT 2",
        )
        .bind(&date)
        .fetch_all(&self.pool)
        .await?;
        if electricity_taxes.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "multiple applicable electricity tax profiles require an explicit SEL tax mapping"
                    .into(),
            ));
        }
        let water = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'WATER' AND unit = 'M3' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED' AND customer_category = $3
               AND effective_from <= $4 AND (effective_to IS NULL OR effective_to >= $4)
             ORDER BY effective_from DESC",
        )
        .bind(currency)
        .bind(revision)
        .bind(&customer_category)
        .bind(&date)
        .fetch_all(&self.pool)
        .await?;
        if water.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "multiple applicable water tariffs exist for this category".into(),
            ));
        }
        let water_taxes = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'WATER' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC, jurisdiction, base LIMIT 2",
        )
        .bind(&date)
        .fetch_all(&self.pool)
        .await?;
        if water_taxes.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "multiple applicable water tax profiles require an explicit SEL tax mapping".into(),
            ));
        }
        Ok(LocalTariffSnapshot {
            electricity_unit_price: sqlite_decimal_column(electricity.first(), "unit_price")?,
            electricity_tax_rate: sqlite_decimal_column(electricity_taxes.first(), "rate")?,
            water_unit_price: sqlite_decimal_column(water.first(), "unit_price")?,
            water_tax_rate: sqlite_decimal_column(water_taxes.first(), "rate")?,
        })
    }

    pub async fn issue_utility_invoice(
        &self,
        input: &UtilityBillingRequest,
        response: &UtilityBillingResponse,
    ) -> Result<SyncEnvelope, sqlx::Error> {
        let _guard = self.write_lock.lock().await;
        if response.billing_period != input.billing_period || response.due_date != input.due_date {
            return Err(sqlx::Error::Protocol(
                "invoice document metadata does not match the billing request".into(),
            ));
        }
        let receivable_id = response.receivable_id.ok_or_else(|| {
            sqlx::Error::Protocol("local utility invoice requires a receivable id".into())
        })?;
        let mut tx = self.pool.begin().await?;
        let identity = sqlx::query(
            "SELECT c.customer_code, c.legal_name, c.contact_fields_json, o.legal_name AS organization_name
             FROM customers c JOIN organizations o ON o.id = c.organization_id
             WHERE c.id = $1 AND c.organization_id = $2",
        )
        .bind(input.customer_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        let contacts: serde_json::Value = serde_json::from_str(
            &identity.try_get::<String, _>("contact_fields_json")?,
        )
        .map_err(|error| sqlx::Error::Protocol(format!("invalid customer contacts: {error}")))?;
        let identity_snapshot = InvoiceIdentitySnapshot {
            organization_name: identity.try_get("organization_name")?,
            customer_code: identity.try_get("customer_code")?,
            legal_name: identity.try_get("legal_name")?,
            address: contacts
                .get("address")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            phone: contacts
                .get("phone")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        };
        let payload = serde_json::to_value(UtilityInvoiceSyncPayload {
            request: input.clone(),
            invoice_id: response.invoice_id,
            receivable_id,
            identity_snapshot: Some(identity_snapshot.clone()),
        })
        .map_err(|error| sqlx::Error::Protocol(format!("invalid utility JSON: {error}")))?;
        let event = local_event(
            self,
            Uuid::new_v4(),
            response.invoice_id,
            "UTILITY_INVOICE_ISSUED",
            payload,
        )?;
        if let Some(refs) = &input.meter_readings {
            let quantities = load_invoice_meter_readings(
                &mut tx,
                self.organization_id,
                input.customer_id,
                &input.billing_period,
                refs,
            )
            .await?;
            if response_line_quantity(response, "ELECTRICITY") != Some(quantities.0)
                || response_line_quantity(response, "WATER") != Some(quantities.1)
                || input.electricity.quantity != quantities.0
                || input.water.quantity != quantities.1
            {
                return Err(sqlx::Error::Protocol(
                    "invoice quantities do not match the selected meter readings".into(),
                ));
            }
        }
        sqlx::query(
            "INSERT INTO invoices
             (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot_json, status, version, document_snapshot_json)
             VALUES ($1, $2, $3, datetime('now'), $4, $5, 'v2-a4-2', $6, 'LOCAL_PENDING', 1, $7)",
        )
        .bind(response.invoice_id.to_string())
        .bind(input.customer_id.to_string())
        .bind(&input.billing_period)
        .bind(&response.currency)
        .bind(response.total.to_string())
        .bind(serde_json::to_string(response).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid invoice snapshot JSON: {error}"))
        })?)
        .bind(serde_json::to_string(&identity_snapshot).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid invoice identity snapshot JSON: {error}"))
        })?)
        .execute(&mut *tx)
        .await?;
        for line in &response.lines {
            insert_invoice_line(&mut tx, response.invoice_id, line).await?;
        }
        if let Some(refs) = &input.meter_readings {
            sqlx::query(
                "INSERT INTO invoice_meter_readings (invoice_id, meter_reading_id, service_code)
                 VALUES ($1, $2, 'ELECTRICITY'), ($1, $3, 'WATER')",
            )
            .bind(response.invoice_id.to_string())
            .bind(refs.electricity_reading_id.to_string())
            .bind(refs.water_reading_id.to_string())
            .execute(&mut *tx)
            .await?;
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
            "INSERT INTO invoice_voids (id, invoice_id, reason, actor_id, created_at)
             VALUES ($1, $2, $3, $4, datetime('now'))",
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
    ) -> Result<(SolarSaleResponse, SyncEnvelope), LocalSolarSaleError> {
        let _guard = self.write_lock.lock().await;
        if input.principal <= Decimal::ZERO {
            return Err(LocalSolarSaleError::InvalidAmount);
        }
        let customer_exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
                .bind(input.customer_id.to_string())
                .bind(self.organization_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        if customer_exists.is_none() {
            return Err(LocalSolarSaleError::CustomerNotFound);
        }
        let product_rows = sqlx::query(
            "SELECT price_reference FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT' AND currency = $2
               AND active = 1 AND status = 'APPROVED'
               AND effective_from <= date('now')
               AND (effective_to IS NULL OR effective_to >= date('now'))
             ORDER BY effective_from DESC, id LIMIT 2",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_all(&self.pool)
        .await?;
        let Some(product_row) = product_rows.first() else {
            return Err(LocalSolarSaleError::ProductNotFound);
        };
        if product_rows.len() != 1 {
            return Err(LocalSolarSaleError::ProductNotFound);
        }
        let subtotal = Decimal::from_str(&product_row.try_get::<String, _>("price_reference")?)
            .map_err(|error| LocalSolarSaleError::InvalidProductPrice(error.to_string()))?;
        let tax_rows = sqlx::query(
            "SELECT id, jurisdiction, base, rate, source, approval_ref
             FROM tax_profile_versions
             WHERE service_code = 'SOLAR_KIT' AND status = 'APPROVED'
               AND effective_from <= date('now')
               AND (effective_to IS NULL OR effective_to >= date('now'))
             ORDER BY effective_from DESC, jurisdiction, base, id LIMIT 2",
        )
        .fetch_all(&self.pool)
        .await?;
        if tax_rows.len() != 1 {
            return Err(LocalSolarSaleError::TaxProfileUnavailable);
        }
        let tax_row = &tax_rows[0];
        let tax_base: String = tax_row.try_get("base")?;
        if !matches!(tax_base.as_str(), "SUBTOTAL" | "SERVICE") {
            return Err(LocalSolarSaleError::TaxProfileUnavailable);
        }
        let tax_rate = Decimal::from_str(&tax_row.try_get::<String, _>("rate")?)
            .map_err(|error| LocalSolarSaleError::InvalidProductPrice(error.to_string()))?
            .normalize();
        let subtotal = subtotal.round_dp(2);
        let tax_amount = (subtotal * tax_rate).round_dp(2);
        let total = subtotal + tax_amount;
        if input.principal.round_dp(2) != total {
            return Err(LocalSolarSaleError::PriceMismatch);
        }
        let sale_id = Uuid::new_v4();
        let receivable_id = Uuid::new_v4();
        let response = SolarSaleResponse {
            sale_id,
            receivable_id,
            customer_id: input.customer_id,
            subtotal,
            tax_rate,
            tax_amount,
            total,
            principal: total,
            currency: input.currency.clone(),
            status: "LOCAL_PENDING".into(),
            product_code: Some(input.product_code.trim().to_owned()),
        };
        let payload = serde_json::to_value(SolarSaleSyncPayload {
            request: input.clone(),
            sale_id,
            receivable_id,
        })
        .map_err(|error| LocalSolarSaleError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), sale_id, "SOLAR_SALE_CREATED", payload)?;
        let tax_snapshot = serde_json::to_string(&sel_contracts::SolarTaxSnapshot {
            schema_version: 1,
            service_code: "SOLAR_KIT".into(),
            tax_profile_id: Uuid::parse_str(&tax_row.try_get::<String, _>("id")?)
                .map_err(|error| LocalSolarSaleError::InvalidPayload(error.to_string()))?,
            jurisdiction: tax_row.try_get("jurisdiction")?,
            base: tax_base,
            rate: tax_rate,
            amount: tax_amount,
            source: tax_row.try_get("source")?,
            approval_ref: tax_row.try_get("approval_ref")?,
        })
        .map_err(|error| LocalSolarSaleError::InvalidPayload(error.to_string()))?;
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
             VALUES ($1, $2, $3, '1', $4, $5, $6)",
        )
        .bind(line_id.to_string())
        .bind(sale_id.to_string())
        .bind(input.product_code.trim())
        .bind(subtotal.to_string())
        .bind(tax_snapshot)
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

    pub async fn create_solar_quote(
        &self,
        input: &CreateSolarQuoteRequest,
    ) -> Result<Option<SolarQuoteResponse>, LocalSolarQuoteError> {
        let product_rows = sqlx::query(
            "SELECT price_reference FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT'
               AND currency = $2 AND active = 1 AND status = 'APPROVED'
               AND effective_from <= date('now')
               AND (effective_to IS NULL OR effective_to >= date('now'))
             ORDER BY effective_from DESC, id LIMIT 2",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_all(&self.pool)
        .await?;
        let Some(product_row) = product_rows.first() else {
            return Ok(None);
        };
        if product_rows.len() != 1 {
            return Err(LocalSolarQuoteError::ProductAmbiguous);
        }
        let subtotal = Decimal::from_str(&product_row.try_get::<String, _>("price_reference")?)
            .map_err(|error| {
                sqlx::Error::Protocol(format!("invalid solar product price: {error}"))
            })?;
        let taxes = sqlx::query(
            "SELECT rate, base FROM tax_profile_versions
             WHERE service_code = 'SOLAR_KIT' AND status = 'APPROVED'
               AND effective_from <= date('now')
               AND (effective_to IS NULL OR effective_to >= date('now'))
             ORDER BY effective_from DESC, jurisdiction, base, id LIMIT 2",
        )
        .fetch_all(&self.pool)
        .await?;
        if taxes.len() != 1
            || !matches!(
                taxes[0].try_get::<String, _>("base")?.as_str(),
                "SUBTOTAL" | "SERVICE"
            )
        {
            return Err(LocalSolarQuoteError::TaxProfileUnavailable);
        }
        let tax_rate = Decimal::from_str(&taxes[0].try_get::<String, _>("rate")?)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid solar VAT rate: {error}")))?
            .normalize();
        let subtotal = subtotal.round_dp(2);
        let tax_amount = (subtotal * tax_rate).round_dp(2);
        let total = subtotal + tax_amount;
        Ok(Some(SolarQuoteResponse {
            product_code: input.product_code.trim().to_owned(),
            subtotal,
            tax_rate,
            tax_amount,
            total,
            principal: total,
            currency: input.currency.clone(),
            terms_revision: input.terms_revision.clone(),
            status: "QUOTED_FROM_APPROVED_CATALOG".into(),
        }))
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
            "SELECT immutable_snapshot_json, status, period FROM invoices
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
            let period: String = row.try_get("period")?;
            response.status = status;
            if response.billing_period.is_empty() {
                response.billing_period = period;
            }
            Ok(response)
        })
        .transpose()
    }

    pub async fn get_invoice_identity_snapshot(
        &self,
        id: Uuid,
    ) -> Result<Option<InvoiceIdentitySnapshot>, sqlx::Error> {
        let snapshot: Option<String> = sqlx::query_scalar(
            "SELECT document_snapshot_json FROM invoices
             WHERE id = $1 AND customer_id IN
               (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        snapshot
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice identity snapshot: {error}"))
                })
            })
            .transpose()
    }

    pub async fn get_invoice_issued_at(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT issued_at FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_invoice_responses(
        &self,
        customer_id: Option<Uuid>,
        period: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT i.id, i.invoice_number, i.period, i.issued_at
             FROM invoices i
             JOIN customers c ON c.id = i.customer_id
             WHERE c.organization_id = $1
               AND ($2 IS NULL OR i.customer_id = $2)
               AND ($3 IS NULL OR i.period = $3)
               AND ($4 IS NULL OR i.status = $4)
             ORDER BY i.issued_at DESC, i.id DESC
             LIMIT $5",
        )
        .bind(self.organization_id.to_string())
        .bind(customer_id.map(|id| id.to_string()))
        .bind(period)
        .bind(status)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(&self.pool)
        .await?;
        let mut invoices = Vec::with_capacity(rows.len());
        for row in rows {
            let id = parse_uuid_column(&row, "id", "invoice")?;
            let Some(response) = self.get_invoice_response(id).await? else {
                continue;
            };
            let mut value = serde_json::to_value(response).map_err(|error| {
                sqlx::Error::Protocol(format!("invalid invoice response JSON: {error}"))
            })?;
            let object = value.as_object_mut().ok_or_else(|| {
                sqlx::Error::Protocol("invoice response is not a JSON object".into())
            })?;
            object.insert(
                "invoice_number".into(),
                row.try_get::<Option<String>, _>("invoice_number")
                    .map(serde_json::Value::from)?,
            );
            object.insert(
                "period".into(),
                row.try_get::<Option<String>, _>("period")
                    .map(serde_json::Value::from)?,
            );
            object.insert(
                "issued_at".into(),
                serde_json::Value::String(row.try_get("issued_at")?),
            );
            invoices.push(value);
        }
        Ok(invoices)
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
        let credited_text: String = sqlx::query_scalar(
            "SELECT COALESCE(GROUP_CONCAT(amount, ','), '')
             FROM credit_notes WHERE receivable_id = $1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let credited = credited_text
            .split(',')
            .filter(|value| !value.is_empty())
            .try_fold(Decimal::ZERO, |total, value| {
                let amount = Decimal::from_str(value).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid credit amount: {error}"))
                })?;
                Ok::<Decimal, sqlx::Error>(total + amount)
            })?;
        let rejected = receivable_status == "REJECTED";
        let balance = if rejected {
            Decimal::ZERO
        } else {
            principal - paid - credited
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
            overdue += (amount_due - schedule_paid).max(Decimal::ZERO);
        }
        let overdue = overdue.round_dp(2);
        Ok(Some(json!({
            "id": row.try_get::<String, _>("id")?,
            "customer_id": row.try_get::<String, _>("customer_id")?,
            "currency": row.try_get::<String, _>("currency")?,
            "principal": principal,
            "paid": paid,
            "credited": credited,
            "balance": balance,
            "overdue_amount": overdue,
            "overdue": overdue > Decimal::ZERO,
            "status": if rejected {
                "REJECTED"
            } else if paid + credited >= principal {
                "SETTLED"
            } else if receivable_status == "CREDITED" {
                "CREDITED"
            } else if overdue > Decimal::ZERO {
                "OVERDUE"
            } else {
                "OPEN"
            }
        })))
    }

    pub async fn customer_statement(
        &self,
        customer_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let customer_exists = sqlx::query(
            "SELECT id FROM customers
             WHERE id = $1 AND organization_id = $2",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .is_some();
        if !customer_exists {
            return Ok(None);
        }

        let rows = sqlx::query(
            "SELECT id FROM receivables
             WHERE customer_id = $1 ORDER BY id",
        )
        .bind(customer_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut receivables = Vec::with_capacity(rows.len());
        for row in rows {
            let receivable_id: String = row.try_get("id")?;
            let receivable_id = receivable_id.parse::<Uuid>().map_err(|error| {
                sqlx::Error::Protocol(format!("invalid receivable id in statement: {error}"))
            })?;
            if let Some(summary) = self.get_receivable_summary(receivable_id).await? {
                receivables.push(summary);
            }
        }
        let invoice_rows = sqlx::query(
            "SELECT i.id, i.invoice_number, i.period, i.issued_at, i.status,
                    i.immutable_snapshot_json
             FROM invoices i JOIN customers c ON c.id = i.customer_id
             WHERE i.customer_id = $1 AND c.organization_id = $2
             ORDER BY i.issued_at DESC, i.id DESC LIMIT 100",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let invoices = invoice_rows
            .iter()
            .map(|row| {
                let snapshot: String = row.try_get("immutable_snapshot_json")?;
                let mut response: UtilityBillingResponse = serde_json::from_str(&snapshot)
                    .map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
                    })?;
                response.status = row.try_get("status")?;
                response.invoice_number = row.try_get("invoice_number")?;
                if response.billing_period.is_empty() {
                    response.billing_period = row.try_get("period")?;
                }
                let mut value = serde_json::to_value(response).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice response JSON: {error}"))
                })?;
                let object = value.as_object_mut().ok_or_else(|| {
                    sqlx::Error::Protocol("invoice response is not a JSON object".into())
                })?;
                object.insert(
                    "period".into(),
                    serde_json::Value::String(row.try_get("period")?),
                );
                object.insert(
                    "issued_at".into(),
                    serde_json::Value::String(row.try_get("issued_at")?),
                );
                Ok(value)
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let payment_rows = sqlx::query(
            "SELECT id, amount_received, currency, method, paid_at, status
             FROM payments WHERE customer_id = $1
             ORDER BY paid_at DESC, id DESC LIMIT 100",
        )
        .bind(customer_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let allocation_rows = sqlx::query(
            "SELECT pa.payment_id, pa.receivable_id, pa.schedule_id,
                    pa.allocated_amount, pa.currency
             FROM payment_allocations pa
             JOIN payments p ON p.id = pa.payment_id
             WHERE p.customer_id = $1
               AND p.id IN (SELECT id FROM payments WHERE customer_id = $1
                            ORDER BY paid_at DESC, id DESC LIMIT 100)
             ORDER BY pa.payment_id, pa.id",
        )
        .bind(customer_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut allocations_by_payment: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        for allocation in allocation_rows {
            let payment_id: String = allocation.try_get("payment_id")?;
            allocations_by_payment
                .entry(payment_id)
                .or_default()
                .push(json!({
                    "receivable_id": parse_uuid_column(&allocation, "receivable_id", "allocation receivable")?,
                    "schedule_id": allocation.try_get::<Option<String>, _>("schedule_id")?
                        .map(|id| id.parse::<Uuid>().map_err(|error| sqlx::Error::Protocol(format!("invalid schedule id: {error}"))))
                        .transpose()?,
                    "amount": format!("{:.2}", decimal_text(&allocation, "allocated_amount")?.round_dp(2)),
                    "currency": allocation.try_get::<String, _>("currency")?
                }));
        }
        let reversal_rows = sqlx::query(
            "SELECT pr.payment_id, pr.amount, pr.reason, pr.created_at
             FROM payment_reversals pr
             JOIN payments p ON p.id = pr.payment_id
             WHERE p.customer_id = $1
               AND p.id IN (SELECT id FROM payments WHERE customer_id = $1
                            ORDER BY paid_at DESC, id DESC LIMIT 100)
             ORDER BY pr.payment_id, pr.created_at, pr.id",
        )
        .bind(customer_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut reversals_by_payment: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
        for reversal in reversal_rows {
            let payment_id: String = reversal.try_get("payment_id")?;
            reversals_by_payment
                .entry(payment_id)
                .or_default()
                .push(json!({
                    "amount": format!("{:.2}", decimal_text(&reversal, "amount")?.round_dp(2)),
                    "reason": reversal.try_get::<String, _>("reason")?,
                    "created_at": reversal.try_get::<String, _>("created_at")?
                }));
        }
        let mut payments = Vec::with_capacity(payment_rows.len());
        for row in payment_rows {
            let payment_id = parse_uuid_column(&row, "id", "payment")?;
            let payment_key = payment_id.to_string();
            payments.push(json!({
                "payment_id": payment_id,
                "amount_received": format!("{:.2}", decimal_text(&row, "amount_received")?.round_dp(2)),
                "currency": row.try_get::<String, _>("currency")?,
                "method": row.try_get::<String, _>("method")?,
                "paid_at": row.try_get::<String, _>("paid_at")?,
                "status": row.try_get::<String, _>("status")?,
                "allocations": allocations_by_payment.remove(&payment_key).unwrap_or_default(),
                "reversals": reversals_by_payment.remove(&payment_key).unwrap_or_default()
            }));
        }
        Ok(Some(json!({
            "customer_id": customer_id,
            "receivables": receivables,
            "invoices": invoices,
            "payments": payments
        })))
    }

    pub async fn get_installment_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let plan = sqlx::query(
            "SELECT ip.id, ip.receivable_id, ip.contract_id, ip.principal_amount,
                    ip.currency, ip.start_date, ip.status, ip.terms_snapshot_json,
                    r.customer_id
             FROM installment_plans ip
             JOIN receivables r ON r.id = ip.receivable_id
             JOIN customers c ON c.id = r.customer_id
             WHERE ip.id = $1 AND c.organization_id = $2",
        )
        .bind(plan_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(plan) = plan else {
            return Ok(None);
        };
        let receivable_id = plan.try_get::<String, _>("receivable_id")?;
        let principal = decimal_text(&plan, "principal_amount")?;
        let plan_currency: String = plan.try_get("currency")?;
        let plan_start_date: String = plan.try_get("start_date")?;
        let terms_json: String = plan.try_get("terms_snapshot_json")?;
        let terms_snapshot = serde_json::from_str::<serde_json::Value>(&terms_json)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid terms snapshot: {error}")))?;
        let today = Utc::now().date_naive();
        let rows = sqlx::query(
            "SELECT id, sequence, due_date, amount_due, schedule_status
             FROM installment_schedule WHERE plan_id = $1 ORDER BY sequence",
        )
        .bind(plan_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut total_paid = Decimal::ZERO;
        let mut total_overdue = Decimal::ZERO;
        let mut schedule = Vec::with_capacity(rows.len());
        for row in rows {
            let schedule_id: String = row.try_get("id")?;
            let due_date_text: String = row.try_get("due_date")?;
            let due_date = NaiveDate::parse_from_str(&due_date_text, "%Y-%m-%d")
                .map_err(|error| sqlx::Error::Protocol(format!("invalid due date: {error}")))?;
            let amount_due = decimal_text(&row, "amount_due")?.round_dp(2);
            let paid_rows = sqlx::query(
                "SELECT pa.allocated_amount, p.amount_received,
                        COALESCE((SELECT GROUP_CONCAT(pr.amount, ',')
                                  FROM payment_reversals pr WHERE pr.payment_id = p.id), '')
                         AS reversed_amounts
                 FROM payment_allocations pa
                 JOIN payments p ON p.id = pa.payment_id
                 WHERE pa.schedule_id = $1
                   AND p.status IN ('LOCAL_PENDING', 'CENTRAL_ACCEPTED')",
            )
            .bind(&schedule_id)
            .fetch_all(&self.pool)
            .await?;
            let paid = paid_rows
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
            let paid = paid.round_dp(2).min(amount_due);
            let remaining = (amount_due - paid).round_dp(2);
            let status = if remaining <= Decimal::ZERO {
                "PAID"
            } else if due_date < today {
                total_overdue += remaining;
                "OVERDUE"
            } else {
                "OPEN"
            };
            total_paid += paid;
            schedule.push(json!({
                "id": schedule_id,
                "sequence": row.try_get::<i64, _>("sequence")?,
                "due_date": due_date_text,
                "amount_due": format!("{amount_due:.2}"),
                "paid": format!("{paid:.2}"),
                "remaining": format!("{remaining:.2}"),
                "stored_status": row.try_get::<String, _>("schedule_status")?,
                "status": status,
                "overdue": status == "OVERDUE"
            }));
        }
        let remaining = (principal - total_paid).round_dp(2).max(Decimal::ZERO);
        Ok(Some(json!({
            "id": plan.try_get::<String, _>("id")?,
            "receivable_id": receivable_id,
            "contract_id": plan.try_get::<String, _>("contract_id")?,
            "customer_id": plan.try_get::<String, _>("customer_id")?,
            "principal": format!("{:.2}", principal.round_dp(2)),
            "paid": format!("{:.2}", total_paid.round_dp(2)),
            "remaining": format!("{remaining:.2}"),
            "overdue_amount": format!("{:.2}", total_overdue.round_dp(2)),
            "currency": plan_currency,
            "start_date": plan_start_date,
            "stored_status": plan.try_get::<String, _>("status")?,
            "terms_snapshot": terms_snapshot,
            "schedule": schedule
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

    pub async fn list_payments(
        &self,
        limit: u32,
        customer_id: Option<Uuid>,
        status: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT p.id, p.customer_id, p.amount_received, p.currency,
                    p.method, p.paid_at, p.status
             FROM payments p
             JOIN customers c ON c.id = p.customer_id
             WHERE c.organization_id = $1
               AND ($2 IS NULL OR p.customer_id = $2)
               AND ($3 IS NULL OR p.status = $3)
             ORDER BY p.paid_at DESC, p.id DESC
             LIMIT $4",
        )
        .bind(self.organization_id.to_string())
        .bind(customer_id.map(|id| id.to_string()))
        .bind(status)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(json!({
                    "payment_id": parse_uuid_column(&row, "id", "payment")?,
                    "customer_id": parse_uuid_column(&row, "customer_id", "payment customer")?,
                    "amount_received": format!("{:.2}", decimal_text(&row, "amount_received")?.round_dp(2)),
                    "currency": row.try_get::<String, _>("currency")?,
                    "method": row.try_get::<String, _>("method")?,
                    "paid_at": row.try_get::<String, _>("paid_at")?,
                    "status": row.try_get::<String, _>("status")?
                }))
            })
            .collect()
    }

    pub async fn payment_document_data(
        &self,
        payment_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let payment = sqlx::query(
            "SELECT p.id, p.customer_id, p.amount_received, p.currency, p.method,
                    p.paid_at, p.status, c.customer_code, c.legal_name, o.legal_name AS organization_name,
                    c.contact_fields_json
             FROM payments p
             JOIN customers c ON c.id = p.customer_id
             JOIN organizations o ON o.id = c.organization_id
             WHERE p.id = $1 AND c.organization_id = $2",
        )
        .bind(payment_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(payment) = payment else {
            return Ok(None);
        };

        let allocation_rows = sqlx::query(
            "SELECT pa.receivable_id, pa.schedule_id, pa.allocated_amount, pa.currency,
                    CASE WHEN r.source_type = 'INVOICE' THEN COALESCE(i.invoice_number, r.source_id)
                         WHEN r.source_type = 'SALE' THEN COALESCE((SELECT s.document_reference FROM sales s WHERE s.id = r.source_id), r.source_id)
                         ELSE r.source_id END AS source_reference
             FROM payment_allocations pa
             JOIN receivables r ON r.id = pa.receivable_id
             LEFT JOIN invoices i ON i.id = r.source_id AND r.source_type = 'INVOICE'
             WHERE pa.payment_id = $1 ORDER BY pa.id",
        )
        .bind(payment_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let allocations = allocation_rows
            .into_iter()
            .map(|row| {
                Ok(json!({
                    "receivable_id": parse_uuid_column(&row, "receivable_id", "payment receivable")?,
                    "schedule_id": row.try_get::<Option<String>, _>("schedule_id")?.and_then(|value| value.parse::<Uuid>().ok()),
                    "amount": format!("{:.2}", decimal_text(&row, "allocated_amount")?.round_dp(2)),
                    "currency": row.try_get::<String, _>("currency")?,
                    "source_reference": row.try_get::<String, _>("source_reference")?
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let reversal_rows = sqlx::query(
            "SELECT amount, reason, created_at FROM payment_reversals
             WHERE payment_id = $1 ORDER BY created_at, id",
        )
        .bind(payment_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let reversals = reversal_rows
            .into_iter()
            .map(|row| {
                Ok(json!({
                    "amount": format!("{:.2}", decimal_text(&row, "amount")?.round_dp(2)),
                    "reason": row.try_get::<String, _>("reason")?,
                    "created_at": row.try_get::<String, _>("created_at")?
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let customer_id = parse_uuid_column(&payment, "customer_id", "payment customer")?;
        let contact_fields: serde_json::Value =
            serde_json::from_str(&payment.try_get::<String, _>("contact_fields_json")?).map_err(
                |error| sqlx::Error::Protocol(format!("invalid customer contact fields: {error}")),
            )?;
        Ok(Some(json!({
            "document_schema_version": 1,
            "document_type": "PAYMENT_RECEIPT",
            "document_reference": format!("SEL-PAY-{}", payment_id.simple().to_string().to_uppercase()),
            "qr_internal_reference": format!("SEL:PAYMENT:{payment_id}"),
            "organization_name": payment.try_get::<String, _>("organization_name")?,
            "payment": {
                "payment_id": payment_id,
                "customer_id": customer_id,
                "amount_received": format!("{:.2}", decimal_text(&payment, "amount_received")?.round_dp(2)),
                "currency": payment.try_get::<String, _>("currency")?,
                "method": payment.try_get::<String, _>("method")?,
                "paid_at": payment.try_get::<String, _>("paid_at")?,
                "status": payment.try_get::<String, _>("status")?
            },
            "customer": {
                "customer_id": customer_id,
                "customer_code": payment.try_get::<String, _>("customer_code")?,
                "legal_name": payment.try_get::<String, _>("legal_name")?,
                "address": contact_fields.get("address").cloned().unwrap_or(serde_json::Value::Null),
                "phone": contact_fields.get("phone").cloned().unwrap_or(serde_json::Value::Null)
            },
            "allocations": allocations,
            "reversals": reversals,
            "origin": "LOCAL"
        })))
    }

    pub async fn list_solar_sales(
        &self,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT s.id, s.customer_id, s.contract_id, ip.id AS installment_plan_id, s.currency,
                    s.sold_at, s.sale_type, s.status, r.id AS receivable_id,
                    r.principal_total
             FROM sales s
             JOIN customers c ON c.id = s.customer_id
             LEFT JOIN receivables r ON r.source_type = 'SALE' AND r.source_id = s.id
             LEFT JOIN installment_plans ip ON ip.contract_id = s.contract_id
             WHERE c.organization_id = $1
             ORDER BY s.sold_at DESC, s.id DESC
             LIMIT $2",
        )
        .bind(self.organization_id.to_string())
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(json!({
                    "sale_id": parse_uuid_column(&row, "id", "solar sale")?,
                    "customer_id": parse_uuid_column(&row, "customer_id", "solar customer")?,
                    "contract_id": row.try_get::<Option<String>, _>("contract_id")?.and_then(|value| value.parse::<Uuid>().ok()),
                    "installment_plan_id": row.try_get::<Option<String>, _>("installment_plan_id")?.and_then(|value| value.parse::<Uuid>().ok()),
                    "receivable_id": row.try_get::<Option<String>, _>("receivable_id")?.and_then(|value| value.parse::<Uuid>().ok()),
                    "principal": format!("{:.2}", decimal_text(&row, "principal_total")?.round_dp(2)),
                    "currency": row.try_get::<String, _>("currency")?,
                    "sold_at": row.try_get::<String, _>("sold_at")?,
                    "sale_type": row.try_get::<String, _>("sale_type")?,
                    "status": row.try_get::<String, _>("status")?
                }))
            })
            .collect()
    }

    pub async fn solar_sale_document_data(
        &self,
        sale_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let sale = sqlx::query(
            "SELECT s.id, s.customer_id, s.contract_id, s.currency, s.sold_at, s.status,
                    r.id AS receivable_id, r.principal_total,
                    c.customer_code, c.legal_name, o.legal_name AS organization_name, c.contact_fields_json
             FROM sales s
             JOIN customers c ON c.id = s.customer_id
             JOIN organizations o ON o.id = c.organization_id
             LEFT JOIN receivables r ON r.source_type = 'SALE' AND r.source_id = s.id
             WHERE s.id = $1 AND c.organization_id = $2",
        )
        .bind(sale_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(sale) = sale else { return Ok(None) };
        let customer_id = parse_uuid_column(&sale, "customer_id", "solar sale customer")?;
        let contacts: serde_json::Value = serde_json::from_str(
            &sale.try_get::<String, _>("contact_fields_json")?,
        )
        .map_err(|error| sqlx::Error::Protocol(format!("invalid solar contacts: {error}")))?;
        let line_rows = sqlx::query(
            "SELECT product_ref, quantity, unit_price, tax_snapshot_json, total
             FROM sale_lines WHERE sale_id = $1 ORDER BY id",
        )
        .bind(sale_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let lines = line_rows
            .into_iter()
            .map(|line| {
                let tax_snapshot: String = line.try_get("tax_snapshot_json")?;
                let unit_price = Decimal::from_str(&line.try_get::<String, _>("unit_price")?)
                    .map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid solar unit price: {error}"))
                    })?;
                let quantity = Decimal::from_str(&line.try_get::<String, _>("quantity")?).map_err(
                    |error| sqlx::Error::Protocol(format!("invalid solar quantity: {error}")),
                )?;
                let tax_snapshot = serde_json::from_str::<serde_json::Value>(&tax_snapshot)
                    .map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid solar tax snapshot: {error}"))
                    })?;
                let tax_amount = tax_snapshot["amount"]
                    .as_str()
                    .and_then(|value| Decimal::from_str(value).ok())
                    .unwrap_or(Decimal::ZERO);
                Ok(json!({
                    "product_reference": line.try_get::<String, _>("product_ref")?,
                    "quantity": quantity.normalize().to_string(),
                    "unit_price": format!("{:.2}", unit_price.round_dp(2)),
                    "subtotal": format!("{:.2}", (unit_price * quantity).round_dp(2)),
                    "tax_amount": format!("{:.2}", tax_amount.round_dp(2)),
                    "tax_snapshot": tax_snapshot,
                    "total": line.try_get::<String, _>("total")?
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let subtotal = lines
            .iter()
            .filter_map(|line| line["subtotal"].as_str()?.parse::<Decimal>().ok())
            .sum::<Decimal>()
            .round_dp(2);
        let tax_amount = lines
            .iter()
            .filter_map(|line| line["tax_amount"].as_str()?.parse::<Decimal>().ok())
            .sum::<Decimal>()
            .round_dp(2);
        let receivable_id: Option<String> = sale.try_get("receivable_id")?;
        let plan_id: Option<String> = if let Some(receivable_id) = receivable_id.as_deref() {
            sqlx::query_scalar("SELECT id FROM installment_plans WHERE receivable_id = $1")
                .bind(receivable_id)
                .fetch_optional(&self.pool)
                .await?
        } else {
            None
        };
        let schedule = if let Some(plan_id) = plan_id.as_deref() {
            let rows = sqlx::query(
                "SELECT id, sequence, due_date, amount_due, schedule_status
                 FROM installment_schedule WHERE plan_id = $1 ORDER BY sequence",
            )
            .bind(plan_id)
            .fetch_all(&self.pool)
            .await?;
            rows.into_iter()
                .map(|item| {
                    Ok(json!({
                        "id": parse_uuid_column(&item, "id", "solar installment")?,
                        "sequence": item.try_get::<i64, _>("sequence")?,
                        "due_date": item.try_get::<String, _>("due_date")?,
                        "amount_due": item.try_get::<String, _>("amount_due")?,
                        "status": item.try_get::<String, _>("schedule_status")?
                    }))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?
        } else {
            Vec::new()
        };
        let status: String = sale.try_get("status")?;
        Ok(Some(json!({
            "document_schema_version": 1,
            "document_type": "SOLAR_SALE_CONFIRMATION",
            "document_reference": format!("SEL-SOLAR-{}", sale_id.simple().to_string().to_uppercase()),
            "qr_internal_reference": format!("SEL:SOLAR-SALE:{sale_id}"),
            "organization_name": sale.try_get::<String, _>("organization_name")?,
            "sale": {
                "sale_id": sale_id,
                "receivable_id": receivable_id,
                "customer_id": customer_id,
                "contract_id": sale.try_get::<Option<String>, _>("contract_id")?,
                "subtotal": format!("{:.2}", subtotal),
                "tax_amount": format!("{:.2}", tax_amount),
                "principal": format!("{:.2}", decimal_text(&sale, "principal_total")?.round_dp(2)),
                "currency": sale.try_get::<String, _>("currency")?,
                "sold_at": sale.try_get::<String, _>("sold_at")?,
                "status": status
            },
            "customer": {
                "customer_code": sale.try_get::<String, _>("customer_code")?,
                "legal_name": sale.try_get::<String, _>("legal_name")?,
                "address": contacts.get("address").cloned().unwrap_or(serde_json::Value::Null),
                "phone": contacts.get("phone").cloned().unwrap_or(serde_json::Value::Null)
            },
            "lines": lines,
            "installment_plan": { "id": plan_id, "schedule": schedule },
            "origin": if status == "LOCAL_PENDING" { "LOCAL_PENDING" } else { "CENTRAL_ACCEPTED" }
        })))
    }

    /// Returns only currently applicable, approved solar products from the
    /// last verified catalog snapshot. Prices remain decimal strings at the
    /// API boundary and are never computed by the desktop client.
    pub async fn list_active_solar_products(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, sku, label, price_reference, currency,
                    effective_from, effective_to, approval_ref
             FROM products
             WHERE product_type = 'SOLAR_KIT'
               AND active = 1
               AND status = 'APPROVED'
               AND effective_from <= date('now')
               AND (effective_to IS NULL OR effective_to >= date('now'))
             ORDER BY sku, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<String, _>("id")?,
                    "sku": row.try_get::<String, _>("sku")?,
                    "product_type": "SOLAR_KIT",
                    "label": row.try_get::<String, _>("label")?,
                    "price_reference": row.try_get::<String, _>("price_reference")?,
                    "currency": row.try_get::<String, _>("currency")?,
                    "effective_from": row.try_get::<String, _>("effective_from")?,
                    "effective_to": row.try_get::<Option<String>, _>("effective_to")?,
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
                    "status": "APPROVED"
                }))
            })
            .collect()
    }

    pub async fn allocate_payment(
        &self,
        payment_id: Uuid,
        allocations: &[PaymentAllocationRequest],
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<serde_json::Value, LocalPaymentAllocationError> {
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'payment.allocate' AND idempotency_key = $3",
        )
        .bind(self.organization_id.to_string())
        .bind(self.actor_id.to_string())
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(LocalPaymentAllocationError::IdempotencyPayloadMismatch);
            }
            let original: String = row.try_get("response_json")?;
            return Ok(serde_json::from_str(&original)?);
        }
        if allocations.is_empty() || allocations.iter().any(|item| item.amount <= Decimal::ZERO) {
            return Err(LocalPaymentAllocationError::InvalidAllocation);
        }
        let mut grouped = std::collections::BTreeMap::<Uuid, Decimal>::new();
        let mut persisted = std::collections::BTreeMap::<(Uuid, Option<Uuid>), Decimal>::new();
        for allocation in allocations {
            *grouped
                .entry(allocation.receivable_id)
                .or_insert(Decimal::ZERO) += allocation.amount;
            *persisted
                .entry((allocation.receivable_id, allocation.schedule_id))
                .or_insert(Decimal::ZERO) += allocation.amount;
        }
        let requested_total: Decimal = grouped.values().copied().sum();
        let payment = sqlx::query(
            "SELECT customer_id, CAST(amount_received AS TEXT) AS amount_received, currency, status
             FROM payments WHERE id = $1",
        )
        .bind(payment_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(LocalPaymentAllocationError::PaymentNotFound)?;
        let status: String = payment.try_get("status")?;
        if status != "LOCAL_PENDING" && status != "CENTRAL_ACCEPTED" {
            return Err(LocalPaymentAllocationError::PaymentNotAllocatable);
        }
        let customer_id: String = payment.try_get("customer_id")?;
        let amount_received = decimal_text(&payment, "amount_received")?;
        let currency: String = payment.try_get("currency")?;
        let prior_payment_allocated_text: String = sqlx::query_scalar(
            "SELECT CAST(COALESCE(SUM(allocated_amount), 0) AS TEXT)
             FROM payment_allocations WHERE payment_id = $1",
        )
        .bind(payment_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        let prior_payment_allocated = Decimal::from_str(&prior_payment_allocated_text)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid allocated amount: {error}")))?;
        if prior_payment_allocated + requested_total > amount_received {
            return Err(LocalPaymentAllocationError::AllocationExceedsPayment);
        }
        for (receivable_id, allocated_amount) in &grouped {
            let row = sqlx::query(
                "SELECT customer_id, currency, principal_total
                 FROM receivables WHERE id = $1",
            )
            .bind(receivable_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(LocalPaymentAllocationError::ContextInvalid)?;
            if row.try_get::<String, _>("customer_id")? != customer_id
                || row.try_get::<String, _>("currency")? != currency
            {
                return Err(LocalPaymentAllocationError::ContextInvalid);
            }
            let principal = decimal_text(&row, "principal_total")?;
            let paid_text: String = sqlx::query_scalar(
                "SELECT CAST(COALESCE(SUM(allocated_amount), 0) AS TEXT)
                 FROM payment_allocations WHERE receivable_id = $1",
            )
            .bind(receivable_id.to_string())
            .fetch_one(&mut *tx)
            .await?;
            let paid = Decimal::from_str(&paid_text)
                .map_err(|error| sqlx::Error::Protocol(format!("invalid paid amount: {error}")))?;
            if paid + allocated_amount > principal {
                return Err(LocalPaymentAllocationError::Overallocated);
            }
            for ((_, schedule_id), _) in persisted.iter().filter(|((id, _), _)| id == receivable_id)
            {
                if let Some(schedule_id) = schedule_id {
                    let exists: Option<String> = sqlx::query_scalar(
                        "SELECT s.id FROM installment_schedule s
                         JOIN installment_plans p ON p.id = s.plan_id
                         WHERE s.id = $1 AND p.receivable_id = $2",
                    )
                    .bind(schedule_id.to_string())
                    .bind(receivable_id.to_string())
                    .fetch_optional(&mut *tx)
                    .await?;
                    if exists.is_none() {
                        return Err(LocalPaymentAllocationError::ContextInvalid);
                    }
                }
            }
        }
        for ((receivable_id, schedule_id), allocated_amount) in &persisted {
            sqlx::query(
                "INSERT INTO payment_allocations
                 (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(payment_id.to_string())
            .bind(receivable_id.to_string())
            .bind(schedule_id.map(|id| id.to_string()))
            .bind(allocated_amount.to_string())
            .bind(&currency)
            .execute(&mut *tx)
            .await?;
        }
        let body = json!({
            "payment_id": payment_id,
            "status": status,
            "amount": format!("{:.2}", amount_received),
            "currency": currency,
            "allocations": allocations
        });
        let event = local_event(
            self,
            Uuid::new_v4(),
            payment_id,
            "PAYMENT_ALLOCATED",
            serde_json::to_value(PaymentAllocationSyncPayload {
                payment_id,
                allocations: allocations.to_vec(),
            })
            .map_err(|error| {
                LocalPaymentAllocationError::Database(sqlx::Error::Protocol(format!(
                    "invalid payment allocation JSON: {error}"
                )))
            })?,
        )
        .map_err(LocalPaymentAllocationError::Database)?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, device, event_type, subject_id, occurred_at, redacted_metadata_json)
             VALUES ($1, $2, $3, 'PAYMENT_ALLOCATED', $4, datetime('now'), $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(self.actor_id.to_string())
        .bind(self.device_id.to_string())
        .bind(payment_id.to_string())
        .bind(body.to_string())
        .execute(&mut *tx)
        .await?;
        insert_command_in_tx(&mut tx, &event).await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json, created_at)
             VALUES ($1, $2, 'payment.allocate', $3, $4, 201, $5, datetime('now'))",
        )
        .bind(self.organization_id.to_string())
        .bind(self.actor_id.to_string())
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(body.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(body)
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

    pub async fn create_credit_note_offline(
        &self,
        receivable_id: Uuid,
        input: &CreditNoteRequest,
    ) -> Result<(serde_json::Value, SyncEnvelope), LocalCreditNoteError> {
        let _guard = self.write_lock.lock().await;
        if input.amount <= Decimal::ZERO {
            return Err(LocalCreditNoteError::InvalidAmount);
        }
        if input.reason.trim().is_empty() {
            return Err(LocalCreditNoteError::ReasonRequired);
        }
        let credit_note_id = Uuid::new_v4();
        let payload = serde_json::to_value(CreditNoteSyncPayload {
            receivable_id,
            credit_note_id,
            amount: input.amount,
            reason: input.reason.trim().to_owned(),
            actor_id: self.actor_id,
        })
        .map_err(|error| {
            LocalCreditNoteError::Database(sqlx::Error::Protocol(format!(
                "invalid credit note JSON: {error}"
            )))
        })?;
        let event = local_event(
            self,
            Uuid::new_v4(),
            receivable_id,
            "CREDIT_NOTE_RECORDED",
            payload,
        )?;
        let mut tx = self.pool.begin().await?;
        let Some(receivable) =
            sqlx::query("SELECT currency, principal_total, status FROM receivables WHERE id = $1")
                .bind(receivable_id.to_string())
                .fetch_optional(&mut *tx)
                .await?
        else {
            return Err(LocalCreditNoteError::ReceivableNotFound);
        };
        let status: String = receivable.try_get("status")?;
        if matches!(status.as_str(), "VOID" | "REJECTED" | "SETTLED") {
            return Err(LocalCreditNoteError::ReceivableNotCreditable);
        }
        let currency: String = receivable.try_get("currency")?;
        let principal_text: String = receivable.try_get("principal_total")?;
        let principal = Decimal::from_str(&principal_text)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid principal: {error}")))?;
        let payment_rows = sqlx::query(
            "SELECT pa.allocated_amount, p.amount_received,
                    COALESCE((SELECT GROUP_CONCAT(pr.amount, ',') FROM payment_reversals pr
                              WHERE pr.payment_id = p.id), '') AS reversed_amounts
             FROM payment_allocations pa
             JOIN payments p ON p.id = pa.payment_id
             WHERE pa.receivable_id = $1
               AND p.status IN ('LOCAL_PENDING', 'CENTRAL_ACCEPTED')",
        )
        .bind(receivable_id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        let mut paid = Decimal::ZERO;
        for row in payment_rows {
            let allocated = Decimal::from_str(&row.try_get::<String, _>("allocated_amount")?)
                .map_err(|error| sqlx::Error::Protocol(format!("invalid allocation: {error}")))?;
            let received = Decimal::from_str(&row.try_get::<String, _>("amount_received")?)
                .map_err(|error| sqlx::Error::Protocol(format!("invalid payment: {error}")))?;
            let reversed_text: String = row.try_get("reversed_amounts")?;
            let reversed = reversed_text
                .split(',')
                .filter(|value| !value.is_empty())
                .try_fold(Decimal::ZERO, |total, value| {
                    let amount = Decimal::from_str(value).map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid reversal: {error}"))
                    })?;
                    Ok::<Decimal, sqlx::Error>(total + amount)
                })?;
            if received > Decimal::ZERO {
                paid += allocated * ((received - reversed).max(Decimal::ZERO) / received);
            }
        }
        let credited_text: String = sqlx::query_scalar(
            "SELECT COALESCE(GROUP_CONCAT(amount, ','), '')
             FROM credit_notes WHERE receivable_id = $1",
        )
        .bind(receivable_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        let credited = credited_text
            .split(',')
            .filter(|value| !value.is_empty())
            .try_fold(Decimal::ZERO, |total, value| {
                let amount = Decimal::from_str(value).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid credit amount: {error}"))
                })?;
                Ok::<Decimal, sqlx::Error>(total + amount)
            })?;
        let outstanding = principal - paid - credited;
        if input.amount > outstanding {
            return Err(LocalCreditNoteError::AmountExceedsBalance);
        }
        sqlx::query(
            "INSERT INTO credit_notes
             (id, receivable_id, amount, reason, initiated_by, created_at)
             VALUES ($1, $2, $3, $4, $5, datetime('now'))",
        )
        .bind(credit_note_id.to_string())
        .bind(receivable_id.to_string())
        .bind(input.amount.to_string())
        .bind(input.reason.trim())
        .bind(self.actor_id.to_string())
        .execute(&mut *tx)
        .await?;
        let remaining = outstanding - input.amount;
        let new_status = if remaining <= Decimal::ZERO {
            "SETTLED"
        } else {
            "CREDITED"
        };
        sqlx::query("UPDATE receivables SET status = $1 WHERE id = $2")
            .bind(new_status)
            .bind(receivable_id.to_string())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, device, event_type, subject_id, occurred_at, redacted_metadata_json)
             VALUES ($1, $2, $3, 'CREDIT_NOTE_RECORDED', $4, datetime('now'), $5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(self.actor_id.to_string())
        .bind(self.device_id.to_string())
        .bind(receivable_id.to_string())
        .bind(
            json!({
                "credit_note_id": credit_note_id,
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
                "credit_note_id": credit_note_id,
                "receivable_id": receivable_id,
                "amount": format!("{:.2}", input.amount),
                "currency": currency,
                "reason": input.reason.trim(),
                "status": "LOCAL_PENDING",
                "receivable_status": new_status,
                "remaining_balance": format!("{remaining:.2}")
            }),
            event,
        ))
    }

    pub async fn pending_count(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT count(*) FROM outbox WHERE state = 'PENDING'")
            .fetch_one(&self.pool)
            .await
    }

    pub async fn sync_issue_counts(&self) -> Result<(i64, i64), sqlx::Error> {
        let row = sqlx::query(
            "SELECT
                (SELECT count(*) FROM sync_conflicts) +
                (SELECT count(*) FROM outbox o WHERE o.state = 'CONFLICT'
                 AND NOT EXISTS (SELECT 1 FROM sync_conflicts c WHERE c.event_id = o.event_id)) AS conflicts,
                (SELECT count(*) FROM outbox WHERE state = 'REJECTED') AS rejected",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.try_get("conflicts")?, row.try_get("rejected")?))
    }

    /// Read-only synchronization diagnostics. Never returns stored command payloads.
    pub async fn list_sync_issues(
        &self,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, event_id, reason, central_revision, created_at, issue_type, command_type
             FROM (
                SELECT c.id, c.event_id, c.reason, c.central_revision, c.created_at,
                       'CONFLICT' AS issue_type, NULL AS command_type
                FROM sync_conflicts c
                UNION ALL
                SELECT j.event_id AS id, j.event_id,
                       COALESCE(o.last_error, j.status) AS reason, NULL AS central_revision,
                       j.created_at, o.state AS issue_type, j.command_type
                FROM outbox o JOIN command_journal j ON j.event_id = o.event_id
                WHERE o.state IN ('CONFLICT', 'REJECTED')
                  AND NOT EXISTS (SELECT 1 FROM sync_conflicts c WHERE c.event_id = j.event_id)
             ) AS issues
             ORDER BY created_at DESC, id DESC LIMIT $1",
        )
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(json!({
                    "id": row.try_get::<String, _>("id")?,
                    "event_id": row.try_get::<String, _>("event_id")?,
                    "reason": row.try_get::<String, _>("reason")?,
                    "central_revision": row.try_get::<Option<i64>, _>("central_revision")?,
                    "created_at": row.try_get::<String, _>("created_at")?,
                    "status": row.try_get::<String, _>("issue_type")?,
                    "command_type": row.try_get::<Option<String>, _>("command_type")?
                }))
            })
            .collect()
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
        let current_cursor = sqlx::query_scalar::<_, String>(
            "SELECT value FROM sync_state WHERE key = 'central_cursor'",
        )
        .fetch_optional(&mut *tx)
        .await?
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
        if response.next_cursor < current_cursor {
            return Err(sqlx::Error::Protocol(
                "sync pull cursor cannot move backwards".into(),
            ));
        }
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
            if sequence <= current_cursor {
                let existing = sqlx::query_as::<_, (String, String, i64, String)>(
                    "SELECT entity, entity_id, revision, change_json
                     FROM central_changes WHERE sequence = $1",
                )
                .bind(sequence)
                .fetch_optional(&mut *tx)
                .await?;
                let Some((existing_entity, existing_entity_id, existing_revision, existing_json)) =
                    existing
                else {
                    return Err(sqlx::Error::Protocol(
                        "sync pull contains a sequence gap before the durable cursor".into(),
                    ));
                };
                let existing_change = serde_json::from_str::<serde_json::Value>(&existing_json)
                    .map_err(|error| {
                        sqlx::Error::Protocol(format!(
                            "stored sync change payload is invalid: {error}"
                        ))
                    })?;
                if existing_entity != entity
                    || existing_entity_id != entity_id
                    || existing_revision != revision
                    || existing_change != change_json
                {
                    return Err(sqlx::Error::Protocol(
                        "sync pull sequence collision has different content".into(),
                    ));
                }
                continue;
            }
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
                    "service_accounts" => {
                        apply_service_account_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    "meters" => {
                        apply_meter_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
                        .await?;
                    }
                    "meter_readings" => {
                        apply_meter_reading_change(
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
                        apply_payment_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            &change_json,
                        )
                        .await?;
                    }
                    "payment_reversals" => {
                        apply_payment_reversal_change(&mut tx, self.organization_id, &change_json)
                            .await?;
                    }
                    "credit_notes" => {
                        apply_credit_note_change(
                            &mut tx,
                            self.organization_id,
                            entity_id,
                            revision,
                            &change_json,
                        )
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

impl LocalStore {
    pub async fn create_service_account(
        &self,
        customer_id: Uuid,
        input: &CreateServiceAccount,
    ) -> Result<ServiceAccount, LocalMeterError> {
        if !matches!(input.service_code.as_str(), "ELECTRICITY" | "WATER")
            || !matches!(
                input.connection_status.as_str(),
                "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN"
            )
        {
            return Err(LocalMeterError::InvalidConfiguration);
        }
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let customer_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND organization_id = $2)",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if !customer_exists {
            return Err(LocalMeterError::CustomerNotFound);
        }
        let id = Uuid::new_v4();
        let activated_at = input.activated_at.map(|date| date.to_string());
        sqlx::query(
            "INSERT INTO service_accounts
             (id, customer_id, service_code, contract_ref, connection_status, activated_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id.to_string())
        .bind(customer_id.to_string())
        .bind(&input.service_code)
        .bind(&input.contract_ref)
        .bind(&input.connection_status)
        .bind(activated_at)
        .execute(&mut *tx)
        .await?;
        let account = ServiceAccount {
            id,
            customer_id,
            service_code: input.service_code.clone(),
            contract_ref: input.contract_ref.clone(),
            connection_status: input.connection_status.clone(),
            activated_at: input.activated_at,
            revision: 1,
        };
        let payload = serde_json::to_value(&account)
            .map_err(|error| LocalMeterError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), id, "SERVICE_ACCOUNT_CREATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn list_service_accounts(
        &self,
        customer_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, LocalMeterError> {
        let rows = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref,
                    sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.customer_id = $1 AND c.organization_id = $2
             ORDER BY sa.service_code, sa.id",
        )
        .bind(customer_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(service_account_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(LocalMeterError::from)
    }

    pub async fn get_service_account(
        &self,
        id: Uuid,
    ) -> Result<Option<ServiceAccount>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref,
                    sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(service_account_from_row).transpose()
    }

    pub async fn update_service_account(
        &self,
        id: Uuid,
        expected_revision: i64,
        input: &UpdateServiceAccount,
    ) -> Result<Option<(ServiceAccount, SyncEnvelope)>, LocalMeterError> {
        if expected_revision < 1
            || (input.contract_ref.is_missing()
                && input.connection_status.is_none()
                && input.activated_at.is_missing())
            || input
                .connection_status
                .as_deref()
                .is_some_and(|status| !matches!(status, "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN"))
            || matches!(&input.contract_ref, sel_contracts::PatchValue::Value(value) if value.trim().is_empty() || value.chars().count() > 128)
        {
            return Err(LocalMeterError::InvalidConfiguration);
        }
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref,
                    sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2 AND sa.revision = $3",
        )
        .bind(id.to_string())
        .bind(self.organization_id.to_string())
        .bind(expected_revision)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let current = service_account_from_row(&row)?;
        let contract_ref = input.contract_ref.apply_to(current.contract_ref.clone());
        let connection_status = input
            .connection_status
            .clone()
            .unwrap_or(current.connection_status.clone());
        let activated_at = input.activated_at.apply_to(current.activated_at);
        let new_revision = expected_revision + 1;
        sqlx::query(
            "UPDATE service_accounts SET contract_ref = $1, connection_status = $2,
             activated_at = $3, revision = $4
             WHERE id = $5 AND revision = $6",
        )
        .bind(&contract_ref)
        .bind(&connection_status)
        .bind(activated_at.map(|date| date.to_string()))
        .bind(new_revision)
        .bind(id.to_string())
        .bind(expected_revision)
        .execute(&mut *tx)
        .await?;
        let account = ServiceAccount {
            contract_ref,
            connection_status,
            activated_at,
            revision: new_revision,
            ..current
        };
        let payload = serde_json::to_value(ServiceAccountUpdateSyncPayload {
            expected_revision,
            account: account.clone(),
        })
        .map_err(|error| LocalMeterError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), id, "SERVICE_ACCOUNT_UPDATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(Some((account, event)))
    }

    pub async fn create_meter(
        &self,
        service_account_id: Uuid,
        input: &CreateMeter,
    ) -> Result<Meter, LocalMeterError> {
        let serial = input.serial.trim();
        if serial.is_empty()
            || serial.chars().count() > 128
            || !matches!(input.unit.as_str(), "KWH" | "M3")
        {
            return Err(LocalMeterError::InvalidInput);
        }
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let service_code: Option<String> = sqlx::query_scalar(
            "SELECT sa.service_code FROM service_accounts sa
             JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2",
        )
        .bind(service_account_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        let service_code = service_code.ok_or(LocalMeterError::ServiceAccountNotFound)?;
        let expected_unit = match service_code.as_str() {
            "ELECTRICITY" => "KWH",
            "WATER" => "M3",
            _ => return Err(LocalMeterError::InvalidConfiguration),
        };
        if input.unit != expected_unit {
            return Err(LocalMeterError::InvalidConfiguration);
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO meters (id, service_account_id, serial, unit, active)
             VALUES ($1, $2, $3, $4, 1)",
        )
        .bind(id.to_string())
        .bind(service_account_id.to_string())
        .bind(serial)
        .bind(&input.unit)
        .execute(&mut *tx)
        .await?;
        let meter = Meter {
            id,
            service_account_id,
            serial: serial.to_owned(),
            unit: input.unit.clone(),
            active: true,
        };
        let payload = serde_json::to_value(&meter)
            .map_err(|error| LocalMeterError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), id, "METER_CREATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(meter)
    }

    pub async fn list_meters(
        &self,
        service_account_id: Uuid,
    ) -> Result<Vec<Meter>, LocalMeterError> {
        let rows = sqlx::query(
            "SELECT m.id, m.service_account_id, m.serial, m.unit, m.active
             FROM meters m JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.service_account_id = $1 AND c.organization_id = $2
             ORDER BY m.serial, m.id",
        )
        .bind(service_account_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(meter_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(LocalMeterError::from)
    }

    pub async fn list_meter_readings(
        &self,
        meter_id: Uuid,
        period: Option<&str>,
    ) -> Result<Vec<MeterReading>, LocalMeterError> {
        let rows = sqlx::query(
            "SELECT r.id, r.meter_id, r.period, r.previous, r.current,
                    r.measured_quantity, r.source, r.corrected_from_id, r.correction_reason
             FROM meter_readings r JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.meter_id = $1 AND c.organization_id = $2
               AND ($3 IS NULL OR r.period = $3)
             ORDER BY r.period, CASE WHEN r.corrected_from_id IS NULL THEN 0 ELSE 1 END,
                      r.created_at, r.id",
        )
        .bind(meter_id.to_string())
        .bind(self.organization_id.to_string())
        .bind(period)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(meter_reading_from_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(LocalMeterError::from)
    }

    pub async fn preview_meter_reading(
        &self,
        meter_id: Uuid,
        input: &CreateMeterReading,
    ) -> Result<MeterReadingPreview, LocalMeterError> {
        validate_reading_input(
            &input.period,
            &input.previous,
            &input.current,
            &input.source,
        )?;
        let mut tx = self.pool.begin().await?;
        let active: Option<i64> = sqlx::query_scalar(
            "SELECT m.active FROM meters m
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.id = $1 AND c.organization_id = $2",
        )
        .bind(meter_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        if active.ok_or(LocalMeterError::MeterNotFound)? == 0 {
            return Err(LocalMeterError::InvalidConfiguration);
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meter_readings
             WHERE meter_id = $1 AND period = $2 AND corrected_from_id IS NULL)",
        )
        .bind(meter_id.to_string())
        .bind(&input.period)
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            return Err(LocalMeterError::DuplicateReading);
        }
        let previous_period_current: Option<String> = sqlx::query_scalar(
            "SELECT current FROM meter_readings
             WHERE meter_id = $1 AND period < $2
               AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = meter_readings.id)
             ORDER BY period DESC, created_at DESC, id DESC LIMIT 1",
        )
        .bind(meter_id.to_string())
        .bind(&input.period)
        .fetch_optional(&mut *tx)
        .await?;
        if previous_period_current
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .is_some_and(|value| value != input.previous)
        {
            return Err(LocalMeterError::PreviousIndexMismatch);
        }
        tx.commit().await?;
        Ok(MeterReadingPreview {
            meter_id,
            period: input.period.clone(),
            previous: input.previous,
            current: input.current,
            measured_quantity: input.current - input.previous,
            source: input.source.trim().to_owned(),
        })
    }

    pub async fn utility_meter_reading_quantities(
        &self,
        customer_id: Uuid,
        period: &str,
        refs: &UtilityMeterReadingRefs,
    ) -> Result<(Decimal, Decimal), LocalMeterError> {
        let mut tx = self.pool.begin().await?;
        let quantities =
            load_invoice_meter_readings(&mut tx, self.organization_id, customer_id, period, refs)
                .await
                .map_err(|error| match error {
                    sqlx::Error::Protocol(_) | sqlx::Error::RowNotFound => {
                        LocalMeterError::ReadingUnavailable
                    }
                    other => LocalMeterError::Database(other),
                })?;
        tx.commit().await?;
        Ok(quantities)
    }

    pub async fn create_meter_reading(
        &self,
        meter_id: Uuid,
        input: &CreateMeterReading,
    ) -> Result<MeterReading, LocalMeterError> {
        validate_reading_input(
            &input.period,
            &input.previous,
            &input.current,
            &input.source,
        )?;
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let active: Option<i64> = sqlx::query_scalar(
            "SELECT m.active FROM meters m
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.id = $1 AND c.organization_id = $2",
        )
        .bind(meter_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        if active.ok_or(LocalMeterError::MeterNotFound)? == 0 {
            return Err(LocalMeterError::InvalidConfiguration);
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meter_readings
             WHERE meter_id = $1 AND period = $2 AND corrected_from_id IS NULL)",
        )
        .bind(meter_id.to_string())
        .bind(&input.period)
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            return Err(LocalMeterError::DuplicateReading);
        }
        let previous_period_current: Option<String> = sqlx::query_scalar(
            "SELECT current FROM meter_readings
             WHERE meter_id = $1 AND period < $2
               AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = meter_readings.id)
             ORDER BY period DESC, created_at DESC, id DESC LIMIT 1",
        )
        .bind(meter_id.to_string())
        .bind(&input.period)
        .fetch_optional(&mut *tx)
        .await?;
        if previous_period_current
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .is_some_and(|value| value != input.previous)
        {
            return Err(LocalMeterError::PreviousIndexMismatch);
        }
        let id = Uuid::new_v4();
        let quantity = input.current - input.previous;
        sqlx::query(
            "INSERT INTO meter_readings
             (id, meter_id, period, previous, current, measured_quantity, source, corrected_from_id, correction_reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL)",
        )
        .bind(id.to_string())
        .bind(meter_id.to_string())
        .bind(&input.period)
        .bind(input.previous.to_string())
        .bind(input.current.to_string())
        .bind(quantity.to_string())
        .bind(input.source.trim())
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error.as_database_error().is_some_and(|database| database.is_unique_violation()) {
                LocalMeterError::DuplicateReading
            } else {
                LocalMeterError::Database(error)
            }
        })?;
        let reading = MeterReading {
            id,
            meter_id,
            period: input.period.clone(),
            previous: input.previous,
            current: input.current,
            measured_quantity: quantity,
            source: input.source.trim().to_owned(),
            corrected_from_id: None,
            correction_reason: None,
        };
        let payload = serde_json::to_value(&reading)
            .map_err(|error| LocalMeterError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), id, "METER_READING_CREATED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(reading)
    }

    pub async fn correct_meter_reading(
        &self,
        reading_id: Uuid,
        input: &CorrectMeterReading,
    ) -> Result<MeterReading, LocalMeterError> {
        let reason = input.reason.trim();
        if reason.is_empty() {
            return Err(LocalMeterError::CorrectionReasonRequired);
        }
        if reason.chars().count() > 500 || input.source.trim().is_empty() {
            return Err(LocalMeterError::InvalidInput);
        }
        let _guard = self.write_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            "SELECT r.id, r.meter_id, r.period, r.previous, r.current,
                    r.measured_quantity, r.source, r.corrected_from_id, r.correction_reason
             FROM meter_readings r JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.id = $1 AND c.organization_id = $2",
        )
        .bind(reading_id.to_string())
        .bind(self.organization_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(LocalMeterError::MeterNotFound);
        };
        let target = meter_reading_from_row(&row)?;
        let invoiced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM invoice_meter_readings WHERE meter_reading_id = $1)",
        )
        .bind(reading_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if invoiced {
            return Err(LocalMeterError::ReadingAlreadyInvoiced);
        }
        let has_correction: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meter_readings WHERE corrected_from_id = $1)",
        )
        .bind(reading_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        if has_correction {
            return Err(LocalMeterError::ReadingNotLatest);
        }
        if input.current < target.previous {
            return Err(LocalMeterError::InvalidIndex);
        }
        let next_period_previous: Option<String> = sqlx::query_scalar(
            "SELECT previous FROM meter_readings
             WHERE meter_id = $1 AND period > $2
             ORDER BY period, created_at, id LIMIT 1",
        )
        .bind(target.meter_id.to_string())
        .bind(&target.period)
        .fetch_optional(&mut *tx)
        .await?;
        if next_period_previous
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .is_some_and(|value| value != input.current)
        {
            return Err(LocalMeterError::PreviousIndexMismatch);
        }
        let id = Uuid::new_v4();
        let quantity = input.current - target.previous;
        sqlx::query(
            "INSERT INTO meter_readings
             (id, meter_id, period, previous, current, measured_quantity, source, corrected_from_id, correction_reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id.to_string())
        .bind(target.meter_id.to_string())
        .bind(&target.period)
        .bind(target.previous.to_string())
        .bind(input.current.to_string())
        .bind(quantity.to_string())
        .bind(input.source.trim())
        .bind(reading_id.to_string())
        .bind(reason)
        .execute(&mut *tx)
        .await?;
        let correction = MeterReading {
            id,
            meter_id: target.meter_id,
            period: target.period,
            previous: target.previous,
            current: input.current,
            measured_quantity: quantity,
            source: input.source.trim().to_owned(),
            corrected_from_id: Some(reading_id),
            correction_reason: Some(reason.to_owned()),
        };
        let payload = serde_json::to_value(&correction)
            .map_err(|error| LocalMeterError::InvalidPayload(error.to_string()))?;
        let event = local_event(self, Uuid::new_v4(), id, "METER_READING_CORRECTED", payload)?;
        insert_command_in_tx(&mut tx, &event).await?;
        tx.commit().await?;
        Ok(correction)
    }
}

fn validate_reading_input(
    period: &str,
    previous: &Decimal,
    current: &Decimal,
    source: &str,
) -> Result<(), LocalMeterError> {
    if chrono::NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d").is_err()
        || source.trim().is_empty()
        || source.trim().chars().count() > 64
    {
        return Err(LocalMeterError::InvalidInput);
    }
    if previous.is_sign_negative() || current < previous {
        return Err(LocalMeterError::InvalidIndex);
    }
    Ok(())
}

async fn load_invoice_meter_readings(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    customer_id: Uuid,
    period: &str,
    refs: &UtilityMeterReadingRefs,
) -> Result<(Decimal, Decimal), sqlx::Error> {
    if refs.electricity_reading_id == refs.water_reading_id {
        return Err(sqlx::Error::Protocol(
            "electricity and water must use distinct meter readings".into(),
        ));
    }
    // Validate the caller's organization scope and service identity for both readings.
    let electricity = sqlx::query(
        "SELECT r.measured_quantity FROM meter_readings r
         JOIN meters m ON m.id = r.meter_id
         JOIN service_accounts sa ON sa.id = m.service_account_id
         JOIN customers c ON c.id = sa.customer_id
         WHERE r.id = $1 AND c.id = $2 AND c.organization_id = $3
           AND sa.service_code = 'ELECTRICITY' AND m.unit = 'KWH' AND r.period = $4
           AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
           AND NOT EXISTS (SELECT 1 FROM invoice_meter_readings imr WHERE imr.meter_reading_id = r.id)",
    )
    .bind(refs.electricity_reading_id.to_string())
    .bind(customer_id.to_string())
    .bind(organization_id.to_string())
    .bind(period)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("selected electricity reading is unavailable".into()))?;
    let water = sqlx::query(
        "SELECT r.measured_quantity FROM meter_readings r
         JOIN meters m ON m.id = r.meter_id
         JOIN service_accounts sa ON sa.id = m.service_account_id
         JOIN customers c ON c.id = sa.customer_id
         WHERE r.id = $1 AND c.id = $2 AND c.organization_id = $3
           AND sa.service_code = 'WATER' AND m.unit = 'M3' AND r.period = $4
           AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
           AND NOT EXISTS (SELECT 1 FROM invoice_meter_readings imr WHERE imr.meter_reading_id = r.id)",
    )
    .bind(refs.water_reading_id.to_string())
    .bind(customer_id.to_string())
    .bind(organization_id.to_string())
    .bind(period)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("selected water reading is unavailable".into()))?;
    let parse = |row: sqlx::sqlite::SqliteRow| -> Result<Decimal, sqlx::Error> {
        let value: String = row.try_get("measured_quantity")?;
        Decimal::from_str(&value)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid meter quantity: {error}")))
    };
    Ok((parse(electricity)?, parse(water)?))
}

async fn verify_invoice_readings_projection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    customer_id: Uuid,
    period: &str,
    invoice_id: Uuid,
    refs: &UtilityMeterReadingRefs,
) -> Result<(Decimal, Decimal), sqlx::Error> {
    if refs.electricity_reading_id == refs.water_reading_id {
        return Err(sqlx::Error::Protocol(
            "electricity and water meter readings must be distinct".into(),
        ));
    }
    let mut quantities = [Decimal::ZERO; 2];
    for (index, (reading_id, service, unit)) in [
        (refs.electricity_reading_id, "ELECTRICITY", "KWH"),
        (refs.water_reading_id, "WATER", "M3"),
    ]
    .into_iter()
    .enumerate()
    {
        let row = sqlx::query(
            "SELECT r.measured_quantity FROM meter_readings r
             JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.id = $1 AND c.id = $2 AND c.organization_id = $3
               AND sa.service_code = $4 AND m.unit = $5 AND r.period = $6
               AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
               AND NOT EXISTS (SELECT 1 FROM invoice_meter_readings imr
                               WHERE imr.meter_reading_id = r.id AND imr.invoice_id <> $7)",
        )
        .bind(reading_id.to_string())
        .bind(customer_id.to_string())
        .bind(organization_id.to_string())
        .bind(service)
        .bind(unit)
        .bind(period)
        .bind(invoice_id.to_string())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            sqlx::Error::Protocol(format!("{service} meter reading cannot be projected"))
        })?;
        let raw: String = row.try_get("measured_quantity")?;
        quantities[index] = Decimal::from_str(&raw)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid meter quantity: {error}")))?;
    }
    Ok((quantities[0], quantities[1]))
}

async fn bind_invoice_meter_readings(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    invoice_id: Uuid,
    refs: &UtilityMeterReadingRefs,
) -> Result<(), sqlx::Error> {
    for (reading_id, service_code) in [
        (refs.electricity_reading_id, "ELECTRICITY"),
        (refs.water_reading_id, "WATER"),
    ] {
        sqlx::query(
            "INSERT INTO invoice_meter_readings (invoice_id, meter_reading_id, service_code)
             VALUES ($1, $2, $3) ON CONFLICT(invoice_id, service_code) DO NOTHING",
        )
        .bind(invoice_id.to_string())
        .bind(reading_id.to_string())
        .bind(service_code)
        .execute(&mut **tx)
        .await?;
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT meter_reading_id FROM invoice_meter_readings
             WHERE invoice_id = $1 AND service_code = $2",
        )
        .bind(invoice_id.to_string())
        .bind(service_code)
        .fetch_optional(&mut **tx)
        .await?;
        let expected = reading_id.to_string();
        if stored.as_deref() != Some(expected.as_str()) {
            return Err(sqlx::Error::Protocol(
                "invoice meter reading projection conflicts with local provenance".into(),
            ));
        }
    }
    Ok(())
}

fn response_line_quantity(
    response: &UtilityBillingResponse,
    service_code: &str,
) -> Option<Decimal> {
    response
        .lines
        .iter()
        .find(|line| line.service_code == service_code)
        .map(|line| line.quantity)
}

fn service_account_from_row(row: &SqliteRow) -> Result<ServiceAccount, sqlx::Error> {
    Ok(ServiceAccount {
        id: parse_local_uuid(row.try_get("id")?, "service account id")?,
        customer_id: parse_local_uuid(row.try_get("customer_id")?, "service account customer id")?,
        service_code: row.try_get("service_code")?,
        contract_ref: row.try_get("contract_ref")?,
        connection_status: row.try_get("connection_status")?,
        activated_at: row
            .try_get::<Option<String>, _>("activated_at")?
            .map(|value| NaiveDate::parse_from_str(&value, "%Y-%m-%d"))
            .transpose()
            .map_err(|error| {
                sqlx::Error::Protocol(format!("invalid service activation date: {error}"))
            })?,
        revision: row.try_get("revision")?,
    })
}

fn meter_from_row(row: &SqliteRow) -> Result<Meter, sqlx::Error> {
    Ok(Meter {
        id: parse_local_uuid(row.try_get("id")?, "meter id")?,
        service_account_id: parse_local_uuid(
            row.try_get("service_account_id")?,
            "meter service account id",
        )?,
        serial: row.try_get("serial")?,
        unit: row.try_get("unit")?,
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

fn meter_reading_from_row(row: &SqliteRow) -> Result<MeterReading, sqlx::Error> {
    let decimal = |name: &str| -> Result<Decimal, sqlx::Error> {
        let value: String = row.try_get(name)?;
        Decimal::from_str(&value).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid meter reading {name}: {error}"))
        })
    };
    Ok(MeterReading {
        id: parse_local_uuid(row.try_get("id")?, "meter reading id")?,
        meter_id: parse_local_uuid(row.try_get("meter_id")?, "meter id")?,
        period: row.try_get("period")?,
        previous: decimal("previous")?,
        current: decimal("current")?,
        measured_quantity: decimal("measured_quantity")?,
        source: row.try_get("source")?,
        corrected_from_id: row
            .try_get::<Option<String>, _>("corrected_from_id")?
            .map(|value| parse_local_uuid(value, "corrected reading id"))
            .transpose()?,
        correction_reason: row.try_get("correction_reason")?,
    })
}

fn parse_local_uuid(value: String, label: &str) -> Result<Uuid, sqlx::Error> {
    value
        .parse()
        .map_err(|error| sqlx::Error::Protocol(format!("invalid {label}: {error}")))
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
        "PAYMENT_ALLOCATED" => {
            sqlx::query(
                "UPDATE payments SET status = 'CONFLICT_REQUIRES_REVIEW'
                 WHERE id = $1 AND status = 'LOCAL_PENDING'",
            )
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
        "CREDIT_NOTE_RECORDED" => {
            sqlx::query(
                "UPDATE receivables SET status = 'CONFLICT_REQUIRES_REVIEW'
                 WHERE id = $1 AND status = 'CREDITED'",
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
        customer_category: row.try_get("customer_category")?,
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
        "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision
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

async fn apply_service_account_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let account_id = change
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("service account change has no id".into()))?;
    if account_id != entity_id {
        return Err(sqlx::Error::Protocol(
            "service account change id does not match its entity id".into(),
        ));
    }
    let customer_id = change
        .get("customer_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("service account change has no customer id".into()))?;
    let service_code = change
        .get("service_code")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            sqlx::Error::Protocol("service account change has no service code".into())
        })?;
    let connection_status = change
        .get("connection_status")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("service account change has no status".into()))?;
    if !matches!(service_code, "ELECTRICITY" | "WATER")
        || !matches!(connection_status, "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN")
    {
        return Err(sqlx::Error::Protocol(
            "service account change contains unsupported values".into(),
        ));
    }
    let customer_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND organization_id = $2)",
    )
    .bind(customer_id)
    .bind(organization_id.to_string())
    .fetch_one(&mut **tx)
    .await?;
    if !customer_exists {
        return Err(sqlx::Error::Protocol(
            "service account references a customer not yet synchronized".into(),
        ));
    }
    let incoming_revision = change
        .get("revision")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(revision);
    if incoming_revision < 1 || incoming_revision != revision {
        return Err(sqlx::Error::Protocol(
            "service account revision is invalid".into(),
        ));
    }
    let contract_ref = change
        .get("contract_ref")
        .and_then(serde_json::Value::as_str);
    let activated_at = change
        .get("activated_at")
        .and_then(serde_json::Value::as_str);
    sqlx::query(
        "INSERT INTO service_accounts
         (id, customer_id, service_code, contract_ref, connection_status, activated_at, revision)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(account_id)
    .bind(customer_id)
    .bind(service_code)
    .bind(contract_ref)
    .bind(connection_status)
    .bind(activated_at)
    .bind(incoming_revision)
    .execute(&mut **tx)
    .await?;
    let stored: (String, String, Option<String>, String, Option<String>, i64) = sqlx::query_as(
        "SELECT customer_id, service_code, contract_ref, connection_status, activated_at, revision
         FROM service_accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_one(&mut **tx)
    .await?;
    if stored
        != (
            customer_id.to_owned(),
            service_code.to_owned(),
            contract_ref.map(str::to_owned),
            connection_status.to_owned(),
            activated_at.map(str::to_owned),
            incoming_revision,
        )
    {
        let immutable_matches = stored.0 == customer_id && stored.1 == service_code;
        if !immutable_matches {
            return Err(sqlx::Error::Protocol(
                "service account id was reused with different immutable data".into(),
            ));
        }
        if stored.5 >= incoming_revision {
            if stored.5 == incoming_revision
                && (stored.2.as_deref() != contract_ref
                    || stored.3 != connection_status
                    || stored.4.as_deref() != activated_at)
            {
                return Err(sqlx::Error::Protocol(
                    "service account has divergent data at the same revision".into(),
                ));
            }
            return Ok(());
        }
        sqlx::query("UPDATE service_accounts SET contract_ref = $1, connection_status = $2, activated_at = $3, revision = $4 WHERE id = $5 AND revision = $6")
            .bind(contract_ref)
            .bind(connection_status)
            .bind(activated_at)
            .bind(incoming_revision)
            .bind(account_id)
            .bind(stored.5)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn apply_meter_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let meter_id = change
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("meter change has no id".into()))?;
    if meter_id != entity_id {
        return Err(sqlx::Error::Protocol(
            "meter change id does not match its entity id".into(),
        ));
    }
    let account_id = change
        .get("service_account_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("meter change has no service account id".into()))?;
    let serial = change
        .get("serial")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("meter change has no serial".into()))?;
    let unit = change
        .get("unit")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("meter change has no unit".into()))?;
    let active = change
        .get("active")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| sqlx::Error::Protocol("meter change has no active flag".into()))?;
    let service_code: Option<String> = sqlx::query_scalar(
        "SELECT sa.service_code FROM service_accounts sa
         JOIN customers c ON c.id = sa.customer_id
         WHERE sa.id = $1 AND c.organization_id = $2",
    )
    .bind(account_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    let expected_unit = match service_code.as_deref() {
        Some("ELECTRICITY") => "KWH",
        Some("WATER") => "M3",
        _ => {
            return Err(sqlx::Error::Protocol(
                "meter references a service account not yet synchronized".into(),
            ))
        }
    };
    if unit != expected_unit {
        return Err(sqlx::Error::Protocol(
            "meter unit does not match its service account".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO meters (id, service_account_id, serial, unit, active)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT(id) DO NOTHING",
    )
    .bind(meter_id)
    .bind(account_id)
    .bind(serial)
    .bind(unit)
    .bind(i64::from(active))
    .execute(&mut **tx)
    .await?;
    let stored: (String, String, String, i64) =
        sqlx::query_as("SELECT service_account_id, serial, unit, active FROM meters WHERE id = $1")
            .bind(meter_id)
            .fetch_one(&mut **tx)
            .await?;
    if stored
        != (
            account_id.to_owned(),
            serial.to_owned(),
            unit.to_owned(),
            i64::from(active),
        )
    {
        return Err(sqlx::Error::Protocol(
            "meter id was reused with different immutable data".into(),
        ));
    }
    let _ = revision;
    Ok(())
}

async fn apply_meter_reading_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    _revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let reading: MeterReading = serde_json::from_value(change.clone())
        .map_err(|error| sqlx::Error::Protocol(format!("invalid meter reading change: {error}")))?;
    if reading.id.to_string() != entity_id
        || chrono::NaiveDate::parse_from_str(&format!("{}-01", reading.period), "%Y-%m-%d").is_err()
        || reading.previous.is_sign_negative()
        || reading.current < reading.previous
        || reading.measured_quantity != reading.current - reading.previous
        || reading.source.trim().is_empty()
    {
        return Err(sqlx::Error::Protocol(
            "meter reading change violates its immutable invariants".into(),
        ));
    }
    let meter_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM meters m
         JOIN service_accounts sa ON sa.id = m.service_account_id
         JOIN customers c ON c.id = sa.customer_id
         WHERE m.id = $1 AND c.organization_id = $2)",
    )
    .bind(reading.meter_id.to_string())
    .bind(organization_id.to_string())
    .fetch_one(&mut **tx)
    .await?;
    if !meter_exists {
        return Err(sqlx::Error::Protocol(
            "meter reading references a meter not yet synchronized".into(),
        ));
    }
    if let Some(parent_id) = reading.corrected_from_id {
        let parent_valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meter_readings
             WHERE id = $1 AND meter_id = $2 AND period = $3)",
        )
        .bind(parent_id.to_string())
        .bind(reading.meter_id.to_string())
        .bind(&reading.period)
        .fetch_one(&mut **tx)
        .await?;
        if !parent_valid
            || reading
                .correction_reason
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
        {
            return Err(sqlx::Error::Protocol(
                "meter reading correction has no valid predecessor or reason".into(),
            ));
        }
    }
    sqlx::query(
        "INSERT INTO meter_readings
         (id, meter_id, period, previous, current, measured_quantity, source,
          corrected_from_id, correction_reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(reading.id.to_string())
    .bind(reading.meter_id.to_string())
    .bind(&reading.period)
    .bind(reading.previous.to_string())
    .bind(reading.current.to_string())
    .bind(reading.measured_quantity.to_string())
    .bind(&reading.source)
    .bind(reading.corrected_from_id.map(|id| id.to_string()))
    .bind(&reading.correction_reason)
    .execute(&mut **tx)
    .await?;
    let stored = sqlx::query(
        "SELECT id, meter_id, period, previous, current, measured_quantity, source,
                corrected_from_id, correction_reason
         FROM meter_readings WHERE id = $1",
    )
    .bind(reading.id.to_string())
    .fetch_one(&mut **tx)
    .await?;
    let stored_reading = meter_reading_from_row(&stored)?;
    if serde_json::to_value(stored_reading).map_err(|error| {
        sqlx::Error::Protocol(format!("invalid stored meter reading JSON: {error}"))
    })? != serde_json::to_value(reading).map_err(|error| {
        sqlx::Error::Protocol(format!("invalid pulled meter reading JSON: {error}"))
    })? {
        return Err(sqlx::Error::Protocol(
            "meter reading id was reused with different immutable data".into(),
        ));
    }
    Ok(())
}

async fn apply_customer_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    customer_id: &str,
    revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let existing = sqlx::query(
        "SELECT revision, customer_code, legal_name, customer_category, contact_fields_json, status
         FROM customers WHERE id = $1 AND organization_id = $2",
    )
    .bind(customer_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    if existing
        .as_ref()
        .is_some_and(|row| row.get::<i64, _>("revision") >= revision)
    {
        return Ok(());
    }
    let customer_code = change
        .get("customer_code")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("customer_code").ok())
        })
        .unwrap_or_else(|| customer_id.to_owned());
    let current_contact = existing
        .as_ref()
        .and_then(|row| row.try_get::<String, _>("contact_fields_json").ok())
        .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
        .unwrap_or_else(|| json!({}));
    let supports_explicit_clear = change
        .get("patch_format_version")
        .and_then(serde_json::Value::as_u64)
        == Some(u64::from(CUSTOMER_PATCH_FORMAT_VERSION));
    let customer_contact_value = |key: &str| match change.get(key) {
        Some(value) if !value.is_null() || supports_explicit_clear => value.clone(),
        Some(_) | None => current_contact
            .get(key)
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    };
    let legal_name = change
        .get("legal_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("legal_name").ok())
        })
        .unwrap_or_else(|| "Client synchronisé".into());
    let status = change
        .get("status")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("status").ok())
        })
        .unwrap_or_else(|| "ACTIVE".into());
    let customer_category = match change.get("customer_category") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(category)) => {
            let category = category.trim().to_ascii_uppercase();
            (!category.is_empty()).then_some(category)
        }
        Some(_) | None => existing.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("customer_category")
                .ok()
                .flatten()
        }),
    };
    let contact = json!({
        "address": customer_contact_value("address"),
        "phone": customer_contact_value("phone")
    });
    sqlx::query(
        "INSERT INTO customers
         (id, organization_id, customer_code, legal_name, customer_category, contact_fields_json, status, revision, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET legal_name = excluded.legal_name,
           customer_category = excluded.customer_category,
           contact_fields_json = excluded.contact_fields_json, status = excluded.status,
           revision = excluded.revision",
    )
    .bind(customer_id)
    .bind(organization_id.to_string())
    .bind(&customer_code)
    .bind(&legal_name)
    .bind(customer_category)
    .bind(contact.to_string())
    .bind(&status)
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
    let mut response: UtilityBillingResponse = serde_json::from_value(
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
    if !response.billing_period.is_empty() && response.billing_period != request.billing_period {
        return Err(sqlx::Error::Protocol(
            "invoice sync period does not match its request".into(),
        ));
    }
    if response.due_date.is_some() && response.due_date != request.due_date {
        return Err(sqlx::Error::Protocol(
            "invoice sync due date does not match its request".into(),
        ));
    }
    response.billing_period = request.billing_period.clone();
    response.due_date = request.due_date;
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
    let document_identity = change
        .get("identity_snapshot")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let _: InvoiceIdentitySnapshot =
        serde_json::from_value(document_identity.clone()).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid invoice identity snapshot: {error}"))
        })?;
    let expected_hash = utility_snapshot_sha256(&response)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice response: {error}")))?;
    if response.snapshot_sha256 != expected_hash {
        return Err(sqlx::Error::Protocol(
            "invoice sync snapshot hash is invalid".into(),
        ));
    }
    if let Some(refs) = &request.meter_readings {
        let quantities = verify_invoice_readings_projection(
            tx,
            organization_id,
            request.customer_id,
            &request.billing_period,
            response.invoice_id,
            refs,
        )
        .await?;
        if response_line_quantity(&response, "ELECTRICITY") != Some(quantities.0)
            || response_line_quantity(&response, "WATER") != Some(quantities.1)
            || request.electricity.quantity != quantities.0
            || request.water.quantity != quantities.1
        {
            return Err(sqlx::Error::Protocol(
                "synced invoice quantities do not match the meter readings".into(),
            ));
        }
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
        let existing_hash = utility_snapshot_sha256(&existing_response)
            .map_err(|error| sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}")))?;
        if existing_customer != request.customer_id.to_string()
            || existing_hash != expected_hash
            || (!existing_response.billing_period.is_empty()
                && existing_response.billing_period != request.billing_period)
            || (existing_response.due_date.is_some()
                && existing_response.due_date != request.due_date)
        {
            return Err(sqlx::Error::Protocol(
                "conflicting local invoice projection".into(),
            ));
        }
        existing_response.billing_period = request.billing_period.clone();
        existing_response.due_date = request.due_date;
        sqlx::query(
            "UPDATE invoices SET immutable_snapshot_json = $1, status = $2,
             version = CASE WHEN version < $3 THEN $3 ELSE version END WHERE id = $4",
        )
        .bind(serde_json::to_string(&existing_response).map_err(|error| {
            sqlx::Error::Protocol(format!("invalid reconciled invoice snapshot: {error}"))
        })?)
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
        if let Some(refs) = &request.meter_readings {
            bind_invoice_meter_readings(tx, response.invoice_id, refs).await?;
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO invoices
         (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot_json, status, version, document_snapshot_json)
         VALUES ($1, $2, $3, datetime('now'), $4, $5, 'v2-a4-2', $6, $7, $8, $9)",
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
    .bind(document_identity.to_string())
    .execute(&mut **tx)
    .await?;
    for line in &response.lines {
        insert_invoice_line(tx, response.invoice_id, line).await?;
    }
    if let Some(refs) = &request.meter_readings {
        bind_invoice_meter_readings(tx, response.invoice_id, refs).await?;
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
        "INSERT INTO invoice_voids (id, invoice_id, reason, actor_id, created_at)
         VALUES ($1, $2, $3, $4, datetime('now'))
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
    entity_id: &str,
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
    if payment_id != entity_id {
        return Err(sqlx::Error::Protocol(
            "payment sync entity id does not match payment id".into(),
        ));
    }
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
            let allocation_id = allocation
                .get("allocation_id")
                .and_then(serde_json::Value::as_str);
            if let Some(allocation_id) = allocation_id {
                if let Some(existing) = sqlx::query(
                    "SELECT payment_id, receivable_id, schedule_id, allocated_amount, currency
                     FROM payment_allocations WHERE id = $1",
                )
                .bind(allocation_id)
                .fetch_optional(&mut **tx)
                .await?
                {
                    let same = existing.try_get::<String, _>("payment_id")? == payment_id
                        && existing.try_get::<String, _>("receivable_id")? == receivable_id
                        && existing.try_get::<Option<String>, _>("schedule_id")?
                            == schedule_id.map(str::to_owned)
                        && existing.try_get::<String, _>("allocated_amount")? == allocated_amount
                        && existing.try_get::<String, _>("currency")? == currency;
                    if !same {
                        return Err(sqlx::Error::Protocol(
                            "conflicting local payment allocation projection".into(),
                        ));
                    }
                    continue;
                }

                // A local pending payment already owns a semantically identical
                // allocation, but it may have generated a local UUID. Once the
                // central projection supplies the durable allocation identity,
                // reconcile that row instead of inserting a second allocation.
                let matches = sqlx::query(
                    "SELECT id FROM payment_allocations
                     WHERE payment_id = $1 AND receivable_id = $2
                       AND allocated_amount = $3 AND currency = $4
                       AND (schedule_id = $5 OR (schedule_id IS NULL AND $5 IS NULL))
                     LIMIT 2",
                )
                .bind(payment_id)
                .bind(receivable_id)
                .bind(allocated_amount)
                .bind(currency)
                .bind(schedule_id)
                .fetch_all(&mut **tx)
                .await?;
                if matches.len() == 1 {
                    let local_id: String = matches[0].try_get("id")?;
                    sqlx::query("UPDATE payment_allocations SET id = $1 WHERE id = $2")
                        .bind(allocation_id)
                        .bind(local_id)
                        .execute(&mut **tx)
                        .await?;
                    continue;
                }
                if matches.len() > 1 {
                    return Err(sqlx::Error::Protocol(
                        "ambiguous local payment allocation projection".into(),
                    ));
                }
            }
            let already_present: i64 = if let Some(allocation_id) = allocation_id {
                sqlx::query_scalar("SELECT count(*) FROM payment_allocations WHERE id = $1")
                    .bind(allocation_id)
                    .fetch_one(&mut **tx)
                    .await?
            } else {
                sqlx::query_scalar(
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
                .await?
            };
            if already_present == 0 {
                let allocation_id = allocation_id
                    .map(str::to_owned)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                sqlx::query(
                    "INSERT INTO payment_allocations
                     (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(allocation_id)
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
        let allocation_id = allocation
            .get("allocation_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query(
            "INSERT INTO payment_allocations
             (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(allocation_id)
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

async fn apply_credit_note_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    organization_id: Uuid,
    entity_id: &str,
    _revision: i64,
    change: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    let credit_note_id = change
        .get("credit_note_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("credit note id is missing".into()))?;
    if credit_note_id != entity_id {
        return Err(sqlx::Error::Protocol(
            "credit note entity id does not match credit note id".into(),
        ));
    }
    let receivable_id = change
        .get("receivable_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("credit note receivable id is missing".into()))?;
    let amount = change
        .get("amount")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| sqlx::Error::Protocol("credit note amount is missing".into()))?;
    let reason = change
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .ok_or_else(|| sqlx::Error::Protocol("credit note reason is missing".into()))?;
    let actor_id = change
        .get("actor_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("CENTRAL");
    let receivable_exists: Option<String> = sqlx::query_scalar(
        "SELECT r.id FROM receivables r
         JOIN customers c ON c.id = r.customer_id
         WHERE r.id = $1 AND c.organization_id = $2",
    )
    .bind(receivable_id)
    .bind(organization_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    if receivable_exists.is_none() {
        return Err(sqlx::Error::Protocol(
            "credit note references a receivable not present locally".into(),
        ));
    }
    if let Some(existing) = sqlx::query(
        "SELECT receivable_id, amount, reason, initiated_by
         FROM credit_notes WHERE id = $1",
    )
    .bind(credit_note_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let same = existing.try_get::<String, _>("receivable_id")? == receivable_id
            && existing.try_get::<String, _>("amount")? == amount
            && existing.try_get::<String, _>("reason")? == reason
            && existing.try_get::<String, _>("initiated_by")? == actor_id;
        if !same {
            return Err(sqlx::Error::Protocol(
                "conflicting local credit note projection".into(),
            ));
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO credit_notes
         (id, receivable_id, amount, reason, initiated_by, created_at)
         VALUES ($1, $2, $3, $4, $5, datetime('now'))",
    )
    .bind(credit_note_id)
    .bind(receivable_id)
    .bind(amount)
    .bind(reason)
    .bind(actor_id)
    .execute(&mut **tx)
    .await?;
    if let Some(status) = change
        .get("receivable_status")
        .and_then(serde_json::Value::as_str)
    {
        sqlx::query("UPDATE receivables SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(receivable_id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO audit_events
         (id, actor, event_type, subject_id, occurred_at, redacted_metadata_json)
         VALUES ($1, $2, 'CREDIT_NOTE_RECORDED', $3, datetime('now'), $4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(actor_id)
    .bind(receivable_id)
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

fn parse_uuid_column(row: &SqliteRow, column: &str, label: &str) -> Result<Uuid, sqlx::Error> {
    let value: String = row.try_get(column)?;
    value
        .parse::<Uuid>()
        .map_err(|error| sqlx::Error::Protocol(format!("invalid {label} id: {error}")))
}

fn required_catalog_text(value: &serde_json::Value, field: &str) -> Result<String, sqlx::Error> {
    optional_catalog_text(value, field)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| sqlx::Error::Protocol(format!("catalog snapshot field {field} is missing")))
}

fn optional_catalog_text<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .or_else(|| value.get("payload").and_then(|payload| payload.get(field)))
        .and_then(serde_json::Value::as_str)
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
