use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rust_decimal::Decimal;
use sel_contracts::{
    offline_grant_canonical_bytes, solar_schedule_id, utility_snapshot_sha256,
    ActivateSolarContractRequest, CreateCustomer, CreateSolarSaleRequest, Customer,
    InvoiceVoidSyncPayload, PaymentReversalSyncPayload, PaymentSyncPayload, RecordPaymentRequest,
    ReversePaymentRequest, SolarContractSyncPayload, SolarSaleResponse, SolarSaleSyncPayload,
    SyncEnvelope, SyncEventResult, UpdateCustomer, UtilityBillingRequest, UtilityBillingResponse,
    UtilityInvoiceSyncPayload,
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
    require_signed_offline_grants: bool,
    offline_grant_public_key: Option<VerifyingKey>,
}

#[derive(Clone, Debug)]
pub struct AuthSessionPrincipal {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub role: String,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("credentials are invalid")]
    InvalidCredentials,
    #[error("session is invalid or expired")]
    InvalidSession,
    #[error("password hashing failed: {0}")]
    PasswordHash(String),
    #[error("stored session response is invalid: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("user was not found or is not active")]
    UserNotFound,
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
pub enum PaymentReverseError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("payment was not found in the organization")]
    PaymentNotFound,
    #[error("payment is not accepted centrally")]
    PaymentNotAccepted,
    #[error("payment reversal actor is not active in the organization")]
    ActorNotActive,
    #[error("payment reversal actor is not authorized")]
    ActorNotAuthorized,
    #[error("reversal amount is invalid")]
    InvalidAmount,
    #[error("reversal exceeds the payment amount still reversible")]
    AmountExceedsRemaining,
    #[error("reversal reason is required")]
    ReasonRequired,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

pub enum PaymentReverseOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

#[derive(Debug, Error)]
pub enum InvoiceVoidError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("invoice was not found in the organization")]
    InvoiceNotFound,
    #[error("invoice is already void")]
    AlreadyVoided,
    #[error("invoice has accepted payments and cannot be voided directly")]
    HasPayments,
    #[error("invoice void actor is not active")]
    ActorNotActive,
    #[error("invoice void actor is not authorized")]
    ActorNotAuthorized,
    #[error("void reason is required")]
    ReasonRequired,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

pub enum InvoiceVoidOutcome {
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
        let (require_signed_offline_grants, offline_grant_public_key) = Self::security_config();
        Ok(Self {
            pool,
            organization_id,
            require_signed_offline_grants,
            offline_grant_public_key,
        })
    }

    pub fn from_pool(pool: PgPool, organization_id: Uuid) -> Self {
        let (require_signed_offline_grants, offline_grant_public_key) = Self::security_config();
        Self {
            pool,
            organization_id,
            require_signed_offline_grants,
            offline_grant_public_key,
        }
    }

    fn security_config() -> (bool, Option<VerifyingKey>) {
        let require_signed_offline_grants = std::env::var("SEL_REQUIRE_SIGNED_OFFLINE_GRANTS")
            .ok()
            .as_deref()
            == Some("1");
        let offline_grant_public_key = std::env::var("SEL_OFFLINE_GRANT_PUBLIC_KEY")
            .ok()
            .and_then(|encoded| BASE64.decode(encoded).ok())
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
            .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok());
        (require_signed_offline_grants, offline_grant_public_key)
    }

    pub fn with_offline_grant_verifier(mut self, public_key: VerifyingKey) -> Self {
        self.require_signed_offline_grants = true;
        self.offline_grant_public_key = Some(public_key);
        self
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

    /// Hash a password using Argon2id before it is provisioned into a user row.
    /// The API never accepts or returns a stored password hash.
    pub fn hash_user_password(password: &str) -> Result<String, AuthError> {
        if password.chars().count() < 12 {
            return Err(AuthError::PasswordHash(
                "password must contain at least 12 characters".into(),
            ));
        }
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|error| AuthError::PasswordHash(error.to_string()))
    }

    /// Provision or rotate a user's password through an explicit administrative
    /// operation. This is intentionally a storage primitive; a future admin API
    /// must authorize the caller before invoking it.
    pub async fn set_user_password(&self, user_id: Uuid, password: &str) -> Result<(), AuthError> {
        let password_hash = Self::hash_user_password(password)?;
        let updated = sqlx::query(
            "UPDATE users SET password_hash = $1
             WHERE id = $2 AND organization_id = $3 AND status = 'ACTIVE'",
        )
        .bind(password_hash)
        .bind(user_id)
        .bind(self.organization_id)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AuthError::UserNotFound);
        }
        Ok(())
    }

    pub async fn login_user(
        &self,
        identity_ref: &str,
        password: &str,
    ) -> Result<serde_json::Value, AuthError> {
        let row = sqlx::query(
            "SELECT id, role, password_hash
             FROM users
             WHERE organization_id = $1 AND identity_ref = $2 AND status = 'ACTIVE'",
        )
        .bind(self.organization_id)
        .bind(identity_ref.trim())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
        let user_id: Uuid = row.try_get("id")?;
        let role: String = row.try_get("role")?;
        let stored_hash: Option<String> = row.try_get("password_hash")?;
        let Some(stored_hash) = stored_hash else {
            return Err(AuthError::InvalidCredentials);
        };
        let parsed = PasswordHash::new(&stored_hash)
            .map_err(|error| AuthError::PasswordHash(error.to_string()))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AuthError::InvalidCredentials)?;

        let session_id = Uuid::new_v4();
        let access_token = Uuid::new_v4().to_string();
        let refresh_token = Uuid::new_v4().to_string();
        let access_expires_at = Utc::now() + Duration::minutes(15);
        let refresh_expires_at = Utc::now() + Duration::days(30);
        sqlx::query(
            "INSERT INTO auth_sessions
             (id, organization_id, user_id, access_token_hash, refresh_token_hash,
              access_expires_at, refresh_expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(session_id)
        .bind(self.organization_id)
        .bind(user_id)
        .bind(token_digest(&access_token))
        .bind(token_digest(&refresh_token))
        .bind(access_expires_at)
        .bind(refresh_expires_at)
        .execute(&self.pool)
        .await?;
        Ok(json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "token_type": "Bearer",
            "user_id": user_id,
            "organization_id": self.organization_id,
            "role": role,
            "access_expires_at": access_expires_at,
            "refresh_expires_at": refresh_expires_at
        }))
    }

    pub async fn refresh_user_session(
        &self,
        refresh_token: &str,
    ) -> Result<serde_json::Value, AuthError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT s.id, s.user_id, u.role, s.refresh_expires_at
             FROM auth_sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.organization_id = $1 AND s.refresh_token_hash = $2
               AND s.revoked_at IS NULL AND u.status = 'ACTIVE'
             FOR UPDATE OF s",
        )
        .bind(self.organization_id)
        .bind(token_digest(refresh_token))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::InvalidSession)?;
        let refresh_expires_at: DateTime<Utc> = row.try_get("refresh_expires_at")?;
        if refresh_expires_at <= Utc::now() {
            return Err(AuthError::InvalidSession);
        }
        let session_id: Uuid = row.try_get("id")?;
        let user_id: Uuid = row.try_get("user_id")?;
        let role: String = row.try_get("role")?;
        let access_token = Uuid::new_v4().to_string();
        let next_refresh_token = Uuid::new_v4().to_string();
        let access_expires_at = Utc::now() + Duration::minutes(15);
        sqlx::query(
            "UPDATE auth_sessions
             SET access_token_hash = $1, refresh_token_hash = $2,
                 access_expires_at = $3
             WHERE id = $4",
        )
        .bind(token_digest(&access_token))
        .bind(token_digest(&next_refresh_token))
        .bind(access_expires_at)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(json!({
            "access_token": access_token,
            "refresh_token": next_refresh_token,
            "token_type": "Bearer",
            "user_id": user_id,
            "organization_id": self.organization_id,
            "role": role,
            "access_expires_at": access_expires_at,
            "refresh_expires_at": refresh_expires_at
        }))
    }

    pub async fn revoke_user_session(&self, access_token: &str) -> Result<(), AuthError> {
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = now()
             WHERE organization_id = $1 AND access_token_hash = $2 AND revoked_at IS NULL",
        )
        .bind(self.organization_id)
        .bind(token_digest(access_token))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn authenticate_access_token(
        &self,
        access_token: &str,
    ) -> Result<AuthSessionPrincipal, AuthError> {
        let row = sqlx::query(
            "SELECT s.user_id, s.organization_id, u.role
             FROM auth_sessions s
             JOIN users u ON u.id = s.user_id
             WHERE s.organization_id = $1 AND s.access_token_hash = $2
               AND s.revoked_at IS NULL AND s.access_expires_at > now()
               AND u.status = 'ACTIVE'",
        )
        .bind(self.organization_id)
        .bind(token_digest(access_token))
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AuthError::InvalidSession)?;
        Ok(AuthSessionPrincipal {
            user_id: row.try_get("user_id")?,
            organization_id: row.try_get("organization_id")?,
            role: row.try_get("role")?,
        })
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
            let document = snapshot.get("v2_document").cloned().unwrap_or(snapshot);
            serde_json::from_value(document).map_err(|error| {
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
                    COALESCE((
                        SELECT SUM(
                            pa.allocated_amount * GREATEST(
                                p.amount_received - COALESCE(rev.reversed_amount, 0),
                                0
                            ) / NULLIF(p.amount_received, 0)
                        )
                        FROM payment_allocations pa
                        JOIN payments p ON p.id = pa.payment_id
                        LEFT JOIN (
                            SELECT payment_id, SUM(amount) AS reversed_amount
                            FROM payment_reversals
                            GROUP BY payment_id
                        ) rev ON rev.payment_id = p.id
                        WHERE pa.receivable_id = r.id
                          AND p.status = 'CENTRAL_ACCEPTED'
                    ), 0) AS paid
             FROM receivables r
             WHERE r.id = $1 AND r.customer_id IN
                 (SELECT id FROM customers WHERE organization_id = $2)
             GROUP BY r.id, r.customer_id, r.currency, r.principal_total",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let receivable_id: Uuid = row.try_get("id")?;
        let principal: Decimal = row.try_get("principal_total")?;
        let paid: Decimal = row.try_get("paid")?;
        let principal = principal.round_dp(2);
        let mut paid = paid.round_dp(2);
        paid.rescale(2);
        let balance = (principal - paid).round_dp(2);
        let overdue: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(GREATEST(s.amount_due - COALESCE((
                 SELECT SUM(pa.allocated_amount * GREATEST(
                     p.amount_received - COALESCE(rev.reversed_amount, 0), 0
                 ) / NULLIF(p.amount_received, 0))
                 FROM payment_allocations pa
                 JOIN payments p ON p.id = pa.payment_id
                 LEFT JOIN (
                     SELECT payment_id, SUM(amount) AS reversed_amount
                     FROM payment_reversals GROUP BY payment_id
                 ) rev ON rev.payment_id = p.id
                 WHERE pa.schedule_id = s.id AND p.status = 'CENTRAL_ACCEPTED'
             ), 0), 0), 0)
             FROM installment_schedule s
             JOIN installment_plans ip ON ip.id = s.plan_id
             WHERE ip.receivable_id = $1 AND s.due_date < CURRENT_DATE",
        )
        .bind(receivable_id)
        .fetch_one(&self.pool)
        .await?;
        let overdue = overdue.round_dp(2);
        let status = if paid >= principal {
            "SETTLED"
        } else if overdue > Decimal::ZERO {
            "OVERDUE"
        } else {
            "OPEN"
        };
        Ok(Some(json!({
            "id": receivable_id,
            "customer_id": row.try_get::<Uuid, _>("customer_id")?,
            "currency": row.try_get::<String, _>("currency")?,
            "principal": format!("{principal:.2}"),
            "paid": format!("{paid:.2}"),
            "balance": format!("{balance:.2}"),
            "overdue_amount": format!("{overdue:.2}"),
            "overdue": overdue > Decimal::ZERO,
            "status": status
        })))
    }

    pub async fn void_invoice(
        &self,
        invoice_id: Uuid,
        actor_id: Uuid,
        reason: &str,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<InvoiceVoidOutcome, InvoiceVoidError> {
        self.void_invoice_with_id(
            invoice_id,
            Uuid::new_v4(),
            actor_id,
            reason,
            idempotency_key,
            request_digest,
        )
        .await
    }

    pub async fn void_invoice_with_id(
        &self,
        invoice_id: Uuid,
        void_id: Uuid,
        actor_id: Uuid,
        reason: &str,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<InvoiceVoidOutcome, InvoiceVoidError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'invoice.void' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(InvoiceVoidError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(InvoiceVoidOutcome::Replay(response));
        }
        if reason.trim().is_empty() {
            return Err(InvoiceVoidError::ReasonRequired);
        }
        let role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM users
             WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'",
        )
        .bind(actor_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(role) = role else {
            return Err(InvoiceVoidError::ActorNotActive);
        };
        if !matches!(role.as_str(), "ADMIN" | "SUPERVISOR") {
            return Err(InvoiceVoidError::ActorNotAuthorized);
        }
        let Some(invoice) = sqlx::query(
            "SELECT i.status, r.id AS receivable_id
             FROM invoices i
             JOIN customers c ON c.id = i.customer_id
             LEFT JOIN receivables r ON r.source_type = 'INVOICE' AND r.source_id = i.id
             WHERE i.id = $1 AND c.organization_id = $2
             FOR UPDATE OF i",
        )
        .bind(invoice_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(InvoiceVoidError::InvoiceNotFound);
        };
        let status: String = invoice.try_get("status")?;
        if status == "VOID" {
            return Err(InvoiceVoidError::AlreadyVoided);
        }
        let receivable_id: Option<Uuid> = invoice.try_get("receivable_id")?;
        if let Some(receivable_id) = receivable_id {
            let has_paid: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                    SELECT 1
                    FROM payment_allocations pa
                    JOIN payments p ON p.id = pa.payment_id
                    LEFT JOIN (
                        SELECT payment_id, SUM(amount) AS reversed_amount
                        FROM payment_reversals GROUP BY payment_id
                    ) rev ON rev.payment_id = p.id
                    WHERE pa.receivable_id = $1 AND p.status = 'CENTRAL_ACCEPTED'
                      AND pa.allocated_amount * GREATEST(
                          p.amount_received - COALESCE(rev.reversed_amount, 0), 0
                      ) / NULLIF(p.amount_received, 0) > 0
                )",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            if has_paid {
                return Err(InvoiceVoidError::HasPayments);
            }
        }
        sqlx::query("UPDATE invoices SET status = 'VOID' WHERE id = $1")
            .bind(invoice_id)
            .execute(&mut *tx)
            .await?;
        if let Some(receivable_id) = receivable_id {
            sqlx::query("UPDATE receivables SET status = 'VOID' WHERE id = $1")
                .bind(receivable_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT INTO invoice_voids (id, invoice_id, reason, actor_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(void_id)
        .bind(invoice_id)
        .bind(reason.trim())
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, event_type, subject_id, occurred_at, redacted_metadata)
             VALUES ($1, $2, 'INVOICE_VOIDED', $3, now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(invoice_id)
        .bind(json!({"void_id": void_id, "reason": reason.trim()}))
        .execute(&mut *tx)
        .await?;
        let body = json!({
            "invoice_id": invoice_id,
            "void_id": void_id,
            "status": "VOID",
            "reason": reason.trim(),
            "actor_id": actor_id
        });
        sqlx::query(
            "INSERT INTO central_changes
             (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'invoices', $2, 2, $3)",
        )
        .bind(self.organization_id)
        .bind(invoice_id)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'invoice.void', $3, $4, 200, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(InvoiceVoidOutcome::Created(body))
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
            for ((_allocation_receivable_id, schedule_id), _) in persisted_allocations
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

    pub async fn reverse_payment(
        &self,
        payment_id: Uuid,
        input: &ReversePaymentRequest,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentReverseOutcome, PaymentReverseError> {
        self.reverse_payment_with_id(
            payment_id,
            input,
            actor_id,
            Uuid::new_v4(),
            idempotency_key,
            request_digest,
        )
        .await
    }

    pub async fn reverse_payment_with_id(
        &self,
        payment_id: Uuid,
        input: &ReversePaymentRequest,
        actor_id: Uuid,
        reversal_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentReverseOutcome, PaymentReverseError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2 AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind("payment.reverse")
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(PaymentReverseError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(PaymentReverseOutcome::Replay(serde_json::from_value(
                original,
            )?));
        }
        if input.amount <= Decimal::ZERO {
            return Err(PaymentReverseError::InvalidAmount);
        }
        if input.reason.trim().is_empty() {
            return Err(PaymentReverseError::ReasonRequired);
        }
        let actor_role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM users
             WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'",
        )
        .bind(actor_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(actor_role) = actor_role else {
            return Err(PaymentReverseError::ActorNotActive);
        };
        if !matches!(actor_role.as_str(), "ADMIN" | "CASHIER" | "SUPERVISOR") {
            return Err(PaymentReverseError::ActorNotAuthorized);
        }
        let Some(payment) = sqlx::query(
            "SELECT p.amount_received, p.status
             FROM payments p
             JOIN customers c ON c.id = p.customer_id
             WHERE p.id = $1 AND c.organization_id = $2
             FOR UPDATE OF p",
        )
        .bind(payment_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(PaymentReverseError::PaymentNotFound);
        };
        let payment_status: String = payment.try_get("status")?;
        if payment_status != "CENTRAL_ACCEPTED" {
            return Err(PaymentReverseError::PaymentNotAccepted);
        }
        let amount_received: Decimal = payment.try_get("amount_received")?;
        let reversed: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM payment_reversals WHERE payment_id = $1",
        )
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;
        let remaining = amount_received - reversed;
        if input.amount > remaining {
            return Err(PaymentReverseError::AmountExceedsRemaining);
        }
        sqlx::query(
            "INSERT INTO payment_reversals (id, payment_id, amount, reason, initiated_by)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(reversal_id)
        .bind(payment_id)
        .bind(input.amount)
        .bind(input.reason.trim())
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        let remaining_after = remaining - input.amount;
        let currency: String = sqlx::query_scalar("SELECT currency FROM payments WHERE id = $1")
            .bind(payment_id)
            .fetch_one(&mut *tx)
            .await?;
        let body = json!({
            "reversal_id": reversal_id,
            "payment_id": payment_id,
            "amount": format!("{:.2}", input.amount),
            "currency": currency,
            "status": "CENTRAL_ACCEPTED",
            "reversal_status": "RECORDED",
            "remaining_reversible_amount": format!("{remaining_after:.2}"),
            "reason": input.reason.trim(),
            "actor_id": actor_id
        });
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, event_type, subject_id, occurred_at, redacted_metadata)
             VALUES ($1, $2, 'PAYMENT_REVERSED', $3, now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(payment_id)
        .bind(json!({
            "reversal_id": reversal_id,
            "amount": input.amount,
            "reason": input.reason.trim()
        }))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO central_changes
             (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'payment_reversals', $2, 1, $3)",
        )
        .bind(self.organization_id)
        .bind(reversal_id)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key, request_digest, response_status, response_json)
             VALUES ($1, $2, 'payment.reverse', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PaymentReverseOutcome::Created(body))
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
                    COALESCE((
                        SELECT SUM(
                            pa.allocated_amount * GREATEST(
                                p.amount_received - COALESCE(rev.reversed_amount, 0),
                                0
                            ) / NULLIF(p.amount_received, 0)
                        )
                        FROM payment_allocations pa
                        JOIN payments p ON p.id = pa.payment_id
                        LEFT JOIN (
                            SELECT payment_id, SUM(amount) AS reversed_amount
                            FROM payment_reversals
                            GROUP BY payment_id
                        ) rev ON rev.payment_id = p.id
                        WHERE pa.receivable_id = r.id
                          AND p.status = 'CENTRAL_ACCEPTED'
                    ), 0) AS paid
             FROM receivables r
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
                let mut paid = paid.round_dp(2);
                paid.rescale(2);
                let balance = (principal - paid).round_dp(2);
                Ok(json!({
                    "receivable_id": row.try_get::<Uuid, _>("id")?,
                    "currency": row.try_get::<String, _>("currency")?,
                    "principal": format!("{principal:.2}"),
                    "paid": format!("{paid:.2}"),
                    "balance": format!("{balance:.2}")
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
                let mut response = UtilityBillingResponse {
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
            "INVOICE_VOIDED" => {
                let payload: InvoiceVoidSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "INVOICE_VOID_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_invoice_void_payload".into(),
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
                tx.rollback().await?;
                let outcome = self
                    .void_invoice_with_id(
                        payload.invoice_id,
                        payload.void_id,
                        payload.actor_id,
                        &payload.reason,
                        &format!("sync-invoice-void-{}", event.event_id),
                        &event.payload_sha256,
                    )
                    .await;
                let body = match outcome {
                    Ok(InvoiceVoidOutcome::Created(body))
                    | Ok(InvoiceVoidOutcome::Replay(body)) => body,
                    Err(_) => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "INVOICE_VOID_NOT_ACCEPTED",
                                "review_invoice_status_and_actor_permissions",
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
                        server_entity_id: payload.invoice_id,
                    },
                    Some(("invoices", payload.invoice_id, 2, body)),
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
            "PAYMENT_REVERSED" => {
                let payload: PaymentReversalSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "PAYMENT_REVERSAL_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_payment_reversal_payload".into(),
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
                let PaymentReversalSyncPayload {
                    payment_id,
                    reversal_id,
                    amount,
                    reason,
                    actor_id,
                } = payload;
                tx.rollback().await?;
                let input = ReversePaymentRequest { amount, reason };
                let operation_key = format!("sync-payment-reversal-{}", event.event_id);
                let outcome = self
                    .reverse_payment_with_id(
                        payment_id,
                        &input,
                        actor_id,
                        reversal_id,
                        &operation_key,
                        &event.payload_sha256,
                    )
                    .await;
                let body = match outcome {
                    Ok(PaymentReverseOutcome::Created(body))
                    | Ok(PaymentReverseOutcome::Replay(body)) => body,
                    Err(_) => {
                        let mut reject_tx = self.pool.begin().await?;
                        let result = SyncEventResult::Rejected {
                            code: "PAYMENT_REVERSAL_NOT_ACCEPTED".into(),
                            required_action: "review_payment_reversal_and_retry".into(),
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
                let result = persist_sync_result(
                    &mut ack_tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: reversal_id,
                    },
                    Some(("payment_reversals", reversal_id, 1, body)),
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
        let Some(grant) = sqlx::query(
            "SELECT scopes, limits, issued_at, expires_at, signature
             FROM offline_grants
             WHERE id = $1 AND user_id = $2 AND device_id = $3
               AND issued_at <= now() AND expires_at > now()",
        )
        .bind(event.offline_grant_id)
        .bind(event.actor_id)
        .bind(event.device_id)
        .fetch_optional(&mut **tx)
        .await?
        else {
            return Ok(false);
        };
        let scopes: serde_json::Value = grant.try_get("scopes")?;
        let limits: serde_json::Value = grant.try_get("limits")?;
        let issued_at: DateTime<Utc> = grant.try_get("issued_at")?;
        let expires_at: DateTime<Utc> = grant.try_get("expires_at")?;
        let signature: String = grant.try_get("signature")?;

        if !scope_allows_command(&scopes, &event.command_type) {
            return Ok(false);
        }
        if !limits_allow_event(tx, event, &limits).await? {
            return Ok(false);
        }
        if self.require_signed_offline_grants {
            let Some(verifying_key) = &self.offline_grant_public_key else {
                return Ok(false);
            };
            let Ok(signature_bytes) = BASE64.decode(signature) else {
                return Ok(false);
            };
            let Ok(signature) = Signature::from_slice(&signature_bytes) else {
                return Ok(false);
            };
            let Ok(canonical) = offline_grant_canonical_bytes(
                event.offline_grant_id,
                event.actor_id,
                event.device_id,
                &scopes,
                &limits,
                issued_at,
                expires_at,
            ) else {
                return Ok(false);
            };
            if verifying_key.verify(&canonical, &signature).is_err() {
                return Ok(false);
            }
        }
        Ok(true)
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

fn token_digest(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(token.as_bytes());
    hex::encode(digest.finalize())
}

fn scope_allows_command(scopes: &serde_json::Value, command_type: &str) -> bool {
    let values = scopes
        .as_array()
        .cloned()
        .or_else(|| {
            scopes
                .get("commands")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .or_else(|| {
            scopes
                .get("allowed_commands")
                .and_then(serde_json::Value::as_array)
                .cloned()
        });
    let Some(values) = values else {
        return scopes.as_object().is_none_or(serde_json::Map::is_empty);
    };
    values
        .iter()
        .any(|value| value.as_str() == Some("*") || value.as_str() == Some(command_type))
}

async fn limits_allow_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &SyncEnvelope,
    limits: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let Some(limits) = limits.as_object() else {
        return Ok(true);
    };
    if let Some(max_events) = limits.get("max_events").and_then(serde_json::Value::as_u64) {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sync_inbox
             WHERE device_id = $1 AND result_json->>'status' = 'ACCEPTED'",
        )
        .bind(event.device_id)
        .fetch_one(&mut **tx)
        .await?;
        if count as u64 >= max_events {
            return Ok(false);
        }
    }
    if let Some(max_payment) = limits
        .get("max_payment_amount")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<Decimal>().ok())
    {
        let amount = event
            .payload
            .pointer("/request/received_amount")
            .or_else(|| event.payload.get("received_amount"))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<Decimal>().ok());
        if amount.is_some_and(|amount| amount > max_payment) {
            return Ok(false);
        }
    }
    Ok(true)
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
