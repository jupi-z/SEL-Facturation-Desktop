use chrono::NaiveDate;
use rust_decimal::Decimal;
use sel_contracts::{
    ActivateSolarContractRequest, CreateCustomer, CreateSolarSaleRequest, Customer,
    PaymentSyncPayload, RecordPaymentRequest, SolarContractSyncPayload, SolarSaleResponse,
    SolarSaleSyncPayload, SyncEnvelope, SyncEventResult, UpdateCustomer, UtilityBillingRequest,
    UtilityBillingResponse, UtilityInvoiceSyncPayload, solar_schedule_id,
    utility_snapshot_sha256,
};
use sel_domain::billing;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone)]
pub struct CentralStore {
    pub pool: PgPool,
    pub organization_id: Uuid,
}

#[derive(Clone, Debug)]
pub enum SolarSaleWriteOutcome {
    Created(SolarSaleResponse),
    Replay(SolarSaleResponse),
}

#[derive(Debug, Error)]
pub enum SolarSaleWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("customer does not belong to the organization")]
    CustomerNotFound,
    #[error("active solar product was not found")]
    ProductNotFound,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub enum SolarContractWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

#[derive(Debug, Error)]
pub enum SolarContractWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("the referenced sale or receivable was not found")]
    SaleNotFound,
    #[error("the sale already has an installment contract")]
    ContractAlreadyExists,
    #[error("schedule currency does not match the receivable")]
    CurrencyMismatch,
    #[error("invalid solar schedule: {0}")]
    InvalidSchedule(String),
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Default)]
pub struct TariffSnapshot {
    pub electricity_unit_price: Option<Decimal>,
    pub electricity_tax_rate: Option<Decimal>,
    pub water_unit_price: Option<Decimal>,
    pub water_tax_rate: Option<Decimal>,
}

#[derive(Debug, Error)]
pub enum UtilityIssueError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("invoice period already exists")]
    InvoicePeriodExists,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

pub enum UtilityIssueOutcome {
    Created(UtilityBillingResponse),
    Replay(UtilityBillingResponse),
}

#[derive(Debug, Error)]
pub enum PaymentWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("receivable balance would be overallocated")]
    Overallocated,
    #[error("payment customer or currency does not match the receivable")]
    ContextInvalid,
    #[error("payment allocations exceed the received amount")]
    AllocationExceedsReceipt,
    #[error("payment amount and allocations must be strictly positive")]
    InvalidAllocation,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

pub enum PaymentWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

#[derive(Debug, Error)]
pub enum SyncApplyError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid stored sync result: {0}")]
    StoredResult(#[from] serde_json::Error),
}

impl CentralStore {
    pub async fn connect(database_url: &str, organization_id: Uuid) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await?;
        Ok(Self {
            pool,
            organization_id,
        })
    }

    pub fn from_pool(pool: PgPool, organization_id: Uuid) -> Self {
        Self {
            pool,
            organization_id,
        }
    }

    pub async fn ensure_organization(&self, legal_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO organizations (id, legal_name) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING",
        )
        .bind(self.organization_id)
        .bind(legal_name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_customer(&self, input: &CreateCustomer) -> Result<Customer, sqlx::Error> {
        let id = Uuid::new_v4();
        let contact_fields = json!({
            "address": input.address,
            "phone": input.phone,
        });
        let row = sqlx::query(
            "INSERT INTO customers
             (id, organization_id, customer_code, legal_name, contact_fields, status, revision)
             VALUES ($1, $2, $3, $4, $5, 'ACTIVE', 1)
             RETURNING id, organization_id, customer_code, legal_name, contact_fields, status, revision",
        )
        .bind(id)
        .bind(self.organization_id)
        .bind(&input.customer_code)
        .bind(&input.legal_name)
        .bind(contact_fields)
        .fetch_one(&self.pool)
        .await?;
        customer_from_row(&row)
    }

    pub async fn get_customer(&self, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, contact_fields, status, revision
             FROM customers WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| customer_from_row(&row)).transpose()
    }

    pub async fn list_customers(&self) -> Result<Vec<Customer>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, contact_fields, status, revision
             FROM customers WHERE organization_id = $1 ORDER BY customer_code",
        )
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(customer_from_row).collect()
    }

    pub async fn update_customer(
        &self,
        id: Uuid,
        expected_revision: i64,
        input: &UpdateCustomer,
    ) -> Result<Option<Customer>, sqlx::Error> {
        let Some(current) = self.get_customer(id).await? else {
            return Ok(None);
        };
        let legal_name = input.legal_name.clone().unwrap_or(current.legal_name);
        let address = input.address.clone().or(current.address);
        let phone = input.phone.clone().or(current.phone);
        let status = input.status.clone().unwrap_or(current.status);
        let contact_fields = json!({"address": address, "phone": phone});
        let row = sqlx::query(
            "UPDATE customers
             SET legal_name = $1, contact_fields = $2, status = $3, revision = revision + 1
             WHERE id = $4 AND organization_id = $5 AND revision = $6
             RETURNING id, organization_id, customer_code, legal_name, contact_fields, status, revision",
        )
        .bind(legal_name)
        .bind(contact_fields)
        .bind(status)
        .bind(id)
        .bind(self.organization_id)
        .bind(expected_revision)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| customer_from_row(&row)).transpose()
    }

    pub async fn load_tariffs(
        &self,
        revision: &str,
        currency: &str,
        period: &str,
    ) -> Result<TariffSnapshot, sqlx::Error> {
        let effective_date = NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
            .map_err(|error| sqlx::Error::Protocol(format!("invalid billing period: {error}")))?;
        let electricity = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'ELECTRICITY' AND unit = 'KWH' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED'
               AND effective_from <= $3 AND (effective_to IS NULL OR effective_to >= $3)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(currency)
        .bind(revision)
        .bind(effective_date)
        .fetch_optional(&self.pool)
        .await?;
        let electricity_tax = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'ELECTRICITY' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(effective_date)
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
        .bind(effective_date)
        .fetch_optional(&self.pool)
        .await?;
        let water_tax = sqlx::query(
            "SELECT rate FROM tax_profile_versions
             WHERE service_code = 'WATER' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY effective_from DESC LIMIT 1",
        )
        .bind(effective_date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(TariffSnapshot {
            electricity_unit_price: electricity
                .as_ref()
                .map(|row| row.try_get("unit_price"))
                .transpose()?,
            electricity_tax_rate: electricity_tax
                .as_ref()
                .map(|row| row.try_get("rate"))
                .transpose()?,
            water_unit_price: water
                .as_ref()
                .map(|row| row.try_get("unit_price"))
                .transpose()?,
            water_tax_rate: water_tax
                .as_ref()
                .map(|row| row.try_get("rate"))
                .transpose()?,
        })
    }

    pub async fn issue_utility_invoice(
        &self,
        input: &UtilityBillingRequest,
        response: &UtilityBillingResponse,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<UtilityIssueOutcome, UtilityIssueError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2 AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("billing.utility.issue")
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(UtilityIssueError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(UtilityIssueOutcome::Replay(serde_json::from_value(original)?));
        }
        let duplicate = sqlx::query(
            "SELECT id FROM invoices WHERE customer_id = $1 AND period = $2 AND status <> 'VOID' FOR UPDATE",
        )
        .bind(input.customer_id)
        .bind(&input.billing_period)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate.is_some() {
            return Err(UtilityIssueError::InvoicePeriodExists);
        }
        let customer_exists = sqlx::query(
            "SELECT id FROM customers WHERE id = $1 AND organization_id = $2 FOR SHARE",
        )
        .bind(input.customer_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !customer_exists {
            return Err(sqlx::Error::RowNotFound.into());
        }
        let receivable_id = response
            .receivable_id
            .expect("issued utility response must contain a receivable id");
        let snapshot = serde_json::to_value(response)?;
        sqlx::query(
            "INSERT INTO invoices
             (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot, status, version)
             VALUES ($1, $2, $3, now(), $4, $5, $6, $7, 'CENTRAL_ACCEPTED', 1)",
        )
        .bind(response.invoice_id)
        .bind(input.customer_id)
        .bind(&input.billing_period)
        .bind(&response.currency)
        .bind(response.total)
        .bind("v2-a4-1")
        .bind(snapshot.clone())
        .execute(&mut *tx)
        .await?;
        for line in &response.lines {
            sqlx::query(
                "INSERT INTO invoice_lines
                 (id, invoice_id, service_code, quantity, unit, unit_price, subtotal, tax_snapshot, total)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(Uuid::new_v4())
            .bind(response.invoice_id)
            .bind(&line.service_code)
            .bind(line.quantity)
            .bind(&line.unit)
            .bind(line.unit_price)
            .bind(line.subtotal)
            .bind(json!({"tax": line.tax}))
            .bind(line.total)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'INVOICE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id)
        .bind(response.invoice_id)
        .bind(input.customer_id)
        .bind(&response.currency)
        .bind(response.total)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key, request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 201, $6)",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("billing.utility.issue")
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(snapshot)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(UtilityIssueOutcome::Created(response.clone()))
    }

    pub async fn get_invoice_response(
        &self,
        id: Uuid,
    ) -> Result<Option<UtilityBillingResponse>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT immutable_snapshot FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let snapshot: serde_json::Value = row.try_get("immutable_snapshot")?;
            serde_json::from_value(snapshot).map_err(|error| {
                sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
            })
        })
        .transpose()
    }

    pub async fn get_receivable_summary(
        &self,
        id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT r.id, r.customer_id, r.currency, r.principal_total,
                    COALESCE(SUM(pa.allocated_amount), 0) AS paid
             FROM receivables r
             LEFT JOIN payment_allocations pa ON pa.receivable_id = r.id
             WHERE r.id = $1 AND r.customer_id IN
                 (SELECT id FROM customers WHERE organization_id = $2)
             GROUP BY r.id, r.customer_id, r.currency, r.principal_total",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let principal: Decimal = row.try_get("principal_total")?;
            let paid: Decimal = row.try_get("paid")?;
            let principal = principal.round_dp(2);
            let paid = paid.round_dp(2);
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id")?,
                "customer_id": row.try_get::<Uuid, _>("customer_id")?,
                "currency": row.try_get::<String, _>("currency")?,
                "principal": principal,
                "paid": paid,
                "balance": (principal - paid).round_dp(2),
                "status": if paid >= principal { "SETTLED" } else { "OPEN" }
            }))
        })
        .transpose()
    }

    pub async fn record_payment(
        &self,
        input: &RecordPaymentRequest,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentWriteOutcome, PaymentWriteError> {
        self.record_payment_with_id(input, Uuid::new_v4(), idempotency_key, request_digest)
            .await
    }

    pub async fn record_payment_with_id(
        &self,
        input: &RecordPaymentRequest,
        payment_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentWriteOutcome, PaymentWriteError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2 AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("payment.record")
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(PaymentWriteError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(PaymentWriteOutcome::Replay(serde_json::from_value(original)?));
        }
        if input.received_amount <= Decimal::ZERO {
            return Err(PaymentWriteError::InvalidAllocation);
        }
        let mut grouped_allocations = BTreeMap::<Uuid, Decimal>::new();
        let mut persisted_allocations = BTreeMap::<(Uuid, Option<Uuid>), Decimal>::new();
        for allocation in &input.allocations {
            if allocation.amount <= Decimal::ZERO {
                return Err(PaymentWriteError::InvalidAllocation);
            }
            *grouped_allocations
                .entry(allocation.receivable_id)
                .or_insert(Decimal::ZERO) += allocation.amount;
            *persisted_allocations
                .entry((allocation.receivable_id, allocation.schedule_id))
                .or_insert(Decimal::ZERO) += allocation.amount;
        }
        let allocated_total: Decimal = grouped_allocations.values().copied().sum();
        if allocated_total > input.received_amount {
            return Err(PaymentWriteError::AllocationExceedsReceipt);
        }
        for (receivable_id, allocated_amount) in &grouped_allocations {
            let row = sqlx::query(
                "SELECT r.principal_total, r.currency, r.customer_id
                 FROM receivables r
                 WHERE r.id = $1 AND r.customer_id = $2
                   AND r.customer_id IN (SELECT id FROM customers WHERE organization_id = $3)
                 FOR UPDATE",
            )
            .bind(receivable_id)
            .bind(input.customer_id)
            .bind(self.organization_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(row) = row else {
                return Err(PaymentWriteError::ContextInvalid);
            };
            let currency: String = row.try_get("currency")?;
            if currency != input.currency {
                return Err(PaymentWriteError::ContextInvalid);
            }
            let principal: Decimal = row.try_get("principal_total")?;
            let paid: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE receivable_id = $1",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            if paid + allocated_amount > principal {
                return Err(PaymentWriteError::Overallocated);
            }
            for ((allocation_receivable_id, schedule_id), _) in persisted_allocations
                .iter()
                .filter(|((allocation_receivable_id, _), _)| {
                    allocation_receivable_id == receivable_id
                })
            {
                if let Some(schedule_id) = schedule_id {
                    let schedule_exists = sqlx::query(
                        "SELECT s.id FROM installment_schedule s
                         JOIN installment_plans p ON p.id = s.plan_id
                         WHERE s.id = $1 AND p.receivable_id = $2
                         FOR SHARE",
                    )
                    .bind(schedule_id)
                    .bind(receivable_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .is_some();
                    if !schedule_exists {
                        return Err(PaymentWriteError::ContextInvalid);
                    }
                }
            }
        }
        sqlx::query(
            "INSERT INTO payments
             (id, customer_id, amount_received, currency, method, paid_at, status)
             VALUES ($1, $2, $3, $4, $5, $6, 'CENTRAL_ACCEPTED')",
        )
        .bind(payment_id)
        .bind(input.customer_id)
        .bind(input.received_amount)
        .bind(&input.currency)
        .bind(&input.method)
        .bind(input.occurred_at)
        .execute(&mut *tx)
        .await?;
        for ((receivable_id, schedule_id), allocated_amount) in persisted_allocations {
            sqlx::query(
                "INSERT INTO payment_allocations
                 (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4())
            .bind(payment_id)
            .bind(receivable_id)
            .bind(schedule_id)
            .bind(allocated_amount)
            .bind(&input.currency)
            .execute(&mut *tx)
            .await?;
        }
        let body = json!({
            "payment_id": payment_id,
            "status": "CENTRAL_ACCEPTED",
            "amount": input.received_amount,
            "currency": input.currency
        });
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key, request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 201, $6)",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("payment.record")
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PaymentWriteOutcome::Created(body))
    }

    pub async fn customer_statement(
        &self,
        customer_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let customer_exists =
            sqlx::query("SELECT id FROM customers WHERE id = $1 AND organization_id = $2")
                .bind(customer_id)
                .bind(self.organization_id)
                .fetch_optional(&self.pool)
                .await?
                .is_some();
        if !customer_exists {
            return Ok(None);
        }
        let rows = sqlx::query(
            "SELECT r.id, r.currency, r.principal_total,
                    COALESCE(SUM(pa.allocated_amount), 0) AS paid
             FROM receivables r
             LEFT JOIN payment_allocations pa ON pa.receivable_id = r.id
             WHERE r.customer_id = $1
             GROUP BY r.id, r.currency, r.principal_total
             ORDER BY r.id",
        )
        .bind(customer_id)
        .fetch_all(&self.pool)
        .await?;
        let receivables = rows
            .iter()
            .map(|row| {
                let principal: Decimal = row.try_get("principal_total")?;
                let paid: Decimal = row.try_get("paid")?;
                let principal = principal.round_dp(2);
                let paid = paid.round_dp(2);
                Ok(json!({
                    "receivable_id": row.try_get::<Uuid, _>("id")?,
                    "currency": row.try_get::<String, _>("currency")?,
                    "principal": principal,
                    "paid": paid,
                    "balance": (principal - paid).round_dp(2)
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok(Some(
            json!({"customer_id": customer_id, "receivables": receivables}),
        ))
    }

    pub async fn create_solar_sale(
        &self,
        input: &CreateSolarSaleRequest,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarSaleWriteOutcome, SolarSaleWriteError> {
        self.create_solar_sale_with_ids(
            input,
            Uuid::new_v4(),
            Uuid::new_v4(),
            idempotency_key,
            request_digest,
        )
        .await
    }

    pub async fn create_solar_sale_with_ids(
        &self,
        input: &CreateSolarSaleRequest,
        sale_id: Uuid,
        receivable_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarSaleWriteOutcome, SolarSaleWriteError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2 AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("solar.sale.create")
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(SolarSaleWriteError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(SolarSaleWriteOutcome::Replay(serde_json::from_value(original)?));
        }
        let customer_exists = sqlx::query(
            "SELECT id FROM customers WHERE id = $1 AND organization_id = $2 FOR SHARE",
        )
        .bind(input.customer_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !customer_exists {
            return Err(SolarSaleWriteError::CustomerNotFound);
        }
        let product_exists = sqlx::query(
            "SELECT id FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT' AND currency = $2 AND active = TRUE",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !product_exists {
            return Err(SolarSaleWriteError::ProductNotFound);
        }
        let response = SolarSaleResponse {
            sale_id,
            receivable_id,
            customer_id: input.customer_id,
            principal: input.principal.round_dp(2),
            currency: input.currency.clone(),
            status: "CENTRAL_ACCEPTED".into(),
            product_code: Some(input.product_code.trim().to_owned()),
        };
        let snapshot = serde_json::to_value(&response)?;
        sqlx::query(
            "INSERT INTO sales
             (id, customer_id, currency, sold_at, sale_type, status)
             VALUES ($1, $2, $3, now(), 'SOLAR_KIT', 'CENTRAL_ACCEPTED')",
        )
        .bind(sale_id)
        .bind(input.customer_id)
        .bind(&input.currency)
        .execute(&mut *tx)
        .await?;
        let product_id: Uuid = sqlx::query(
            "SELECT id FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT' AND currency = $2 AND active = TRUE",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_one(&mut *tx)
        .await?
        .try_get("id")?;
        sqlx::query(
            "INSERT INTO sale_lines
             (id, sale_id, product_id, quantity, unit_price, tax_snapshot, total)
             VALUES ($1, $2, $3, 1, $4, '{}'::jsonb, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(sale_id)
        .bind(product_id)
        .bind(response.principal)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'SALE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id)
        .bind(sale_id)
        .bind(input.customer_id)
        .bind(&input.currency)
        .bind(response.principal)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key, request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 201, $6)",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("solar.sale.create")
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(snapshot)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(SolarSaleWriteOutcome::Created(response))
    }

    pub async fn activate_solar_contract(
        &self,
        contract_id: Uuid,
        input: &ActivateSolarContractRequest,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarContractWriteOutcome, SolarContractWriteError> {
        self.activate_solar_contract_with_plan_id(
            contract_id,
            Uuid::new_v4(),
            input,
            idempotency_key,
            request_digest,
        )
        .await
    }

    pub async fn activate_solar_contract_with_plan_id(
        &self,
        contract_id: Uuid,
        plan_id: Uuid,
        input: &ActivateSolarContractRequest,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarContractWriteOutcome, SolarContractWriteError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2 AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("solar.contract.activate")
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(SolarContractWriteError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(SolarContractWriteOutcome::Replay(original));
        }
        let receivable = sqlx::query(
            "SELECT r.customer_id, r.currency, r.principal_total, s.contract_id
             FROM receivables r
             JOIN sales s ON s.id = r.source_id
             JOIN customers c ON c.id = r.customer_id
             WHERE r.id = $1 AND r.source_id = $2 AND r.source_type = 'SALE'
               AND c.organization_id = $3
             FOR UPDATE OF r, s",
        )
        .bind(input.receivable_id)
        .bind(input.sale_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(receivable) = receivable else {
            return Err(SolarContractWriteError::SaleNotFound);
        };
        let existing_contract: Option<Uuid> = receivable.try_get("contract_id")?;
        if existing_contract.is_some() {
            return Err(SolarContractWriteError::ContractAlreadyExists);
        }
        let currency: String = receivable.try_get("currency")?;
        let principal: Decimal = receivable.try_get("principal_total")?;
        if currency != input.currency {
            return Err(SolarContractWriteError::CurrencyMismatch);
        }
        if input.schedule.is_empty()
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
                != principal.round_dp(2)
        {
            return Err(SolarContractWriteError::InvalidSchedule(
                "schedule amounts must be non-negative and equal the sale principal".into(),
            ));
        }
        let contract_exists = sqlx::query("SELECT id FROM contracts WHERE id = $1")
            .bind(contract_id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if contract_exists {
            return Err(SolarContractWriteError::ContractAlreadyExists);
        }
        sqlx::query(
            "INSERT INTO contracts
             (id, customer_id, contract_type, status, terms_revision, signed_at)
             VALUES ($1, $2, 'SOLAR_INSTALLMENT', 'ACTIVE', $3, CURRENT_DATE)",
        )
        .bind(contract_id)
        .bind(receivable.try_get::<Uuid, _>("customer_id")?)
        .bind(&input.approved_terms_revision)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE sales SET contract_id = $1 WHERE id = $2")
            .bind(contract_id)
            .bind(input.sale_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO installment_plans
             (id, receivable_id, contract_id, principal_amount, currency, start_date, status, terms_snapshot)
             VALUES ($1, $2, $3, $4, $5, CURRENT_DATE, 'ACTIVE', $6)",
        )
        .bind(plan_id)
        .bind(input.receivable_id)
        .bind(contract_id)
        .bind(principal)
        .bind(&currency)
        .bind(json!({"terms_revision": &input.approved_terms_revision}))
        .execute(&mut *tx)
        .await?;
        for installment in &input.schedule {
            sqlx::query(
                "INSERT INTO installment_schedule
                 (id, plan_id, sequence, due_date, amount_due, schedule_status)
                 VALUES ($1, $2, $3, $4, $5, 'OPEN')",
            )
            .bind(solar_schedule_id(plan_id, installment.sequence))
            .bind(plan_id)
            .bind(installment.sequence as i32)
            .bind(installment.due_date)
            .bind(installment.amount.round_dp(2))
            .execute(&mut *tx)
            .await?;
        }
        let response = json!({
            "status": "CENTRAL_ACCEPTED",
            "sale_id": input.sale_id,
            "receivable_id": input.receivable_id,
            "installment_plan_id": plan_id,
            "contract_id": contract_id,
            "principal": principal.round_dp(2),
            "currency": currency,
            "terms_revision": &input.approved_terms_revision,
            "schedule": input.schedule
        });
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key, request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 200, $6)",
        )
        .bind(self.organization_id)
        .bind(self.organization_id)
        .bind("solar.contract.activate")
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(SolarContractWriteOutcome::Created(response))
    }

    pub async fn apply_sync_event(
        &self,
        event: &SyncEnvelope,
    ) -> Result<SyncEventResult, SyncApplyError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT payload_sha256, result_json FROM sync_inbox WHERE device_id = $1 AND event_id = $2",
        )
        .bind(event.device_id)
        .bind(event.event_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let digest: String = row.try_get("payload_sha256")?;
            let stored: serde_json::Value = row.try_get("result_json")?;
            let original: SyncEventResult = serde_json::from_value(stored)?;
            if digest != event.payload_sha256 {
                let result = SyncEventResult::Conflict {
                    reason: "EVENT_PAYLOAD_MISMATCH".into(),
                    central_revision: None,
                };
                sqlx::query(
                    "INSERT INTO sync_conflicts
                     (id, device_id, event_id, reason, central_revision, payload_json)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(Uuid::new_v4())
                .bind(event.device_id)
                .bind(event.event_id)
                .bind("EVENT_PAYLOAD_MISMATCH")
                .bind(Option::<i64>::None)
                .bind(event.payload.clone())
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                return Ok(result);
            }
            return Ok(SyncEventResult::Duplicate {
                original: Box::new(original),
            });
        }
        if !self.device_is_authorized(&mut tx, event).await? {
            let result = SyncEventResult::Rejected {
                code: "DEVICE_OR_GRANT_NOT_AUTHORIZED".into(),
                required_action: "enroll_or_refresh_device_grant".into(),
            };
            persist_sync_result(&mut tx, self.organization_id, event, &result, None).await?;
            tx.commit().await?;
            return Ok(result);
        }
        let event_payload_digest = payload_digest(&event.payload);
        if event_payload_digest != event.payload_sha256 {
            let result = SyncEventResult::Rejected {
                code: "PAYLOAD_HASH_MISMATCH".into(),
                required_action: "rebuild_event_with_canonical_payload".into(),
            };
            persist_sync_result(&mut tx, self.organization_id, event, &result, None).await?;
            tx.commit().await?;
            return Ok(result);
        }
        match event.command_type.as_str() {
            "CUSTOMER_CREATED" => {
                let customer_code = event
                    .payload
                    .get("customer_code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let legal_name = event
                    .payload
                    .get("legal_name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if customer_code.is_empty() || legal_name.is_empty() {
                    let result = SyncEventResult::Rejected {
                        code: "CUSTOMER_REQUIRED".into(),
                        required_action: "provide_customer_code_and_legal_name".into(),
                    };
                    persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                        .await?;
                    tx.commit().await?;
                    return Ok(result);
                }
                let duplicate = sqlx::query(
                    "SELECT id FROM customers WHERE organization_id = $1 AND (id = $2 OR customer_code = $3)",
                )
                .bind(self.organization_id)
                .bind(event.aggregate_id)
                .bind(customer_code)
                .fetch_optional(&mut *tx)
                .await?;
                if duplicate.is_some() {
                    let result = SyncEventResult::Conflict {
                        reason: "CUSTOMER_ALREADY_EXISTS".into(),
                        central_revision: None,
                    };
                    persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                        .await?;
                    tx.commit().await?;
                    return Ok(result);
                }
                let contact_fields = json!({
                    "address": event.payload.get("address").cloned().unwrap_or(serde_json::Value::Null),
                    "phone": event.payload.get("phone").cloned().unwrap_or(serde_json::Value::Null)
                });
                sqlx::query(
                    "INSERT INTO customers
                     (id, organization_id, customer_code, legal_name, contact_fields, status, revision)
                     VALUES ($1, $2, $3, $4, $5, 'ACTIVE', 1)",
                )
                .bind(event.aggregate_id)
                .bind(self.organization_id)
                .bind(customer_code)
                .bind(legal_name)
                .bind(contact_fields)
                .execute(&mut *tx)
                .await?;
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: event.aggregate_id,
                    },
                    Some(("customers", event.aggregate_id, 1, event.payload.clone())),
                )
                .await?;
                tx.commit().await?;
                Ok(result)
            }
            "CUSTOMER_UPDATED" => {
                let current = sqlx::query(
                    "SELECT revision FROM customers
                     WHERE id = $1 AND organization_id = $2 FOR UPDATE",
                )
                .bind(event.aggregate_id)
                .bind(self.organization_id)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(current) = current else {
                    let result = SyncEventResult::Rejected {
                        code: "CUSTOMER_NOT_FOUND".into(),
                        required_action: "refresh_customer_before_retry".into(),
                    };
                    persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                        .await?;
                    tx.commit().await?;
                    return Ok(result);
                };
                let current_revision: i64 = current.try_get("revision")?;
                if current_revision != event.aggregate_expected_version {
                    let result = SyncEventResult::Conflict {
                        reason: "CUSTOMER_REVISION_CONFLICT".into(),
                        central_revision: Some(current_revision),
                    };
                    persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                        .await?;
                    tx.commit().await?;
                    return Ok(result);
                }
                let legal_name = event
                    .payload
                    .get("legal_name")
                    .and_then(serde_json::Value::as_str);
                let address = event
                    .payload
                    .get("address")
                    .and_then(serde_json::Value::as_str);
                let phone = event
                    .payload
                    .get("phone")
                    .and_then(serde_json::Value::as_str);
                let status = event
                    .payload
                    .get("status")
                    .and_then(serde_json::Value::as_str);
                let contact_fields = json!({"address": address, "phone": phone});
                sqlx::query(
                    "UPDATE customers SET legal_name = COALESCE($1, legal_name),
                         contact_fields = $2, status = COALESCE($3, status), revision = revision + 1
                     WHERE id = $4 AND organization_id = $5",
                )
                .bind(legal_name)
                .bind(contact_fields)
                .bind(status)
                .bind(event.aggregate_id)
                .bind(self.organization_id)
                .execute(&mut *tx)
                .await?;
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: event.aggregate_id,
                    },
                    Some((
                        "customers",
                        event.aggregate_id,
                        current_revision + 1,
                        event.payload.clone(),
                    )),
                )
                .await?;
                tx.commit().await?;
                Ok(result)
            }
            "UTILITY_INVOICE_ISSUED" => {
                let payload: UtilityInvoiceSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                            code: "UTILITY_PAYLOAD_LEGACY_IDS_REQUIRED".into(),
                            required_action:
                                "recreate_the_local_invoice_with_stable_invoice_and_receivable_ids"
                                    .into(),
                        };
                            persist_sync_result(
                                &mut tx,
                                self.organization_id,
                                event,
                                &result,
                                None,
                            )
                            .await?;
                            tx.commit().await?;
                            return Ok(result);
                        }
                    };
                let UtilityInvoiceSyncPayload {
                    request: input,
                    invoice_id,
                    receivable_id,
                } = payload;
                tx.rollback().await?;
                if input.electricity.unit != "KWH" || input.water.unit != "M3" {
                    return self
                        .persist_sync_rejection(
                            event,
                            "UTILITY_UNIT_INVALID",
                            "send_electricity_in_kwh_and_water_in_m3",
                        )
                        .await;
                }
                if input.water.quantity.is_zero() && !input.water_zero_confirmed {
                    return self
                        .persist_sync_rejection(
                            event,
                            "WATER_ZERO_CONFIRMATION_REQUIRED",
                            "confirm_zero_water_consumption",
                        )
                        .await;
                }
                let tariffs = self
                    .load_tariffs(
                        &input.tariff_revision,
                        &input.currency,
                        &input.billing_period,
                    )
                    .await?;
                let water = if input.water.quantity.is_zero() {
                    billing::WaterInput::ZeroConfirmed
                } else {
                    billing::WaterInput::Positive(input.water.quantity)
                };
                let Some(electricity_unit_price) = tariffs.electricity_unit_price else {
                    return self
                        .persist_sync_rejection(
                            event,
                            "ELECTRICITY_TARIFF_UNAVAILABLE",
                            "publish_an_approved_electricity_tariff_for_the_period",
                        )
                        .await;
                };
                let Some(electricity_tax_rate) = tariffs.electricity_tax_rate else {
                    return self
                        .persist_sync_rejection(
                            event,
                            "ELECTRICITY_TAX_UNAVAILABLE",
                            "publish_an_approved_electricity_tax_for_the_period",
                        )
                        .await;
                };
                let calculated = match billing::calculate_utility(billing::UtilityInput {
                    electricity_kwh: input.electricity.quantity,
                    water_m3: water,
                    electricity_unit_price,
                    electricity_tax_rate,
                    water_tariff: tariffs.water_unit_price,
                    water_tax_rate: tariffs.water_tax_rate,
                    currency: input.currency.clone(),
                }) {
                    Ok(calculated) => calculated,
                    Err(error) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "UTILITY_RULE_INVALID",
                                &error.to_string(),
                            )
                            .await;
                    }
                };
                let lines = calculated
                    .lines
                    .into_iter()
                    .map(|line| sel_contracts::InvoiceLineResponse {
                        service_code: format!("{:?}", line.service).to_uppercase(),
                        quantity: line.quantity,
                        unit: line.unit.into(),
                        unit_price: line.unit_price,
                        subtotal: line.subtotal,
                        tax: line.tax,
                        total: line.total,
                    })
                    .collect::<Vec<_>>();
                let response = UtilityBillingResponse {
                    invoice_id,
                    receivable_id: Some(receivable_id),
                    invoice_number: None,
                    status: "CENTRAL_ACCEPTED".into(),
                    currency: calculated.currency,
                    lines,
                    subtotal: calculated.subtotal,
                    tax: calculated.tax,
                    total: calculated.total,
                    snapshot_sha256: String::new(),
                };
                response.snapshot_sha256 = match utility_snapshot_sha256(&response) {
                    Ok(hash) => hash,
                    Err(error) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "UTILITY_SNAPSHOT_INVALID",
                                &error.to_string(),
                            )
                            .await;
                    }
                };
                let operation_key = format!("sync-utility-{}", event.event_id);
                let outcome = self
                    .issue_utility_invoice(&input, &response, &operation_key, &event.payload_sha256)
                    .await;
                let accepted_response = match outcome {
                    Ok(UtilityIssueOutcome::Created(response)) => response,
                    Ok(UtilityIssueOutcome::Replay(response)) => response,
                    Err(_) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "UTILITY_INVOICE_NOT_ACCEPTED",
                                "review_tariffs_and_invoice_period",
                            )
                            .await;
                    }
                };
                let mut ack_tx = self.pool.begin().await?;
                let result = persist_sync_result(
                    &mut ack_tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: accepted_response.invoice_id,
                    },
                    Some((
                        "invoices",
                        accepted_response.invoice_id,
                        1,
                        json!({
                            "request": input,
                            "response": accepted_response
                        }),
                    )),
                )
                .await?;
                ack_tx.commit().await?;
                Ok(result)
            }
            "SOLAR_SALE_CREATED" => {
                let payload: SolarSaleSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "SOLAR_SALE_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_solar_sale_request".into(),
                            };
                            persist_sync_result(
                                &mut tx,
                                self.organization_id,
                                event,
                                &result,
                                None,
                            )
                            .await?;
                            tx.commit().await?;
                            return Ok(result);
                        }
                    };
                let SolarSaleSyncPayload {
                    request: input,
                    sale_id,
                    receivable_id,
                } = payload;
                tx.rollback().await?;
                let operation_key = format!("sync-solar-sale-{}", event.event_id);
                let outcome = self
                    .create_solar_sale_with_ids(
                        &input,
                        sale_id,
                        receivable_id,
                        &operation_key,
                        &event.payload_sha256,
                    )
                    .await;
                let response = match outcome {
                    Ok(SolarSaleWriteOutcome::Created(response)) => response,
                    Ok(SolarSaleWriteOutcome::Replay(response)) => response,
                    Err(_) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "SOLAR_SALE_NOT_ACCEPTED",
                                "review_solar_product_and_customer",
                            )
                            .await;
                    }
                };
                let sale_id = response.sale_id;
                let mut ack_tx = self.pool.begin().await?;
                let result = persist_sync_result(
                    &mut ack_tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: sale_id,
                    },
                    Some((
                        "sales",
                        sale_id,
                        1,
                        serde_json::to_value(&response).map_err(|error| {
                            sqlx::Error::Protocol(format!("invalid solar sale response: {error}"))
                        })?,
                    )),
                )
                .await?;
                ack_tx.commit().await?;
                Ok(result)
            }
            "SOLAR_CONTRACT_ACTIVATED" => {
                let payload: SolarContractSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "SOLAR_CONTRACT_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_solar_contract_request".into(),
                            };
                            persist_sync_result(
                                &mut tx,
                                self.organization_id,
                                event,
                                &result,
                                None,
                            )
                            .await?;
                            tx.commit().await?;
                            return Ok(result);
                        }
                    };
                let SolarContractSyncPayload {
                    request: input,
                    contract_id,
                    installment_plan_id,
                } = payload;
                tx.rollback().await?;
                let operation_key = format!("sync-solar-contract-{}", event.event_id);
                let outcome = self
                    .activate_solar_contract_with_plan_id(
                        contract_id,
                        installment_plan_id,
                        &input,
                        &operation_key,
                        &event.payload_sha256,
                    )
                    .await;
                let body = match outcome {
                    Ok(SolarContractWriteOutcome::Created(body)) => body,
                    Ok(SolarContractWriteOutcome::Replay(body)) => body,
                    Err(_) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "SOLAR_CONTRACT_NOT_ACCEPTED",
                                "review_solar_schedule_and_receivable",
                            )
                            .await;
                    }
                };
                let entity_id = body
                    .get("installment_plan_id")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(event.aggregate_id);
                let mut ack_tx = self.pool.begin().await?;
                let result = persist_sync_result(
                    &mut ack_tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: entity_id,
                    },
                    Some(("installment_plans", entity_id, 1, body)),
                )
                .await?;
                ack_tx.commit().await?;
                Ok(result)
            }
            "PAYMENT_RECORDED" => {
                let (input, payment_id) =
                    match serde_json::from_value::<PaymentSyncPayload>(event.payload.clone()) {
                        Ok(payload) => (payload.request, payload.payment_id),
                        Err(_) => match serde_json::from_value::<RecordPaymentRequest>(
                            event.payload.clone(),
                        ) {
                            Ok(input) => (input, event.aggregate_id),
                            Err(_) => {
                                let result = SyncEventResult::Rejected {
                                    code: "PAYMENT_PAYLOAD_INVALID".into(),
                                    required_action: "send_a_valid_payment_request".into(),
                                };
                                persist_sync_result(
                                    &mut tx,
                                    self.organization_id,
                                    event,
                                    &result,
                                    None,
                                )
                                .await?;
                                tx.commit().await?;
                                return Ok(result);
                            }
                        },
                    };
                tx.rollback().await?;
                let operation_key = format!("sync-payment-{}", event.event_id);
                let outcome = self
                    .record_payment_with_id(
                        &input,
                        payment_id,
                        &operation_key,
                        &event.payload_sha256,
                    )
                    .await;
                let (body, payment_id) = match outcome {
                    Ok(PaymentWriteOutcome::Created(body)) => {
                        let payment_id = body
                            .get("payment_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(event.aggregate_id);
                        (body, payment_id)
                    }
                    Ok(PaymentWriteOutcome::Replay(body)) => {
                        let payment_id = body
                            .get("payment_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(event.aggregate_id);
                        (body, payment_id)
                    }
                    Err(_) => {
                        let mut reject_tx = self.pool.begin().await?;
                        let result = SyncEventResult::Rejected {
                            code: "PAYMENT_NOT_ACCEPTED".into(),
                            required_action: "review_payment_and_retry".into(),
                        };
                        persist_sync_result(
                            &mut reject_tx,
                            self.organization_id,
                            event,
                            &result,
                            None,
                        )
                        .await?;
                        reject_tx.commit().await?;
                        return Ok(result);
                    }
                };
                let mut ack_tx = self.pool.begin().await?;
                sqlx::query(
                    "UPDATE payments SET origin_event_id = $1 WHERE id = $2 AND origin_event_id IS NULL",
                )
                .bind(event.event_id)
                .bind(payment_id)
                .execute(&mut *ack_tx)
                .await?;
                let result = persist_sync_result(
                    &mut ack_tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: payment_id,
                    },
                    Some((
                        "payments",
                        payment_id,
                        1,
                        json!({
                            "customer_id": input.customer_id,
                            "payment": body,
                            "allocations": input.allocations,
                        }),
                    )),
                )
                .await?;
                ack_tx.commit().await?;
                Ok(result)
            }
            _ => {
                let result = SyncEventResult::Rejected {
                    code: "COMMAND_NOT_IMPLEMENTED".into(),
                    required_action: "use_a_supported_v2_command".into(),
                };
                persist_sync_result(&mut tx, self.organization_id, event, &result, None).await?;
                tx.commit().await?;
                Ok(result)
            }
        }
    }

    pub async fn pull_changes(
        &self,
        cursor: i64,
        limit: i64,
    ) -> Result<(Vec<serde_json::Value>, i64), sqlx::Error> {
        let rows = sqlx::query(
            "SELECT sequence, entity, entity_id, revision, change_json
             FROM central_changes
             WHERE organization_id = $1 AND sequence > $2
             ORDER BY sequence
             LIMIT $3",
        )
        .bind(self.organization_id)
        .bind(cursor)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut next_cursor = cursor;
        let changes = rows
            .iter()
            .map(|row| {
                let sequence: i64 = row.try_get("sequence")?;
                next_cursor = sequence;
                Ok(json!({
                    "sequence": sequence,
                    "entity": row.try_get::<String, _>("entity")?,
                    "entity_id": row.try_get::<Uuid, _>("entity_id")?,
                    "revision": row.try_get::<i64, _>("revision")?,
                    "change": row.try_get::<serde_json::Value, _>("change_json")?
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok((changes, next_cursor))
    }

    pub async fn sync_status(&self) -> Result<serde_json::Value, sqlx::Error> {
        let conflicts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sync_conflicts c
             JOIN devices d ON d.id = c.device_id
             WHERE d.organization_id = $1",
        )
        .bind(self.organization_id)
        .fetch_one(&self.pool)
        .await?;
        let rejected: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sync_inbox i
             JOIN devices d ON d.id = i.device_id
             WHERE d.organization_id = $1 AND i.result_json->>'status' = 'REJECTED'",
        )
        .bind(self.organization_id)
        .fetch_one(&self.pool)
        .await?;
        let last_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT max(sequence) FROM central_changes WHERE organization_id = $1",
        )
        .bind(self.organization_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(json!({
            "protocol_version": 1,
            "state": "READY",
            "pending": 0,
            "conflicts": conflicts,
            "rejected": rejected,
            "last_successful_sync": last_sequence
        }))
    }

    async fn device_is_authorized(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &SyncEnvelope,
    ) -> Result<bool, sqlx::Error> {
        let device = sqlx::query(
            "SELECT id FROM devices WHERE id = $1 AND organization_id = $2 AND state = 'ACTIVE'",
        )
        .bind(event.device_id)
        .bind(self.organization_id)
        .fetch_optional(&mut **tx)
        .await?
        .is_some();
        if !device {
            return Ok(false);
        }
        let actor = sqlx::query(
            "SELECT id FROM users WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'",
        )
        .bind(event.actor_id)
        .bind(self.organization_id)
        .fetch_optional(&mut **tx)
        .await?
        .is_some();
        if !actor {
            return Ok(false);
        }
        Ok(sqlx::query(
            "SELECT id FROM offline_grants
             WHERE id = $1 AND user_id = $2 AND device_id = $3 AND expires_at > now()",
        )
        .bind(event.offline_grant_id)
        .bind(event.actor_id)
        .bind(event.device_id)
        .fetch_optional(&mut **tx)
        .await?
        .is_some())
    }

    async fn persist_sync_rejection(
        &self,
        event: &SyncEnvelope,
        code: &str,
        required_action: &str,
    ) -> Result<SyncEventResult, SyncApplyError> {
        let mut tx = self.pool.begin().await?;
        let result = SyncEventResult::Rejected {
            code: code.into(),
            required_action: required_action.into(),
        };
        persist_sync_result(&mut tx, self.organization_id, event, &result, None).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn persist_sync_result(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    organization_id: Uuid,
    event: &SyncEnvelope,
    result: &SyncEventResult,
    change: Option<(&str, Uuid, i64, serde_json::Value)>,
) -> Result<SyncEventResult, sqlx::Error> {
    let mut accepted_sequence = None;
    if let Some((entity, entity_id, revision, change_json)) = change {
        let row = sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, $2, $3, $4, $5) RETURNING sequence",
        )
        .bind(organization_id)
        .bind(entity)
        .bind(entity_id)
        .bind(revision)
        .bind(change_json)
        .fetch_one(&mut **tx)
        .await?;
        accepted_sequence = Some(row.try_get::<i64, _>("sequence")?);
    }
    let result = match result {
        SyncEventResult::Accepted {
            server_entity_id, ..
        } => SyncEventResult::Accepted {
            server_sequence: accepted_sequence.unwrap_or_default(),
            server_entity_id: *server_entity_id,
        },
        other => other.clone(),
    };
    let result_json = serde_json::to_value(&result)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid sync result: {error}")))?;
    sqlx::query(
        "INSERT INTO sync_inbox
         (device_id, event_id, payload_sha256, result_json, accepted_sequence)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(event.device_id)
    .bind(event.event_id)
    .bind(&event.payload_sha256)
    .bind(result_json)
    .bind(accepted_sequence)
    .execute(&mut **tx)
    .await?;
    Ok(result)
}

fn payload_digest(payload: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(payload).expect("JSON serialization cannot fail"));
    hex::encode(digest.finalize())
}

fn customer_from_row(row: &sqlx::postgres::PgRow) -> Result<Customer, sqlx::Error> {
    let contact_fields: serde_json::Value = row.try_get("contact_fields")?;
    Ok(Customer {
        id: row.try_get("id")?,
        organization_id: row.try_get("organization_id")?,
        customer_code: row.try_get("customer_code")?,
        legal_name: row.try_get("legal_name")?,
        address: contact_fields
            .get("address")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        phone: contact_fields
            .get("phone")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
    })
}
