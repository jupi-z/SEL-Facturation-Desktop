use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rust_decimal::Decimal;
use sel_contracts::{
    offline_grant_canonical_bytes, solar_schedule_id, utility_snapshot_sha256,
    ActivateSolarContractRequest, CorrectMeterReading, CreateCustomer, CreateMeter,
    CreateMeterReading, CreateServiceAccount, CreateSolarQuoteRequest, CreateSolarSaleRequest,
    CreditNoteRequest, CreditNoteSyncPayload, Customer, InvoiceIdentitySnapshot,
    InvoiceVoidSyncPayload, Meter, MeterReading, MeterReplacement, PaymentAllocationSyncPayload,
    PaymentReversalSyncPayload, PaymentSyncPayload, RecordPaymentRequest, ReplaceMeter,
    ReversePaymentRequest, ServiceAccount, ServiceAccountUpdateSyncPayload,
    SolarContractSyncPayload, SolarQuoteResponse, SolarSaleResponse, SolarSaleSyncPayload,
    SyncEnvelope, SyncEventResult, UpdateCustomer, UpdateServiceAccount, UtilityBillingRequest,
    UtilityBillingResponse, UtilityInvoiceSyncPayload, UtilityMeterReadingRefs,
};
use sel_domain::{
    billing,
    document_reference::{issued_document_reference, issued_invoice_reference},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;
use thiserror::Error;
use uuid::Uuid;

pub const MIN_PASSWORD_LENGTH: usize = 8;

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
    pub password_change_required: bool,
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
    #[error("identity reference is required")]
    IdentityRefRequired,
    #[error("an initial administrator or another user is already provisioned")]
    InitialAdminAlreadyProvisioned,
    #[error("organization must exist before provisioning its initial administrator")]
    OrganizationNotFound,
    #[error("actor is not an active administrator in this organization")]
    ActorNotAuthorized,
    #[error("the new password must differ from the current password")]
    PasswordUnchanged,
}

#[derive(Debug, Error)]
pub enum AdminDirectoryError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("actor is not an active administrator in this organization")]
    ActorNotAuthorized,
    #[error("user was not found in this organization")]
    UserNotFound,
    #[error("device or offline grant was not found in this organization")]
    ResourceNotFound,
    #[error("the last active administrator cannot be disabled or demoted")]
    LastAdministrator,
    #[error("an administrator cannot change their own account role or status")]
    SelfModification,
    #[error("identity reference already exists in this organization")]
    IdentityConflict,
    #[error("user fields are invalid")]
    InvalidFields,
}

#[derive(Debug, Error)]
pub enum DeviceWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("device is already enrolled with another key")]
    IdentityMismatch,
    #[error("device is already enrolled")]
    AlreadyEnrolled,
    #[error("device was not found in the organization")]
    NotFound,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
    #[error("device provisioning fields are invalid")]
    InvalidProvisioning,
    #[error("offline grant signature is invalid")]
    InvalidProvisioningSignature,
    #[error("actor or target user is not authorized for device provisioning")]
    ProvisioningContextInvalid,
}

#[derive(Debug, Error)]
pub enum OfflineGrantWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("user or device does not belong to the organization")]
    ContextInvalid,
    #[error("offline grant dates are invalid")]
    InvalidDates,
    #[error("offline grant signature is invalid")]
    InvalidSignature,
    #[error("offline grant signature is not trusted")]
    UntrustedSignature,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum TariffWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("tariff fields are invalid")]
    InvalidFields,
    #[error("approval reference is required")]
    ApprovalRefRequired,
    #[error("tariff draft was not found")]
    NotFound,
    #[error("tariff is not a draft")]
    NotDraft,
    #[error("approved tariff periods overlap")]
    PeriodOverlap,
    #[error(
        "the tariff selected for replacement is incompatible or does not cover the new start date"
    )]
    InvalidSupersededTariff,
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
}

#[derive(Debug, Error)]
pub enum ProductWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("solar product fields are invalid")]
    InvalidFields,
    #[error("an approval reference is required")]
    ApprovalRefRequired,
    #[error("product draft was not found")]
    NotFound,
    #[error("product is not a draft")]
    NotDraft,
    #[error("approved product periods overlap")]
    PeriodOverlap,
    #[error("the product selected for replacement is incompatible")]
    InvalidSupersededProduct,
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
}

#[derive(Debug, Error)]
pub enum ReferenceCatalogWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("reference fields are invalid")]
    InvalidFields,
    #[error("an approval reference is required")]
    ApprovalRefRequired,
    #[error("reference draft was not found")]
    NotFound,
    #[error("reference is not a draft")]
    NotDraft,
    #[error("approved reference periods overlap")]
    PeriodOverlap,
    #[error("the reference selected for replacement is incompatible")]
    InvalidSupersededReference,
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
}

#[derive(Debug)]
pub enum CatalogWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

pub enum DeviceWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

pub enum OfflineGrantWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
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
    #[error("exactly one approved solar VAT profile must apply on the sale date")]
    TaxProfileUnavailable,
    #[error("solar sale principal does not match the approved product price")]
    PriceMismatch,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum SolarQuoteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("exactly one approved solar VAT profile must apply for the quote date")]
    TaxProfileUnavailable,
    #[error("multiple approved solar products apply for the same SKU and date")]
    ProductAmbiguous,
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
    pub revision: Option<String>,
}

#[derive(Debug, Error)]
pub enum UtilityIssueError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("invoice period already exists")]
    InvoicePeriodExists,
    #[error(
        "selected meter readings are invalid, already invoiced, or do not match invoice quantities"
    )]
    MeterReadingInvalid,
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
    #[error("payment would exceed the remaining installment balance")]
    InstallmentOverallocated,
    #[error("payments against an installment receivable must target a schedule installment")]
    InstallmentRequired,
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
pub enum PaymentAllocationWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("payment was not found in the organization")]
    PaymentNotFound,
    #[error("payment is not accepted centrally")]
    PaymentNotAccepted,
    #[error("payment allocations must be strictly positive")]
    InvalidAllocation,
    #[error("payment allocations exceed the received amount")]
    AllocationExceedsPayment,
    #[error("receivable balance would be overallocated")]
    Overallocated,
    #[error("payment would exceed the remaining installment balance")]
    InstallmentOverallocated,
    #[error("payments against an installment receivable must target a schedule installment")]
    InstallmentRequired,
    #[error("payment customer or currency does not match the receivable")]
    ContextInvalid,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
}

pub enum PaymentAllocationWriteOutcome {
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

#[derive(Clone, Debug)]
pub enum CreditNoteWriteOutcome {
    Created(serde_json::Value),
    Replay(serde_json::Value),
}

#[derive(Debug, Error)]
pub enum CreditNoteWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("idempotency key was reused with another payload")]
    IdempotencyPayloadMismatch,
    #[error("receivable was not found in the organization")]
    ReceivableNotFound,
    #[error("receivable cannot receive a credit in its current status")]
    ReceivableNotCreditable,
    #[error("credit note actor is not active in the organization")]
    ActorNotActive,
    #[error("credit note actor is not authorized")]
    ActorNotAuthorized,
    #[error("credit amount is invalid")]
    InvalidAmount,
    #[error("credit amount exceeds the outstanding receivable balance")]
    AmountExceedsBalance,
    #[error("credit note reason is required")]
    ReasonRequired,
    #[error("stored idempotency response is invalid: {0}")]
    StoredResponse(#[from] serde_json::Error),
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

#[derive(Debug, Error)]
pub enum MeterWriteError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("customer was not found in this organization")]
    CustomerNotFound,
    #[error("service account was not found in this organization")]
    ServiceAccountNotFound,
    #[error("service account revision changed during the update")]
    ServiceAccountRevisionConflict,
    #[error("meter was not found in this organization")]
    MeterNotFound,
    #[error("only an active meter can be replaced")]
    MeterNotActive,
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
    #[error("a correction reason is required")]
    CorrectionReasonRequired,
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

    /// Creates the first administrator only when the organization has no users.
    /// Locking the organization row serializes concurrent bootstrap attempts.
    /// The account and its redacted audit record are committed atomically.
    pub async fn bootstrap_initial_admin(
        &self,
        identity_ref: &str,
        password: &str,
    ) -> Result<Uuid, AuthError> {
        let identity_ref = identity_ref.trim();
        if identity_ref.is_empty() {
            return Err(AuthError::IdentityRefRequired);
        }
        let password_hash = Self::hash_user_password(password)?;
        let mut tx = self.pool.begin().await?;
        let organization =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM organizations WHERE id = $1 FOR UPDATE")
                .bind(self.organization_id)
                .fetch_optional(&mut *tx)
                .await?;
        if organization.is_none() {
            return Err(AuthError::OrganizationNotFound);
        }
        let any_user_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM users WHERE organization_id = $1)",
        )
        .bind(self.organization_id)
        .fetch_one(&mut *tx)
        .await?;
        if any_user_exists {
            return Err(AuthError::InitialAdminAlreadyProvisioned);
        }

        let admin_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
             VALUES ($1, $2, 'ADMIN', 'ACTIVE', $3, $4)",
        )
        .bind(admin_id)
        .bind(self.organization_id)
        .bind(identity_ref)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, NULL, $2, 'INITIAL_ADMIN_BOOTSTRAPPED', now(), $3)",
        )
        .bind(Uuid::new_v4())
        .bind(admin_id)
        .bind(json!({"role": "ADMIN"}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(admin_id)
    }

    /// Hash a password using Argon2id before it is provisioned into a user row.
    /// The API never accepts or returns a stored password hash.
    pub fn hash_user_password(password: &str) -> Result<String, AuthError> {
        if password.chars().count() < MIN_PASSWORD_LENGTH {
            return Err(AuthError::PasswordHash(
                "password must contain at least 8 characters".into(),
            ));
        }
        let salt = SaltString::generate(&mut rand::rngs::OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|error| AuthError::PasswordHash(error.to_string()))
    }

    /// Provision or rotate a user's password. The actor check, password update,
    /// session revocation, and redacted audit event commit as one transaction.
    pub async fn set_user_password(
        &self,
        user_id: Uuid,
        actor_id: Uuid,
        password: &str,
    ) -> Result<(), AuthError> {
        let password_hash = Self::hash_user_password(password)?;
        let mut tx = self.pool.begin().await?;
        let actor_role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM users
             WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'
             FOR UPDATE",
        )
        .bind(actor_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::ActorNotAuthorized)?;
        if actor_role != "ADMIN" {
            return Err(AuthError::ActorNotAuthorized);
        }
        let updated = sqlx::query(
            "UPDATE users SET password_hash = $1, password_change_required = TRUE
             WHERE id = $2 AND organization_id = $3 AND status = 'ACTIVE'",
        )
        .bind(password_hash)
        .bind(user_id)
        .bind(self.organization_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AuthError::UserNotFound);
        }
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = now()
             WHERE organization_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(self.organization_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'USER_PASSWORD_PROVISIONED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(user_id)
        .bind(json!({"sessions_revoked": true}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn change_own_password(
        &self,
        user_id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AuthError> {
        if new_password.chars().count() < MIN_PASSWORD_LENGTH {
            return Err(AuthError::PasswordHash(
                "password must contain at least 8 characters".into(),
            ));
        }
        if current_password == new_password {
            return Err(AuthError::PasswordUnchanged);
        }
        let new_hash = Self::hash_user_password(new_password)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT password_hash FROM users
             WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'
             FOR UPDATE",
        )
        .bind(user_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::UserNotFound)?;
        let stored_hash: Option<String> = row.try_get("password_hash")?;
        let stored_hash = stored_hash.ok_or(AuthError::InvalidCredentials)?;
        let parsed = PasswordHash::new(&stored_hash)
            .map_err(|error| AuthError::PasswordHash(error.to_string()))?;
        Argon2::default()
            .verify_password(current_password.as_bytes(), &parsed)
            .map_err(|_| AuthError::InvalidCredentials)?;
        sqlx::query(
            "UPDATE users SET password_hash = $1, password_change_required = FALSE
             WHERE id = $2 AND organization_id = $3",
        )
        .bind(new_hash)
        .bind(user_id)
        .bind(self.organization_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE auth_sessions SET revoked_at = now()
             WHERE organization_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(self.organization_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $2, 'USER_PASSWORD_CHANGED', now(), '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn login_user(
        &self,
        identity_ref: &str,
        password: &str,
    ) -> Result<serde_json::Value, AuthError> {
        let row = sqlx::query(
            "SELECT id, role, password_hash, password_change_required
             FROM users
             WHERE organization_id = $1 AND lower(btrim(identity_ref)) = lower(btrim($2))
               AND status = 'ACTIVE'",
        )
        .bind(self.organization_id)
        .bind(identity_ref.trim())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
        let user_id: Uuid = row.try_get("id")?;
        let role: String = row.try_get("role")?;
        let password_change_required: bool = row.try_get("password_change_required")?;
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
        // A central session survives application restarts but expires absolutely after three days.
        let refresh_expires_at = Utc::now() + Duration::days(3);
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
            "require_password_change": password_change_required,
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
            "SELECT s.id, s.user_id, u.role, u.password_change_required, s.refresh_expires_at
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
        let password_change_required: bool = row.try_get("password_change_required")?;
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
            "require_password_change": password_change_required,
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
            "SELECT s.user_id, s.organization_id, u.role, u.password_change_required
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
            password_change_required: row.try_get("password_change_required")?,
        })
    }

    pub async fn enroll_device(
        &self,
        device_id: Uuid,
        public_key_id: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<DeviceWriteOutcome, DeviceWriteError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'device.enroll' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(DeviceWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(DeviceWriteOutcome::Replay(response));
        }
        let existing = sqlx::query(
            "SELECT public_key_id, state FROM devices
             WHERE id = $1 AND organization_id = $2 FOR UPDATE",
        )
        .bind(device_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            let stored_key: String = existing.try_get("public_key_id")?;
            if stored_key != public_key_id {
                return Err(DeviceWriteError::IdentityMismatch);
            }
            return Err(DeviceWriteError::AlreadyEnrolled);
        }
        let duplicate_key = sqlx::query(
            "SELECT id FROM devices
             WHERE organization_id = $1 AND public_key_id = $2 FOR UPDATE",
        )
        .bind(self.organization_id)
        .bind(public_key_id)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate_key.is_some() {
            return Err(DeviceWriteError::IdentityMismatch);
        }
        let response = json!({
            "device_id": device_id,
            "organization_id": self.organization_id,
            "public_key_id": public_key_id,
            "state": "ACTIVE"
        });
        sqlx::query(
            "INSERT INTO devices (id, organization_id, public_key_id, state)
             VALUES ($1, $2, $3, 'ACTIVE')",
        )
        .bind(device_id)
        .bind(self.organization_id)
        .bind(public_key_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'DEVICE_ENROLLED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(device_id)
        .bind(json!({"public_key_id": public_key_id}))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'device.enroll', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DeviceWriteOutcome::Created(response))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn provision_device(
        &self,
        device_id: Uuid,
        public_key_id: &str,
        sync_token_hash: &str,
        user_id: Uuid,
        grant_id: Uuid,
        scopes: &serde_json::Value,
        limits: &serde_json::Value,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        signature: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<DeviceWriteOutcome, DeviceWriteError> {
        // PostgreSQL TIMESTAMPTZ stores microseconds. Normalize before signing,
        // persisting, and returning the grant so all representations match.
        let issued_at = postgres_timestamp_precision(issued_at);
        let expires_at = postgres_timestamp_precision(expires_at);
        if public_key_id.trim().is_empty()
            || sync_token_hash.len() != 64
            || !sync_token_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || device_id.is_nil()
            || grant_id.is_nil()
            || expires_at <= issued_at
        {
            return Err(DeviceWriteError::InvalidProvisioning);
        }
        let signature_bytes = BASE64
            .decode(signature)
            .map_err(|_| DeviceWriteError::InvalidProvisioningSignature)?;
        let parsed_signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| DeviceWriteError::InvalidProvisioningSignature)?;
        if self.require_signed_offline_grants {
            let Some(verifying_key) = &self.offline_grant_public_key else {
                return Err(DeviceWriteError::InvalidProvisioningSignature);
            };
            let canonical = offline_grant_canonical_bytes(
                grant_id, user_id, device_id, scopes, limits, issued_at, expires_at,
            )
            .map_err(|_| DeviceWriteError::InvalidProvisioningSignature)?;
            if verifying_key.verify(&canonical, &parsed_signature).is_err() {
                return Err(DeviceWriteError::InvalidProvisioningSignature);
            }
        }

        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'device.provision' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(DeviceWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(DeviceWriteOutcome::Replay(response));
        }

        let context_valid = sqlx::query(
            "SELECT 1 FROM users actor
             JOIN users target ON target.organization_id = actor.organization_id
             WHERE actor.id = $1 AND actor.organization_id = $2
               AND actor.role = 'ADMIN' AND actor.status = 'ACTIVE'
               AND target.id = $3 AND target.status = 'ACTIVE' FOR UPDATE OF actor, target",
        )
        .bind(actor_id)
        .bind(self.organization_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !context_valid {
            return Err(DeviceWriteError::ProvisioningContextInvalid);
        }

        let response = json!({
            "device_id": device_id,
            "organization_id": self.organization_id,
            "public_key_id": public_key_id,
            "state": "ACTIVE",
            "offline_grant": {
                "grant_id": grant_id,
                "user_id": user_id,
                "device_id": device_id,
                "scopes": scopes,
                "limits": limits,
                "issued_at": issued_at,
                "expires_at": expires_at,
                "signature": signature
            }
        });
        sqlx::query(
            "INSERT INTO devices (id, organization_id, public_key_id, state, sync_token_hash)
             VALUES ($1, $2, $3, 'ACTIVE', $4)",
        )
        .bind(device_id)
        .bind(self.organization_id)
        .bind(public_key_id.trim())
        .bind(sync_token_hash.to_ascii_lowercase())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO offline_grants
             (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(grant_id)
        .bind(user_id)
        .bind(device_id)
        .bind(scopes)
        .bind(limits)
        .bind(issued_at)
        .bind(expires_at)
        .bind(signature)
        .execute(&mut *tx)
        .await?;
        for (event_type, subject_id, metadata) in [
            (
                "DEVICE_ENROLLED",
                device_id,
                json!({ "public_key_id": public_key_id }),
            ),
            (
                "OFFLINE_GRANT_ISSUED",
                grant_id,
                json!({ "user_id": user_id, "device_id": device_id }),
            ),
        ] {
            sqlx::query(
                "INSERT INTO audit_events
                 (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
                 VALUES ($1,$2,$3,$4,now(),$5)",
            )
            .bind(Uuid::new_v4())
            .bind(actor_id)
            .bind(subject_id)
            .bind(event_type)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1,$2,'device.provision',$3,$4,201,$5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DeviceWriteOutcome::Created(response))
    }

    pub async fn authenticate_sync_device_token(
        &self,
        token: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let device_id = sqlx::query_scalar(
            "SELECT id FROM devices
             WHERE organization_id = $1 AND sync_token_hash = $2 AND state = 'ACTIVE'",
        )
        .bind(self.organization_id)
        .bind(token_digest(token))
        .fetch_optional(&self.pool)
        .await?;
        Ok(device_id)
    }

    pub async fn renew_device_offline_grant(
        &self,
        device_id: Uuid,
        grant_id: Uuid,
        signing_key: &SigningKey,
        now: DateTime<Utc>,
    ) -> Result<serde_json::Value, DeviceWriteError> {
        let mut tx = self.pool.begin().await?;
        let device_active = sqlx::query(
            "SELECT id FROM devices
             WHERE id = $1 AND organization_id = $2 AND state = 'ACTIVE'
             FOR UPDATE",
        )
        .bind(device_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        if device_active.is_none() {
            return Err(DeviceWriteError::NotFound);
        }

        let Some(grant) = sqlx::query(
            "SELECT user_id, scopes, limits, issued_at, expires_at
             FROM offline_grants
             WHERE id = $1 AND device_id = $2 AND revoked_at IS NULL
             FOR UPDATE",
        )
        .bind(grant_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(DeviceWriteError::NotFound);
        };
        let user_id: Uuid = grant.try_get("user_id")?;
        let scopes: serde_json::Value = grant.try_get("scopes")?;
        let limits: serde_json::Value = grant.try_get("limits")?;
        let previous_issued_at: DateTime<Utc> = grant.try_get("issued_at")?;
        let previous_expires_at: DateTime<Utc> = grant.try_get("expires_at")?;

        let user_active = sqlx::query(
            "SELECT id FROM users
             WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'
             FOR UPDATE",
        )
        .bind(user_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        if user_active.is_none() {
            return Err(DeviceWriteError::ProvisioningContextInvalid);
        }

        if previous_expires_at > now + Duration::days(1) {
            tx.commit().await?;
            return Ok(json!({
                "grant_id": grant_id,
                "device_id": device_id,
                "user_id": user_id,
                "expires_at": previous_expires_at,
                "renewed": false
            }));
        }

        let issued_at = postgres_timestamp_precision(now);
        let expires_at = postgres_timestamp_precision(issued_at + Duration::days(7));
        let canonical = offline_grant_canonical_bytes(
            grant_id, user_id, device_id, &scopes, &limits, issued_at, expires_at,
        )
        .map_err(|_| DeviceWriteError::InvalidProvisioningSignature)?;
        let signature = BASE64.encode(signing_key.sign(&canonical).to_bytes());

        sqlx::query(
            "INSERT INTO offline_grant_renewals
             (id, grant_id, device_id, previous_issued_at, previous_expires_at,
              issued_at, expires_at, signature)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(Uuid::new_v4())
        .bind(grant_id)
        .bind(device_id)
        .bind(previous_issued_at)
        .bind(previous_expires_at)
        .bind(issued_at)
        .bind(expires_at)
        .bind(&signature)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE offline_grants
             SET issued_at = $1, expires_at = $2, signature = $3
             WHERE id = $4 AND device_id = $5 AND revoked_at IS NULL",
        )
        .bind(issued_at)
        .bind(expires_at)
        .bind(&signature)
        .bind(grant_id)
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1,$2,$3,'OFFLINE_GRANT_RENEWED',$4,$5)",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(grant_id)
        .bind(now)
        .bind(json!({"device_id":device_id}))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(json!({
            "grant_id": grant_id,
            "device_id": device_id,
            "user_id": user_id,
            "issued_at": issued_at,
            "expires_at": expires_at,
            "signature": signature,
            "renewed": true
        }))
    }

    pub async fn revoke_device(
        &self,
        device_id: Uuid,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<DeviceWriteOutcome, DeviceWriteError> {
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'device.revoke' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(DeviceWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(DeviceWriteOutcome::Replay(response));
        }
        let device = sqlx::query(
            "SELECT public_key_id FROM devices
             WHERE id = $1 AND organization_id = $2 FOR UPDATE",
        )
        .bind(device_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(device) = device else {
            return Err(DeviceWriteError::NotFound);
        };
        let public_key_id: String = device.try_get("public_key_id")?;
        sqlx::query("UPDATE devices SET state = 'REVOKED' WHERE id = $1")
            .bind(device_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE offline_grants SET revoked_at = now() WHERE device_id = $1 AND revoked_at IS NULL")
            .bind(device_id)
            .execute(&mut *tx)
            .await?;
        let response = json!({
            "device_id": device_id,
            "organization_id": self.organization_id,
            "public_key_id": public_key_id,
            "state": "REVOKED"
        });
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'DEVICE_REVOKED', now(), '{}'::jsonb)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'device.revoke', $3, $4, 204, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DeviceWriteOutcome::Created(response))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_offline_grant(
        &self,
        grant_id: Uuid,
        user_id: Uuid,
        device_id: Uuid,
        scopes: &serde_json::Value,
        limits: &serde_json::Value,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        signature: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<OfflineGrantWriteOutcome, OfflineGrantWriteError> {
        if expires_at <= issued_at {
            return Err(OfflineGrantWriteError::InvalidDates);
        }
        let signature_bytes = BASE64
            .decode(signature)
            .map_err(|_| OfflineGrantWriteError::InvalidSignature)?;
        let parsed_signature = Signature::from_slice(&signature_bytes)
            .map_err(|_| OfflineGrantWriteError::InvalidSignature)?;
        if self.require_signed_offline_grants {
            let Some(verifying_key) = &self.offline_grant_public_key else {
                return Err(OfflineGrantWriteError::UntrustedSignature);
            };
            let canonical = offline_grant_canonical_bytes(
                grant_id, user_id, device_id, scopes, limits, issued_at, expires_at,
            )?;
            if verifying_key.verify(&canonical, &parsed_signature).is_err() {
                return Err(OfflineGrantWriteError::UntrustedSignature);
            }
        }
        let mut tx = self.pool.begin().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'offline_grant.create' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(OfflineGrantWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(OfflineGrantWriteOutcome::Replay(response));
        }
        let context_exists = sqlx::query(
            "SELECT 1 FROM users u JOIN devices d ON d.organization_id = u.organization_id
             WHERE u.id = $1 AND u.organization_id = $2 AND u.status = 'ACTIVE'
               AND d.id = $3 AND d.state = 'ACTIVE' FOR UPDATE OF u, d",
        )
        .bind(user_id)
        .bind(self.organization_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if !context_exists {
            return Err(OfflineGrantWriteError::ContextInvalid);
        }
        let response = json!({
            "grant_id": grant_id,
            "user_id": user_id,
            "device_id": device_id,
            "scopes": scopes,
            "limits": limits,
            "issued_at": issued_at,
            "expires_at": expires_at,
            "signature": signature
        });
        sqlx::query(
            "INSERT INTO offline_grants
             (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(grant_id)
        .bind(user_id)
        .bind(device_id)
        .bind(scopes)
        .bind(limits)
        .bind(issued_at)
        .bind(expires_at)
        .bind(signature)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'OFFLINE_GRANT_ISSUED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(grant_id)
        .bind(json!({"user_id": user_id, "device_id": device_id}))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'offline_grant.create', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(OfflineGrantWriteOutcome::Created(response))
    }

    pub async fn create_customer(&self, input: &CreateCustomer) -> Result<Customer, sqlx::Error> {
        let id = Uuid::new_v4();
        let contact_fields = json!({
            "address": input.address,
            "phone": input.phone,
        });
        let row = sqlx::query(
            "INSERT INTO customers
             (id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision)
             VALUES ($1, $2, $3, $4, $5, $6, 'ACTIVE', 1)
             RETURNING id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision",
        )
        .bind(id)
        .bind(self.organization_id)
        .bind(&input.customer_code)
        .bind(&input.legal_name)
        .bind(input.customer_category.as_deref().map(str::trim).map(str::to_ascii_uppercase))
        .bind(contact_fields)
        .fetch_one(&self.pool)
        .await?;
        customer_from_row(&row)
    }

    pub async fn get_customer(&self, id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision
             FROM customers WHERE id = $1 AND organization_id = $2",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| customer_from_row(&row)).transpose()
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
        let rows = sqlx::query(
            "SELECT id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision
             FROM customers
             WHERE organization_id = $1
               AND ($2::text IS NULL OR customer_code > $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::text IS NULL OR strpos(lower(customer_code), lower($4)) > 0
                    OR strpos(lower(legal_name), lower($4)) > 0)
             ORDER BY customer_code
             LIMIT $5",
        )
        .bind(self.organization_id)
        .bind(after_customer_code)
        .bind(status)
        .bind(search)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await?;
        let next_cursor = if rows.len() as i64 > limit.max(1) {
            rows.get(limit.max(1) as usize - 1)
                .map(|row| row.try_get::<String, _>("customer_code"))
                .transpose()?
        } else {
            None
        };
        let customers = rows
            .iter()
            .take(limit.max(1) as usize)
            .map(customer_from_row)
            .collect::<Result<_, _>>()?;
        Ok((customers, next_cursor))
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
        let address = input.address.apply_to(current.address);
        let phone = input.phone.apply_to(current.phone);
        let customer_category = input
            .customer_category
            .apply_to(current.customer_category)
            .map(|value| value.trim().to_ascii_uppercase());
        let status = input.status.clone().unwrap_or(current.status);
        let contact_fields = json!({"address": address, "phone": phone});
        let row = sqlx::query(
            "UPDATE customers
             SET legal_name = $1, customer_category = $2, contact_fields = $3,
                 status = $4, revision = revision + 1
             WHERE id = $5 AND organization_id = $6 AND revision = $7
             RETURNING id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision",
        )
        .bind(legal_name)
        .bind(customer_category)
        .bind(contact_fields)
        .bind(status)
        .bind(id)
        .bind(self.organization_id)
        .bind(expected_revision)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| customer_from_row(&row)).transpose()
    }

    pub async fn utility_meter_reading_quantities(
        &self,
        customer_id: Uuid,
        period: &str,
        refs: &UtilityMeterReadingRefs,
    ) -> Result<(Decimal, Decimal), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let quantities = load_invoice_meter_readings_in_tx(
            &mut tx,
            self.organization_id,
            customer_id,
            period,
            refs,
        )
        .await
        .map_err(|error| match error {
            UtilityIssueError::Database(database) => database,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
        tx.commit().await?;
        Ok(quantities)
    }

    pub async fn load_tariffs(
        &self,
        revision: &str,
        currency: &str,
        period: &str,
        customer_category: &str,
    ) -> Result<TariffSnapshot, sqlx::Error> {
        let customer_category = customer_category.trim().to_ascii_uppercase();
        if customer_category.is_empty() {
            return Err(sqlx::Error::Protocol(
                "customer category is required for tariff selection".into(),
            ));
        }
        let effective_date = NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
            .map_err(|error| sqlx::Error::Protocol(format!("invalid billing period: {error}")))?;
        // The server is authoritative for tariff selection. Older/newer clients may
        // omit the display-only revision; in that case resolve the single approved
        // electricity revision for the customer's category and billing period.
        let revision = if revision.trim().is_empty() {
            let revisions = sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT approval_ref FROM tariff_versions
                 WHERE service_code = 'ELECTRICITY' AND unit = 'KWH' AND currency = $1
                   AND status = 'APPROVED' AND customer_category = $2
                   AND effective_from <= $3 AND (effective_to IS NULL OR effective_to >= $3)
                 ORDER BY approval_ref",
            )
            .bind(currency)
            .bind(&customer_category)
            .bind(effective_date)
            .fetch_all(&self.pool)
            .await?;
            if revisions.len() > 1 {
                return Err(sqlx::Error::Protocol(
                    "multiple approved tariff revisions apply to this customer category and period"
                        .into(),
                ));
            }
            revisions.into_iter().next().unwrap_or_default()
        } else {
            revision.trim().to_owned()
        };
        let electricity = sqlx::query(
            "SELECT unit_price FROM tariff_versions
             WHERE service_code = 'ELECTRICITY' AND unit = 'KWH' AND currency = $1
               AND approval_ref = $2 AND status = 'APPROVED' AND customer_category = $3
               AND effective_from <= $4 AND (effective_to IS NULL OR effective_to >= $4)
             ORDER BY effective_from DESC",
        )
        .bind(currency)
        .bind(&revision)
        .bind(&customer_category)
        .bind(effective_date)
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
        .bind(effective_date)
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
        .bind(&revision)
        .bind(&customer_category)
        .bind(effective_date)
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
        .bind(effective_date)
        .fetch_all(&self.pool)
        .await?;
        if water_taxes.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "multiple applicable water tax profiles require an explicit SEL tax mapping".into(),
            ));
        }
        Ok(TariffSnapshot {
            electricity_unit_price: electricity
                .first()
                .map(|row| row.try_get("unit_price"))
                .transpose()?,
            electricity_tax_rate: electricity_taxes
                .first()
                .map(|row| row.try_get("rate"))
                .transpose()?,
            water_unit_price: water
                .first()
                .map(|row| row.try_get("unit_price"))
                .transpose()?,
            water_tax_rate: water_taxes
                .first()
                .map(|row| row.try_get("rate"))
                .transpose()?,
            revision: (!revision.is_empty()).then_some(revision),
        })
    }

    pub async fn catalog_products(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, sku, product_type, label, price_reference, currency,
                    effective_from, effective_to, approval_ref
             FROM products
             WHERE active = TRUE AND status = 'APPROVED'
               AND effective_from <= CURRENT_DATE
               AND (effective_to IS NULL OR effective_to >= CURRENT_DATE)
             ORDER BY sku",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let price: Decimal = row.try_get("price_reference")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "sku": row.try_get::<String, _>("sku")?,
                    "product_type": row.try_get::<String, _>("product_type")?,
                    "label": row.try_get::<String, _>("label")?,
                    "price_reference": price.normalize().to_string(),
                    "currency": row.try_get::<String, _>("currency")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?
                }))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_solar_product_draft(
        &self,
        sku: &str,
        label: &str,
        price: Decimal,
        currency: &str,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ProductWriteError> {
        let sku = sku.trim().to_ascii_uppercase();
        let label = label.trim();
        let currency = currency.trim().to_ascii_uppercase();
        let max_price = Decimal::new(99_999_999_999_999, 0);
        if sku.is_empty()
            || sku.len() > 64
            || !sku
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || label.is_empty()
            || label.len() > 160
            || price < Decimal::ZERO
            || price > max_price
            || price.normalize().scale() > 6
            || !matches!(currency.as_str(), "USD" | "CDF")
            || effective_to.is_some_and(|date| date < effective_from)
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(ProductWriteError::InvalidFields);
        }

        let mut tx = self.pool.begin().await?;
        let lock_key = format!(
            "idempotency:{}:{}:catalog.product.create:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.product.create' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_digest: String = row.try_get("request_digest")?;
            if stored_digest != request_digest {
                return Err(ProductWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(CatalogWriteOutcome::Replay(response));
        }
        let sku_lock = format!("solar-product:{sku}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(sku_lock)
            .execute(&mut *tx)
            .await?;
        let duplicate = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM products WHERE sku = $1 AND effective_from = $2)",
        )
        .bind(&sku)
        .bind(effective_from)
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            return Err(ProductWriteError::PeriodOverlap);
        }

        let id = Uuid::new_v4();
        let response = json!({
            "id": id,
            "sku": sku,
            "product_type": "SOLAR_KIT",
            "label": label,
            "price_reference": price.normalize().to_string(),
            "currency": currency,
            "active": false,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": null,
            "status": "DRAFT"
        });
        sqlx::query(
            "INSERT INTO products
             (id, sku, product_type, label, price_reference, currency, active,
              effective_from, effective_to, status, approval_ref)
             VALUES ($1, $2, 'SOLAR_KIT', $3, $4, $5, FALSE, $6, $7, 'DRAFT', NULL)",
        )
        .bind(id)
        .bind(&sku)
        .bind(label)
        .bind(price)
        .bind(&currency)
        .bind(effective_from)
        .bind(effective_to)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'SOLAR_PRODUCT_DRAFTED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(id)
        .bind(json!({ "sku": sku, "currency": currency }))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.product.create', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn approve_solar_product(
        &self,
        product_id: Uuid,
        approval_ref: &str,
        supersedes_product_id: Option<Uuid>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ProductWriteError> {
        let approval_ref = approval_ref.trim();
        if approval_ref.is_empty() || approval_ref.len() > 160 {
            return Err(ProductWriteError::ApprovalRefRequired);
        }
        if idempotency_key.trim().is_empty() || request_digest.trim().is_empty() {
            return Err(ProductWriteError::InvalidFields);
        }
        let mut tx = self.pool.begin().await?;
        let idem_lock = format!(
            "idempotency:{}:{}:catalog.product.approve:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(idem_lock)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.product.approve' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_digest")? != request_digest {
                return Err(ProductWriteError::IdempotencyPayloadMismatch);
            }
            return Ok(CatalogWriteOutcome::Replay(row.try_get("response_json")?));
        }

        let product = sqlx::query(
            "SELECT sku, product_type, label, price_reference, currency,
                    effective_from, effective_to, status
             FROM products WHERE id = $1 FOR UPDATE",
        )
        .bind(product_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ProductWriteError::NotFound)?;
        if product.try_get::<String, _>("status")? != "DRAFT" {
            return Err(ProductWriteError::NotDraft);
        }
        let sku: String = product.try_get("sku")?;
        let product_type: String = product.try_get("product_type")?;
        let label: String = product.try_get("label")?;
        let price: Decimal = product.try_get("price_reference")?;
        let currency: String = product.try_get("currency")?;
        let effective_from: NaiveDate = product.try_get("effective_from")?;
        let effective_to: Option<NaiveDate> = product.try_get("effective_to")?;
        let sku_lock = format!("solar-product:{sku}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(sku_lock)
            .execute(&mut *tx)
            .await?;

        if let Some(previous_id) = supersedes_product_id {
            if previous_id == product_id {
                return Err(ProductWriteError::InvalidSupersededProduct);
            }
            let previous = sqlx::query(
                "SELECT sku, product_type, currency, effective_from, effective_to, status
                 FROM products WHERE id = $1 FOR UPDATE",
            )
            .bind(previous_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ProductWriteError::InvalidSupersededProduct)?;
            if previous.try_get::<String, _>("status")? != "APPROVED"
                || previous.try_get::<String, _>("sku")? != sku
                || previous.try_get::<String, _>("product_type")? != product_type
                || previous.try_get::<String, _>("currency")? != currency
                || previous.try_get::<NaiveDate, _>("effective_from")? >= effective_from
                || previous
                    .try_get::<Option<NaiveDate>, _>("effective_to")?
                    .is_some_and(|date| date < effective_from)
            {
                return Err(ProductWriteError::InvalidSupersededProduct);
            }
        }
        let overlap = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM products
                WHERE sku = $1 AND status = 'APPROVED' AND id <> $2
                  AND ($3::uuid IS NULL OR id <> $3)
                  AND ($5::date IS NULL OR effective_from <= $5)
                  AND (effective_to IS NULL OR effective_to >= $4)
            )",
        )
        .bind(&sku)
        .bind(product_id)
        .bind(supersedes_product_id)
        .bind(effective_from)
        .bind(effective_to)
        .fetch_one(&mut *tx)
        .await?;
        if overlap {
            return Err(ProductWriteError::PeriodOverlap);
        }
        if let Some(previous_id) = supersedes_product_id {
            sqlx::query("UPDATE products SET effective_to = $2 WHERE id = $1")
                .bind(previous_id)
                .bind(effective_from - chrono::Duration::days(1))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "UPDATE products SET active = TRUE, status = 'APPROVED',
                    approval_ref = $2, approved_by = $3 WHERE id = $1 AND status = 'DRAFT'",
        )
        .bind(product_id)
        .bind(approval_ref)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        let response = json!({
            "id": product_id,
            "sku": sku,
            "product_type": product_type,
            "label": label,
            "price_reference": price.normalize().to_string(),
            "currency": currency,
            "active": true,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": approval_ref,
            "status": "APPROVED"
        });
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'SOLAR_PRODUCT_APPROVED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(product_id)
        .bind(
            json!({ "approval_ref": approval_ref, "supersedes_product_id": supersedes_product_id }),
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.product.approve', $3, $4, 200, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn revoke_solar_product(
        &self,
        product_id: Uuid,
        change_ref: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ProductWriteError> {
        let change_ref = change_ref.trim();
        if change_ref.is_empty()
            || change_ref.len() > 160
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(ProductWriteError::InvalidFields);
        }
        let mut tx = self.pool.begin().await?;
        let idem_lock = format!(
            "idempotency:{}:{}:catalog.product.revoke:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(idem_lock)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.product.revoke' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_digest")? != request_digest {
                return Err(ProductWriteError::IdempotencyPayloadMismatch);
            }
            return Ok(CatalogWriteOutcome::Replay(row.try_get("response_json")?));
        }
        let row = sqlx::query(
            "SELECT sku, product_type, label, price_reference, currency, active,
                    effective_from, effective_to, approval_ref, status
             FROM products WHERE id = $1 FOR UPDATE",
        )
        .bind(product_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ProductWriteError::NotFound)?;
        let old_status: String = row.try_get("status")?;
        if !matches!(old_status.as_str(), "DRAFT" | "APPROVED" | "REVOKED") {
            return Err(ProductWriteError::NotDraft);
        }
        let price: Decimal = row.try_get("price_reference")?;
        let effective_from: NaiveDate = row.try_get("effective_from")?;
        let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
        let response = json!({
            "id": product_id,
            "sku": row.try_get::<String, _>("sku")?,
            "product_type": row.try_get::<String, _>("product_type")?,
            "label": row.try_get::<String, _>("label")?,
            "price_reference": price.normalize().to_string(),
            "currency": row.try_get::<String, _>("currency")?,
            "active": false,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
            "status": "REVOKED"
        });
        if old_status != "REVOKED" {
            sqlx::query("UPDATE products SET active = FALSE, status = 'REVOKED' WHERE id = $1")
                .bind(product_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO audit_events
                 (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
                 VALUES ($1, $2, $3, 'SOLAR_PRODUCT_REVOKED', now(), $4)",
            )
            .bind(Uuid::new_v4())
            .bind(actor_id)
            .bind(product_id)
            .bind(json!({ "change_ref": change_ref }))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.product.revoke', $3, $4, 200, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn admin_catalog_products(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, sku, product_type, label, price_reference, currency, active,
                    effective_from, effective_to, approval_ref, approved_by, status
             FROM products ORDER BY sku, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let price: Decimal = row.try_get("price_reference")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "sku": row.try_get::<String, _>("sku")?,
                    "product_type": row.try_get::<String, _>("product_type")?,
                    "label": row.try_get::<String, _>("label")?,
                    "price_reference": price.normalize().to_string(),
                    "currency": row.try_get::<String, _>("currency")?,
                    "active": row.try_get::<bool, _>("active")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
                    "approved_by": row.try_get::<Option<Uuid>, _>("approved_by")?,
                    "status": row.try_get::<String, _>("status")?
                }))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_tariff_draft(
        &self,
        service_code: &str,
        customer_category: &str,
        unit: &str,
        unit_price: Decimal,
        currency: &str,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, TariffWriteError> {
        let service_code = service_code.trim().to_ascii_uppercase();
        let customer_category = customer_category.trim().to_ascii_uppercase();
        let unit = unit.trim().to_ascii_uppercase();
        let currency = currency.trim().to_ascii_uppercase();
        let expected_unit = match service_code.as_str() {
            "ELECTRICITY" => "KWH",
            "WATER" => "M3",
            _ => return Err(TariffWriteError::InvalidFields),
        };
        let maximum_unit_price = Decimal::new(99_999_999_999_999, 0);
        if customer_category.is_empty()
            || customer_category.len() > 64
            || unit != expected_unit
            || unit_price < Decimal::ZERO
            || unit_price > maximum_unit_price
            || unit_price.normalize().scale() > 6
            || !matches!(currency.as_str(), "USD" | "CDF")
            || effective_to.is_some_and(|date| date < effective_from)
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(TariffWriteError::InvalidFields);
        }

        let mut tx = self.pool.begin().await?;
        let lock_key = format!(
            "idempotency:{}:{}:catalog.tariff.create:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.tariff.create' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_digest: String = row.try_get("request_digest")?;
            if stored_digest != request_digest {
                return Err(TariffWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(CatalogWriteOutcome::Replay(response));
        }

        let id = Uuid::new_v4();
        let response = json!({
            "id": id,
            "service_code": service_code,
            "customer_category": customer_category,
            "unit": unit,
            "unit_price": unit_price.normalize().to_string(),
            "currency": currency,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": null,
            "status": "DRAFT"
        });
        sqlx::query(
            "INSERT INTO tariff_versions
             (id, service_code, customer_category, unit, unit_price, currency,
              effective_from, effective_to, approval_ref, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, 'DRAFT')",
        )
        .bind(id)
        .bind(&service_code)
        .bind(&customer_category)
        .bind(&unit)
        .bind(unit_price)
        .bind(&currency)
        .bind(effective_from)
        .bind(effective_to)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'TARIFF_VERSION_DRAFTED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(id)
        .bind(json!({"service_code": service_code, "customer_category": customer_category}))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.tariff.create', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn approve_tariff_draft(
        &self,
        tariff_id: Uuid,
        approval_ref: &str,
        supersedes_tariff_id: Option<Uuid>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, TariffWriteError> {
        let approval_ref = approval_ref.trim();
        if approval_ref.is_empty() || approval_ref.len() > 160 {
            return Err(TariffWriteError::ApprovalRefRequired);
        }
        if idempotency_key.trim().is_empty() || request_digest.trim().is_empty() {
            return Err(TariffWriteError::InvalidFields);
        }

        let mut tx = self.pool.begin().await?;
        let idempotency_lock_key = format!(
            "idempotency:{}:{}:catalog.tariff.approve:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(idempotency_lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.tariff.approve' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_digest: String = row.try_get("request_digest")?;
            if stored_digest != request_digest {
                return Err(TariffWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(CatalogWriteOutcome::Replay(response));
        }

        let draft = sqlx::query(
            "SELECT service_code, customer_category, unit, unit_price, currency,
                    effective_from, effective_to, status
             FROM tariff_versions WHERE id = $1 FOR UPDATE",
        )
        .bind(tariff_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(TariffWriteError::NotFound)?;
        let status: String = draft.try_get("status")?;
        if status != "DRAFT" {
            return Err(TariffWriteError::NotDraft);
        }
        let service_code: String = draft.try_get("service_code")?;
        let customer_category: String = draft.try_get("customer_category")?;
        let unit: String = draft.try_get("unit")?;
        let unit_price: Decimal = draft.try_get("unit_price")?;
        let currency: String = draft.try_get("currency")?;
        let effective_from: NaiveDate = draft.try_get("effective_from")?;
        let effective_to: Option<NaiveDate> = draft.try_get("effective_to")?;

        let lock_key = format!("tariff:{service_code}:{customer_category}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(previous_id) = supersedes_tariff_id {
            if previous_id == tariff_id {
                return Err(TariffWriteError::InvalidSupersededTariff);
            }
            let previous = sqlx::query(
                "SELECT service_code, customer_category, unit, currency, effective_from,
                        effective_to, status
                 FROM tariff_versions WHERE id = $1 FOR UPDATE",
            )
            .bind(previous_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(TariffWriteError::InvalidSupersededTariff)?;
            let previous_status: String = previous.try_get("status")?;
            let previous_service: String = previous.try_get("service_code")?;
            let previous_category: String = previous.try_get("customer_category")?;
            let previous_unit: String = previous.try_get("unit")?;
            let previous_currency: String = previous.try_get("currency")?;
            let previous_from: NaiveDate = previous.try_get("effective_from")?;
            let previous_to: Option<NaiveDate> = previous.try_get("effective_to")?;
            if previous_status != "APPROVED"
                || previous_service != service_code
                || previous_category != customer_category
                || previous_unit != unit
                || previous_currency != currency
                || previous_from >= effective_from
                || previous_to.is_some_and(|date| date < effective_from)
            {
                return Err(TariffWriteError::InvalidSupersededTariff);
            }
        }

        let overlap = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM tariff_versions
                WHERE id <> $1 AND ($2::uuid IS NULL OR id <> $2)
                  AND status = 'APPROVED'
                  AND service_code = $3 AND customer_category = $4
                  AND ($6::date IS NULL OR effective_from <= $6)
                  AND (effective_to IS NULL OR effective_to >= $5)
            )",
        )
        .bind(tariff_id)
        .bind(supersedes_tariff_id)
        .bind(&service_code)
        .bind(&customer_category)
        .bind(effective_from)
        .bind(effective_to)
        .fetch_one(&mut *tx)
        .await?;
        if overlap {
            return Err(TariffWriteError::PeriodOverlap);
        }

        if let Some(previous_id) = supersedes_tariff_id {
            let previous_to = effective_from - chrono::Duration::days(1);
            sqlx::query(
                "UPDATE tariff_versions SET effective_to = $2
                 WHERE id = $1 AND status = 'APPROVED'",
            )
            .bind(previous_id)
            .bind(previous_to)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "UPDATE tariff_versions
             SET status = 'APPROVED', approval_ref = $2, approved_by = $3
             WHERE id = $1 AND status = 'DRAFT'",
        )
        .bind(tariff_id)
        .bind(approval_ref)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        let response = json!({
            "id": tariff_id,
            "service_code": service_code,
            "customer_category": customer_category,
            "unit": unit,
            "unit_price": unit_price.normalize().to_string(),
            "currency": currency,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "status": "APPROVED",
            "approval_ref": approval_ref
        });
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, 'TARIFF_VERSION_APPROVED', now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(tariff_id)
        .bind(json!({"approval_ref": approval_ref, "supersedes_tariff_id": supersedes_tariff_id}))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.tariff.approve', $3, $4, 200, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn revoke_tariff(
        &self,
        tariff_id: Uuid,
        change_ref: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, TariffWriteError> {
        let change_ref = change_ref.trim();
        if change_ref.is_empty()
            || change_ref.len() > 160
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(TariffWriteError::InvalidFields);
        }
        let mut tx = self.pool.begin().await?;
        let lock_key = format!(
            "idempotency:{}:{}:catalog.tariff.revoke:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'catalog.tariff.revoke' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_digest: String = row.try_get("request_digest")?;
            if stored_digest != request_digest {
                return Err(TariffWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(CatalogWriteOutcome::Replay(response));
        }

        let row = sqlx::query(
            "SELECT service_code, customer_category, unit, unit_price, currency,
                    effective_from, effective_to, approval_ref, approved_by, status
             FROM tariff_versions WHERE id = $1 FOR UPDATE",
        )
        .bind(tariff_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(TariffWriteError::NotFound)?;
        let old_status: String = row.try_get("status")?;
        if !matches!(old_status.as_str(), "DRAFT" | "APPROVED" | "REVOKED") {
            return Err(TariffWriteError::NotDraft);
        }
        let price: Decimal = row.try_get("unit_price")?;
        let effective_from: NaiveDate = row.try_get("effective_from")?;
        let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
        let response = json!({
            "id": tariff_id,
            "service_code": row.try_get::<String, _>("service_code")?,
            "customer_category": row.try_get::<String, _>("customer_category")?,
            "unit": row.try_get::<String, _>("unit")?,
            "unit_price": price.normalize().to_string(),
            "currency": row.try_get::<String, _>("currency")?,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
            "approved_by": row.try_get::<Option<Uuid>, _>("approved_by")?,
            "status": "REVOKED"
        });
        if old_status != "REVOKED" {
            sqlx::query("UPDATE tariff_versions SET status = 'REVOKED' WHERE id = $1")
                .bind(tariff_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO audit_events
                 (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
                 VALUES ($1, $2, $3, 'TARIFF_VERSION_REVOKED', now(), $4)",
            )
            .bind(Uuid::new_v4())
            .bind(actor_id)
            .bind(tariff_id)
            .bind(json!({ "change_ref": change_ref }))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'catalog.tariff.revoke', $3, $4, 200, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn admin_catalog_tariffs(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, service_code, customer_category, unit, unit_price, currency,
                    effective_from, effective_to, approval_ref, status, approved_by
             FROM tariff_versions
             ORDER BY service_code, customer_category, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let unit_price: Decimal = row.try_get("unit_price")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "service_code": row.try_get::<String, _>("service_code")?,
                    "customer_category": row.try_get::<String, _>("customer_category")?,
                    "unit": row.try_get::<String, _>("unit")?,
                    "unit_price": unit_price.normalize().to_string(),
                    "currency": row.try_get::<String, _>("currency")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
                    "status": row.try_get::<String, _>("status")?,
                    "approved_by": row.try_get::<Option<Uuid>, _>("approved_by")?
                }))
            })
            .collect()
    }

    pub async fn create_solar_quote(
        &self,
        input: &CreateSolarQuoteRequest,
    ) -> Result<Option<SolarQuoteResponse>, SolarQuoteError> {
        let rows = sqlx::query(
            "SELECT price_reference FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT'
               AND currency = $2 AND active = TRUE AND status = 'APPROVED'
               AND effective_from <= CURRENT_DATE
               AND (effective_to IS NULL OR effective_to >= CURRENT_DATE)
             ORDER BY effective_from DESC, id LIMIT 2",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_all(&self.pool)
        .await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        if rows.len() != 1 {
            return Err(SolarQuoteError::ProductAmbiguous);
        }
        let subtotal: Decimal = row.try_get("price_reference")?;
        let taxes = sqlx::query(
            "SELECT id, base, rate FROM tax_profile_versions
             WHERE service_code = 'SOLAR_KIT' AND status = 'APPROVED'
               AND effective_from <= CURRENT_DATE
               AND (effective_to IS NULL OR effective_to >= CURRENT_DATE)
             ORDER BY effective_from DESC, jurisdiction, base, id LIMIT 2",
        )
        .fetch_all(&self.pool)
        .await?;
        if taxes.len() != 1 {
            return Err(SolarQuoteError::TaxProfileUnavailable);
        }
        let tax_rate: Decimal = taxes[0].try_get("rate")?;
        if !matches!(
            taxes[0].try_get::<String, _>("base")?.as_str(),
            "SUBTOTAL" | "SERVICE"
        ) {
            return Err(SolarQuoteError::TaxProfileUnavailable);
        }
        let subtotal = subtotal.round_dp(2);
        let tax_amount = (subtotal * tax_rate).round_dp(2);
        let principal = subtotal + tax_amount;
        Ok(Some(SolarQuoteResponse {
            product_code: input.product_code.trim().to_owned(),
            subtotal,
            tax_rate,
            tax_amount,
            total: principal,
            principal,
            currency: input.currency.clone(),
            terms_revision: input.terms_revision.clone(),
            status: "QUOTED_FROM_APPROVED_CATALOG".into(),
        }))
    }

    pub async fn catalog_tariffs(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, service_code, customer_category, unit, unit_price, currency,
                    effective_from, effective_to, approval_ref
             FROM tariff_versions
             WHERE status = 'APPROVED'
             ORDER BY service_code, customer_category, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let unit_price: Decimal = row.try_get("unit_price")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "service_code": row.try_get::<String, _>("service_code")?,
                    "customer_category": row.try_get::<String, _>("customer_category")?,
                    "unit": row.try_get::<String, _>("unit")?,
                    "unit_price": unit_price.normalize().to_string(),
                    "currency": row.try_get::<String, _>("currency")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<String, _>("approval_ref")?
                }))
            })
            .collect()
    }

    pub async fn catalog_tax_profiles(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, service_code, jurisdiction, base, rate, source,
                    effective_from, effective_to, approval_ref
             FROM tax_profile_versions
             WHERE status = 'APPROVED'
             ORDER BY service_code, jurisdiction, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let rate: Decimal = row.try_get("rate")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "service_code": row.try_get::<String, _>("service_code")?,
                    "jurisdiction": row.try_get::<String, _>("jurisdiction")?,
                    "base": row.try_get::<String, _>("base")?,
                    "rate": rate.normalize().to_string(),
                    "source": row.try_get::<String, _>("source")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?
                }))
            })
            .collect()
    }

    pub async fn catalog_fx_rates(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, from_currency, to_currency, value, effective_from, effective_to,
                    source, approval_ref
             FROM fx_rate_versions
             WHERE approved = TRUE AND status = 'APPROVED'
             ORDER BY from_currency, to_currency, effective_from DESC, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let value: Decimal = row.try_get("value")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "from_currency": row.try_get::<String, _>("from_currency")?,
                    "to_currency": row.try_get::<String, _>("to_currency")?,
                    "value": value.normalize().to_string(),
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "source": row.try_get::<String, _>("source")?,
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?
                }))
            })
            .collect()
    }

    pub async fn load_communication_charge(
        &self,
        currency: &str,
        on_date: NaiveDate,
    ) -> Result<Option<Decimal>, sqlx::Error> {
        let amount = sqlx::query_scalar::<_, String>(
            "SELECT payload->>'amount' FROM catalog_reference_versions
             WHERE reference_type = 'COMMUNICATION_CHARGE'
               AND natural_key = upper($1) AND status = 'APPROVED'
               AND effective_from <= $2 AND (effective_to IS NULL OR effective_to >= $2)
             ORDER BY effective_from DESC, id DESC LIMIT 1",
        )
        .bind(currency.trim())
        .bind(on_date)
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

    pub async fn catalog_communication_charges(
        &self,
        on_date: NaiveDate,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, payload, natural_key, effective_from, effective_to, approval_ref
             FROM catalog_reference_versions
             WHERE reference_type = 'COMMUNICATION_CHARGE' AND status = 'APPROVED'
               AND effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1)
             ORDER BY natural_key, effective_from DESC, id",
        )
        .bind(on_date)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let payload: serde_json::Value = row.try_get("payload")?;
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "label": payload["label"],
                    "amount": payload["amount"],
                    "currency": payload["currency"],
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?
                }))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_reference_draft(
        &self,
        reference_type: &str,
        natural_key: &str,
        payload: serde_json::Value,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        if !matches!(
            reference_type,
            "TAX_PROFILE" | "FX_RATE" | "COMMUNICATION_CHARGE"
        ) || natural_key.trim().is_empty()
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
            || effective_to.is_some_and(|date| date < effective_from)
        {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let operation = format!("catalog.{}.create", reference_type.to_ascii_lowercase());
        let mut tx = self.pool.begin().await?;
        let lock_key = format!(
            "idempotency:{}:{}:{operation}:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(&operation)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_digest")? != request_digest {
                return Err(ReferenceCatalogWriteError::IdempotencyPayloadMismatch);
            }
            return Ok(CatalogWriteOutcome::Replay(row.try_get("response_json")?));
        }
        let same_start = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM catalog_reference_versions
                WHERE reference_type = $1 AND natural_key = $2 AND effective_from = $3
                  AND status <> 'REVOKED'
            )",
        )
        .bind(reference_type)
        .bind(natural_key)
        .bind(effective_from)
        .fetch_one(&mut *tx)
        .await?;
        if same_start {
            return Err(ReferenceCatalogWriteError::PeriodOverlap);
        }
        let id = Uuid::new_v4();
        let response = json!({
            "id": id,
            "reference_type": reference_type,
            "natural_key": natural_key,
            "payload": payload,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": null,
            "status": "DRAFT"
        });
        sqlx::query(
            "INSERT INTO catalog_reference_versions
             (id, reference_type, natural_key, payload, effective_from,
              effective_to, status, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, 'DRAFT', $7)",
        )
        .bind(id)
        .bind(reference_type)
        .bind(natural_key)
        .bind(payload)
        .bind(effective_from)
        .bind(effective_to)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        let event_type = match reference_type {
            "TAX_PROFILE" => "TAX_PROFILE_VERSION_DRAFTED",
            "FX_RATE" => "FX_RATE_VERSION_DRAFTED",
            _ => "COMMUNICATION_CHARGE_VERSION_DRAFTED",
        };
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, $4, now(), $5)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(id)
        .bind(event_type)
        .bind(json!({ "reference_type": reference_type, "natural_key": natural_key }))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 201, $6)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(&operation)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_tax_profile_draft(
        &self,
        service_code: &str,
        jurisdiction: &str,
        base: &str,
        rate: Decimal,
        source: &str,
        change_reason: &str,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        let service_code = service_code.trim().to_ascii_uppercase();
        let jurisdiction = if jurisdiction.trim().is_empty() {
            "SEL".to_owned()
        } else {
            jurisdiction.trim().to_ascii_uppercase()
        };
        let base = if base.trim().is_empty() {
            "SERVICE".to_owned()
        } else {
            base.trim().to_ascii_uppercase()
        };
        let source = source.trim();
        let change_reason = change_reason.trim();
        let max_rate = Decimal::new(99_999_999_999_999, 0);
        if !matches!(service_code.as_str(), "ELECTRICITY" | "WATER" | "SOLAR_KIT")
            || jurisdiction.is_empty()
            || jurisdiction.len() > 64
            || base.is_empty()
            || base.len() > 64
            || source.is_empty()
            || source.len() > 160
            || change_reason.chars().count() > 500
            || rate < Decimal::ZERO
            || rate > max_rate
            || rate.normalize().scale() > 6
        {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let natural_key = format!("{service_code}|{jurisdiction}|{base}");
        let payload = json!({
            "service_code": service_code,
            "jurisdiction": jurisdiction,
            "base": base,
            "rate": rate.normalize().to_string(),
            "source": source,
            "change_reason": change_reason
        });
        self.create_reference_draft(
            "TAX_PROFILE",
            &natural_key,
            payload,
            effective_from,
            effective_to,
            actor_id,
            idempotency_key,
            request_digest,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_fx_rate_draft(
        &self,
        from_currency: &str,
        to_currency: &str,
        value: Decimal,
        source: &str,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        let from_currency = from_currency.trim().to_ascii_uppercase();
        let to_currency = to_currency.trim().to_ascii_uppercase();
        let source = source.trim();
        let max_value = Decimal::new(99_999_999_999_999_999, 9);
        if from_currency.len() != 3
            || to_currency.len() != 3
            || !from_currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || !to_currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || from_currency == to_currency
            || value <= Decimal::ZERO
            || value > max_value
            || value.normalize().scale() > 12
            || source.is_empty()
            || source.len() > 160
        {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let natural_key = format!("{from_currency}|{to_currency}");
        let payload = json!({
            "from_currency": from_currency,
            "to_currency": to_currency,
            "value": value.normalize().to_string(),
            "source": source
        });
        self.create_reference_draft(
            "FX_RATE",
            &natural_key,
            payload,
            effective_from,
            effective_to,
            actor_id,
            idempotency_key,
            request_digest,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_communication_charge_draft(
        &self,
        amount: Decimal,
        currency: &str,
        effective_from: NaiveDate,
        effective_to: Option<NaiveDate>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        let currency = currency.trim().to_ascii_uppercase();
        if currency.len() != 3
            || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || amount <= Decimal::ZERO
            || amount.normalize().scale() > 6
        {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let natural_key = currency.clone();
        let payload = json!({
            "label": "Ligne de communication",
            "amount": amount.normalize().to_string(),
            "currency": currency,
            "taxable": true
        });
        self.create_reference_draft(
            "COMMUNICATION_CHARGE",
            &natural_key,
            payload,
            effective_from,
            effective_to,
            actor_id,
            idempotency_key,
            request_digest,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_reference_draft(
        &self,
        reference_id: Uuid,
        expected_type: &str,
        approval_ref: &str,
        supersedes_id: Option<Uuid>,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        let approval_ref = approval_ref.trim();
        if !matches!(
            expected_type,
            "TAX_PROFILE" | "FX_RATE" | "COMMUNICATION_CHARGE"
        ) || approval_ref.is_empty()
            || approval_ref.len() > 160
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(ReferenceCatalogWriteError::ApprovalRefRequired);
        }
        let operation = "catalog.reference.approve";
        let mut tx = self.pool.begin().await?;
        if expected_type == "TAX_PROFILE" {
            sqlx::query("SELECT pg_advisory_xact_lock(1249681746, 1)")
                .execute(&mut *tx)
                .await?;
        }
        let lock_key = format!(
            "idempotency:{}:{}:{operation}:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(operation)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_digest")? != request_digest {
                return Err(ReferenceCatalogWriteError::IdempotencyPayloadMismatch);
            }
            return Ok(CatalogWriteOutcome::Replay(row.try_get("response_json")?));
        }
        let draft = sqlx::query(
            "SELECT reference_type, natural_key, payload, effective_from, effective_to, status
             FROM catalog_reference_versions WHERE id = $1 FOR UPDATE",
        )
        .bind(reference_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReferenceCatalogWriteError::NotFound)?;
        if draft.try_get::<String, _>("status")? != "DRAFT" {
            return Err(ReferenceCatalogWriteError::NotDraft);
        }
        let reference_type: String = draft.try_get("reference_type")?;
        if reference_type != expected_type {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let natural_key: String = draft.try_get("natural_key")?;
        let payload: serde_json::Value = draft.try_get("payload")?;
        let effective_from: NaiveDate = draft.try_get("effective_from")?;
        let effective_to: Option<NaiveDate> = draft.try_get("effective_to")?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("catalog-reference:{reference_type}:{natural_key}"))
            .execute(&mut *tx)
            .await?;

        if let Some(previous_id) = supersedes_id {
            if previous_id == reference_id {
                return Err(ReferenceCatalogWriteError::InvalidSupersededReference);
            }
            let previous = sqlx::query(
                "SELECT reference_type, natural_key, effective_from, effective_to, status
                 FROM catalog_reference_versions WHERE id = $1 FOR UPDATE",
            )
            .bind(previous_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(ReferenceCatalogWriteError::InvalidSupersededReference)?;
            if previous.try_get::<String, _>("status")? != "APPROVED"
                || previous.try_get::<String, _>("reference_type")? != reference_type
                || previous.try_get::<String, _>("natural_key")? != natural_key
                || previous.try_get::<NaiveDate, _>("effective_from")? >= effective_from
                || previous
                    .try_get::<Option<NaiveDate>, _>("effective_to")?
                    .is_some_and(|date| date < effective_from)
            {
                return Err(ReferenceCatalogWriteError::InvalidSupersededReference);
            }
        }
        let overlap = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM catalog_reference_versions
                WHERE reference_type = $1 AND natural_key = $2 AND status = 'APPROVED'
                  AND id <> $3 AND ($4::uuid IS NULL OR id <> $4)
                  AND ($6::date IS NULL OR effective_from <= $6)
                  AND (effective_to IS NULL OR effective_to >= $5)
            )",
        )
        .bind(&reference_type)
        .bind(&natural_key)
        .bind(reference_id)
        .bind(supersedes_id)
        .bind(effective_from)
        .bind(effective_to)
        .fetch_one(&mut *tx)
        .await?;
        if overlap {
            return Err(ReferenceCatalogWriteError::PeriodOverlap);
        }
        if let Some(previous_id) = supersedes_id {
            let prior_end = effective_from - chrono::Duration::days(1);
            sqlx::query(
                "UPDATE catalog_reference_versions
                 SET effective_to = $2 WHERE id = $1",
            )
            .bind(previous_id)
            .bind(prior_end)
            .execute(&mut *tx)
            .await?;
            if reference_type == "TAX_PROFILE" {
                sqlx::query("UPDATE tax_profile_versions SET effective_to = $2 WHERE id = $1")
                    .bind(previous_id)
                    .bind(prior_end)
                    .execute(&mut *tx)
                    .await?;
            } else if reference_type == "FX_RATE" {
                sqlx::query("UPDATE fx_rate_versions SET effective_to = $2 WHERE id = $1")
                    .bind(previous_id)
                    .bind(prior_end)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        sqlx::query(
            "UPDATE catalog_reference_versions
             SET status = 'APPROVED', approval_ref = $2, approved_by = $3
             WHERE id = $1",
        )
        .bind(reference_id)
        .bind(approval_ref)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        if reference_type == "TAX_PROFILE" {
            sqlx::query(
                "INSERT INTO tax_profile_versions
                 (id, service_code, jurisdiction, base, rate, source,
                  effective_from, effective_to, status, approval_ref, approved_by)
                 VALUES ($1, $2, $3, $4, $5::numeric, $6, $7, $8,
                         'APPROVED', $9, $10)",
            )
            .bind(reference_id)
            .bind(payload["service_code"].as_str().unwrap_or_default())
            .bind(payload["jurisdiction"].as_str().unwrap_or_default())
            .bind(payload["base"].as_str().unwrap_or_default())
            .bind(payload["rate"].as_str().unwrap_or_default())
            .bind(payload["source"].as_str().unwrap_or_default())
            .bind(effective_from)
            .bind(effective_to)
            .bind(approval_ref)
            .bind(actor_id)
            .execute(&mut *tx)
            .await?;
        } else if reference_type == "FX_RATE" {
            sqlx::query(
                "INSERT INTO fx_rate_versions
                 (id, from_currency, to_currency, value, effective_from, effective_to,
                  source, approved, approval_ref, approved_by, status)
                 VALUES ($1, $2, $3, $4::numeric, $5, $6, $7, TRUE, $8, $9, 'APPROVED')",
            )
            .bind(reference_id)
            .bind(payload["from_currency"].as_str().unwrap_or_default())
            .bind(payload["to_currency"].as_str().unwrap_or_default())
            .bind(payload["value"].as_str().unwrap_or_default())
            .bind(effective_from)
            .bind(effective_to)
            .bind(payload["source"].as_str().unwrap_or_default())
            .bind(approval_ref)
            .bind(actor_id)
            .execute(&mut *tx)
            .await?;
        }
        let event_type = match reference_type.as_str() {
            "TAX_PROFILE" => "TAX_PROFILE_VERSION_APPROVED",
            "FX_RATE" => "FX_RATE_VERSION_APPROVED",
            _ => "COMMUNICATION_CHARGE_VERSION_APPROVED",
        };
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
             VALUES ($1, $2, $3, $4, now(), $5)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(reference_id)
        .bind(event_type)
        .bind(json!({ "approval_ref": approval_ref, "supersedes_id": supersedes_id }))
        .execute(&mut *tx)
        .await?;
        let response = json!({
            "id": reference_id,
            "reference_type": reference_type,
            "natural_key": natural_key,
            "payload": payload,
            "effective_from": effective_from.to_string(),
            "effective_to": effective_to.map(|date| date.to_string()),
            "approval_ref": approval_ref,
            "status": "APPROVED"
        });
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 200, $6)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(operation)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn revoke_reference_version(
        &self,
        reference_id: Uuid,
        expected_type: &str,
        change_ref: &str,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CatalogWriteOutcome, ReferenceCatalogWriteError> {
        let change_ref = change_ref.trim();
        if !matches!(
            expected_type,
            "TAX_PROFILE" | "FX_RATE" | "COMMUNICATION_CHARGE"
        ) || change_ref.is_empty()
            || change_ref.len() > 160
            || idempotency_key.trim().is_empty()
            || request_digest.trim().is_empty()
        {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let operation = "catalog.reference.revoke";
        let mut tx = self.pool.begin().await?;
        if expected_type == "TAX_PROFILE" {
            sqlx::query("SELECT pg_advisory_xact_lock(1249681746, 1)")
                .execute(&mut *tx)
                .await?;
        }
        let lock_key = format!(
            "idempotency:{}:{}:{operation}:{idempotency_key}",
            self.organization_id, actor_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await?;
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = $3 AND idempotency_key = $4",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(operation)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_digest")? != request_digest {
                return Err(ReferenceCatalogWriteError::IdempotencyPayloadMismatch);
            }
            return Ok(CatalogWriteOutcome::Replay(row.try_get("response_json")?));
        }
        let row = sqlx::query(
            "SELECT reference_type, status FROM catalog_reference_versions
             WHERE id = $1 FOR UPDATE",
        )
        .bind(reference_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ReferenceCatalogWriteError::NotFound)?;
        let reference_type: String = row.try_get("reference_type")?;
        if reference_type != expected_type {
            return Err(ReferenceCatalogWriteError::InvalidFields);
        }
        let old_status: String = row.try_get("status")?;
        if old_status != "REVOKED" {
            sqlx::query("UPDATE catalog_reference_versions SET status = 'REVOKED' WHERE id = $1")
                .bind(reference_id)
                .execute(&mut *tx)
                .await?;
            if reference_type == "TAX_PROFILE" {
                sqlx::query("UPDATE tax_profile_versions SET status = 'REVOKED' WHERE id = $1")
                    .bind(reference_id)
                    .execute(&mut *tx)
                    .await?;
            } else if reference_type == "FX_RATE" {
                sqlx::query(
                    "UPDATE fx_rate_versions SET status = 'REVOKED', approved = FALSE WHERE id = $1",
                )
                .bind(reference_id)
                .execute(&mut *tx)
                .await?;
            }
            let event_type = match reference_type.as_str() {
                "TAX_PROFILE" => "TAX_PROFILE_VERSION_REVOKED",
                "FX_RATE" => "FX_RATE_VERSION_REVOKED",
                _ => "COMMUNICATION_CHARGE_VERSION_REVOKED",
            };
            sqlx::query(
                "INSERT INTO audit_events
                 (id, actor, subject_id, event_type, occurred_at, redacted_metadata)
                 VALUES ($1, $2, $3, $4, now(), $5)",
            )
            .bind(Uuid::new_v4())
            .bind(actor_id)
            .bind(reference_id)
            .bind(event_type)
            .bind(json!({ "change_ref": change_ref }))
            .execute(&mut *tx)
            .await?;
        }
        let response =
            json!({ "id": reference_id, "reference_type": reference_type, "status": "REVOKED" });
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, $3, $4, $5, 200, $6)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(operation)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CatalogWriteOutcome::Created(response))
    }

    pub async fn admin_reference_versions(
        &self,
        reference_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, reference_type, natural_key, payload, effective_from,
                    effective_to, status, approval_ref, approved_by
             FROM catalog_reference_versions WHERE reference_type = $1
             ORDER BY natural_key, effective_from DESC, id",
        )
        .bind(reference_type)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let effective_from: NaiveDate = row.try_get("effective_from")?;
                let effective_to: Option<NaiveDate> = row.try_get("effective_to")?;
                Ok(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "reference_type": row.try_get::<String, _>("reference_type")?,
                    "natural_key": row.try_get::<String, _>("natural_key")?,
                    "payload": row.try_get::<serde_json::Value, _>("payload")?,
                    "effective_from": effective_from.to_string(),
                    "effective_to": effective_to.map(|date| date.to_string()),
                    "approval_ref": row.try_get::<Option<String>, _>("approval_ref")?,
                    "approved_by": row.try_get::<Option<Uuid>, _>("approved_by")?,
                    "status": row.try_get::<String, _>("status")?
                }))
            })
            .collect()
    }

    pub async fn issue_utility_invoice(
        &self,
        input: &UtilityBillingRequest,
        response: &UtilityBillingResponse,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<UtilityIssueOutcome, UtilityIssueError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .issue_utility_invoice_in_tx(
                &mut tx,
                input,
                response,
                None,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    async fn issue_utility_invoice_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        input: &UtilityBillingRequest,
        response: &UtilityBillingResponse,
        identity_snapshot: Option<&InvoiceIdentitySnapshot>,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<UtilityIssueOutcome, UtilityIssueError> {
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
        if response.billing_period != input.billing_period || response.due_date != input.due_date {
            return Err(sqlx::Error::Protocol(
                "invoice document metadata does not match the billing request".into(),
            )
            .into());
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
        if let Some(refs) = &input.meter_readings {
            let quantities = load_invoice_meter_readings_in_tx(
                tx,
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
                return Err(UtilityIssueError::MeterReadingInvalid);
            }
        }
        let receivable_id = response
            .receivable_id
            .expect("issued utility response must contain a receivable id");
        let identity_snapshot = match identity_snapshot {
            Some(snapshot) => snapshot.clone(),
            None => {
                load_invoice_identity_snapshot(tx, self.organization_id, input.customer_id).await?
            }
        };
        let issued_at: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(&mut *tx)
            .await?;
        let mut issued_response = response.clone();
        issued_response.invoice_number = Some(issued_invoice_reference(issued_at));
        let document_snapshot = serde_json::to_value(&identity_snapshot)?;
        let snapshot = serde_json::to_value(&issued_response)?;
        let invoice_insert = sqlx::query(
            "INSERT INTO invoices
             (id, customer_id, period, invoice_number, issued_at, currency, total, template_version, immutable_snapshot, status, version, document_snapshot)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'CENTRAL_ACCEPTED', 1, $10)",
        )
        .bind(response.invoice_id)
        .bind(input.customer_id)
        .bind(&input.billing_period)
        .bind(&issued_response.invoice_number)
        .bind(issued_at)
        .bind(&issued_response.currency)
        .bind(issued_response.total)
        .bind("v2-a4-2")
        .bind(snapshot.clone())
        .bind(document_snapshot.clone())
        .execute(&mut *tx)
        .await;
        match invoice_insert {
            Ok(_) => {}
            Err(sqlx::Error::Database(database))
                if database.constraint() == Some("one_utility_invoice_period") =>
            {
                return Err(UtilityIssueError::InvoicePeriodExists);
            }
            Err(error) => return Err(error.into()),
        }
        for line in &issued_response.lines {
            sqlx::query(
                "INSERT INTO invoice_lines
                 (id, invoice_id, service_code, quantity, unit, unit_price, subtotal, tax_snapshot, total)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(Uuid::new_v4())
            .bind(issued_response.invoice_id)
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
        if let Some(refs) = &input.meter_readings {
            sqlx::query(
                "INSERT INTO invoice_meter_readings (invoice_id, meter_reading_id, service_code)
                 VALUES ($1, $2, 'ELECTRICITY'), ($1, $3, 'WATER')",
            )
            .bind(issued_response.invoice_id)
            .bind(refs.electricity_reading_id)
            .bind(refs.water_reading_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if error
                    .as_database_error()
                    .is_some_and(|db| db.is_unique_violation())
                {
                    UtilityIssueError::MeterReadingInvalid
                } else {
                    UtilityIssueError::Database(error)
                }
            })?;
        }
        sqlx::query(
            "INSERT INTO receivables
             (id, source_type, source_id, customer_id, currency, principal_total, status)
             VALUES ($1, 'INVOICE', $2, $3, $4, $5, 'OPEN')",
        )
        .bind(receivable_id)
        .bind(issued_response.invoice_id)
        .bind(input.customer_id)
        .bind(&issued_response.currency)
        .bind(issued_response.total)
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
        Ok(UtilityIssueOutcome::Created(issued_response))
    }

    pub async fn get_invoice_response(
        &self,
        id: Uuid,
    ) -> Result<Option<UtilityBillingResponse>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT immutable_snapshot, period, invoice_number, status FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let snapshot: serde_json::Value = row.try_get("immutable_snapshot")?;
            let document = snapshot.get("v2_document").cloned().unwrap_or(snapshot);
            let mut response: UtilityBillingResponse =
                serde_json::from_value(document).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
                })?;
            if response.billing_period.is_empty() {
                response.billing_period = row.try_get("period")?;
            }
            response.invoice_number = row.try_get("invoice_number")?;
            response.status = row.try_get("status")?;
            Ok(response)
        })
        .transpose()
    }

    pub async fn get_invoice_identity_snapshot(
        &self,
        id: Uuid,
    ) -> Result<Option<InvoiceIdentitySnapshot>, sqlx::Error> {
        let snapshot: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT document_snapshot FROM invoices
             WHERE id = $1 AND customer_id IN
               (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        snapshot
            .map(|value| {
                serde_json::from_value(value).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice identity snapshot: {error}"))
                })
            })
            .transpose()
    }

    pub async fn get_invoice_issued_at(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT to_char(issued_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')
             FROM invoices WHERE id = $1 AND customer_id IN
             (SELECT id FROM customers WHERE organization_id = $2)",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_invoice_responses(
        &self,
        customer_id: Option<Uuid>,
        period: Option<&str>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT i.id, i.invoice_number, i.period, i.issued_at, i.status,
                    i.immutable_snapshot
             FROM invoices i
             JOIN customers c ON c.id = i.customer_id
             WHERE c.organization_id = $1
               AND ($2::uuid IS NULL OR i.customer_id = $2)
               AND ($3::text IS NULL OR i.period = $3)
               AND ($4::text IS NULL OR i.status = $4)
             ORDER BY i.issued_at DESC, i.id DESC
             LIMIT $5",
        )
        .bind(self.organization_id)
        .bind(customer_id)
        .bind(period)
        .bind(status)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let snapshot: serde_json::Value = row.try_get("immutable_snapshot")?;
                let document = snapshot.get("v2_document").cloned().unwrap_or(snapshot);
                let mut response: UtilityBillingResponse = serde_json::from_value(document)
                    .map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid invoice snapshot: {error}"))
                    })?;
                response.status = row.try_get("status")?;
                response.invoice_number = row.try_get("invoice_number")?;
                if response.billing_period.is_empty() {
                    response.billing_period = row.try_get::<String, _>("period")?;
                }
                let mut value = serde_json::to_value(response).map_err(|error| {
                    sqlx::Error::Protocol(format!("invalid invoice JSON: {error}"))
                })?;
                let object = value.as_object_mut().ok_or_else(|| {
                    sqlx::Error::Protocol("invoice response is not a JSON object".into())
                })?;
                object.insert(
                    "period".into(),
                    serde_json::Value::from(row.try_get::<Option<String>, _>("period")?),
                );
                let issued_at: DateTime<Utc> = row.try_get("issued_at")?;
                object.insert(
                    "issued_at".into(),
                    serde_json::Value::String(issued_at.to_rfc3339()),
                );
                Ok(value)
            })
            .collect()
    }

    pub async fn list_payment_responses(
        &self,
        customer_id: Option<Uuid>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT p.id, p.document_reference, p.customer_id, p.amount_received, p.currency,
                    p.method, p.paid_at, p.status
             FROM payments p
             JOIN customers c ON c.id = p.customer_id
             WHERE c.organization_id = $1
               AND ($2::uuid IS NULL OR p.customer_id = $2)
               AND ($3::text IS NULL OR p.status = $3)
             ORDER BY p.paid_at DESC, p.id DESC
             LIMIT $4",
        )
        .bind(self.organization_id)
        .bind(customer_id)
        .bind(status)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let amount: Decimal = row.try_get("amount_received")?;
                let paid_at: DateTime<Utc> = row.try_get("paid_at")?;
                let payment_id: Uuid = row.try_get("id")?;
                let document_reference: Option<String> = row.try_get("document_reference")?;
                Ok(json!({
                    "payment_id": payment_id,
                    "payment_number": document_reference.unwrap_or_else(|| payment_id.to_string()),
                    "customer_id": row.try_get::<Uuid, _>("customer_id")?,
                    "amount_received": format!("{:.2}", amount.round_dp(2)),
                    "currency": row.try_get::<String, _>("currency")?,
                    "method": row.try_get::<String, _>("method")?,
                    "paid_at": paid_at.to_rfc3339(),
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
            "SELECT p.id, p.document_reference, p.issued_at, p.receipt_snapshot, p.customer_id, p.amount_received, p.currency, p.method,
                    p.paid_at, p.status, c.customer_code, c.legal_name, o.legal_name AS organization_name,
                    c.contact_fields::text AS contact_fields_json
             FROM payments p
             JOIN customers c ON c.id = p.customer_id
             JOIN organizations o ON o.id = c.organization_id
             WHERE p.id = $1 AND c.organization_id = $2",
        )
        .bind(payment_id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(payment) = payment else {
            return Ok(None);
        };
        let contact_fields: serde_json::Value =
            serde_json::from_str(&payment.try_get::<String, _>("contact_fields_json")?).map_err(
                |error| sqlx::Error::Protocol(format!("invalid customer contact fields: {error}")),
            )?;

        let allocation_rows = sqlx::query(
            "SELECT pa.receivable_id, pa.schedule_id, pa.allocated_amount, pa.currency,
                    CASE WHEN r.source_type = 'INVOICE' THEN COALESCE(i.invoice_number::text, r.source_id::text)
                         WHEN r.source_type = 'SALE' THEN COALESCE(s.document_reference, r.source_id::text)
                         ELSE r.source_id::text END AS source_reference,
                    COALESCE((SELECT string_agg(DISTINCT p.sku, ', ' ORDER BY p.sku)
                              FROM sale_lines sl JOIN products p ON p.id = sl.product_id
                              WHERE sl.sale_id = r.source_id AND r.source_type = 'SALE'), '') AS source_description
             FROM payment_allocations pa
             JOIN receivables r ON r.id = pa.receivable_id
             LEFT JOIN invoices i ON i.id = r.source_id AND r.source_type = 'INVOICE'
             LEFT JOIN sales s ON s.id = r.source_id AND r.source_type = 'SALE'
             WHERE pa.payment_id = $1 ORDER BY pa.id",
        )
        .bind(payment_id)
        .fetch_all(&self.pool)
        .await?;
        let allocations = allocation_rows
            .into_iter()
            .map(|row| {
                let amount: Decimal = row.try_get("allocated_amount")?;
                Ok(json!({
                    "receivable_id": row.try_get::<Uuid, _>("receivable_id")?,
                    "schedule_id": row.try_get::<Option<Uuid>, _>("schedule_id")?,
                    "amount": format!("{:.2}", amount.round_dp(2)),
                    "currency": row.try_get::<String, _>("currency")?,
                    "source_reference": row.try_get::<String, _>("source_reference")?,
                    "source_description": row.try_get::<String, _>("source_description")?
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let reversal_rows = sqlx::query(
            "SELECT amount, reason, created_at FROM payment_reversals
             WHERE payment_id = $1 ORDER BY created_at, id",
        )
        .bind(payment_id)
        .fetch_all(&self.pool)
        .await?;
        let reversals = reversal_rows
            .into_iter()
            .map(|row| {
                let amount: Decimal = row.try_get("amount")?;
                let created_at: DateTime<Utc> = row.try_get("created_at")?;
                Ok(json!({
                    "amount": format!("{:.2}", amount.round_dp(2)),
                    "reason": row.try_get::<String, _>("reason")?,
                    "created_at": created_at.to_rfc3339()
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let customer_id: Uuid = payment.try_get("customer_id")?;
        let amount: Decimal = payment.try_get("amount_received")?;
        let paid_at: DateTime<Utc> = payment.try_get("paid_at")?;
        let issued_at: Option<DateTime<Utc>> = payment.try_get("issued_at")?;
        let document_reference: Option<String> = payment.try_get("document_reference")?;
        let organization_name: String = payment.try_get("organization_name")?;
        let currency: String = payment.try_get("currency")?;
        let method: String = payment.try_get("method")?;
        let status: String = payment.try_get("status")?;
        let customer_code: String = payment.try_get("customer_code")?;
        let customer_name: String = payment.try_get("legal_name")?;
        let snapshot: Option<serde_json::Value> = payment.try_get("receipt_snapshot")?;
        let mut document = snapshot.unwrap_or_else(|| json!({
            "document_schema_version": 1,
            "document_type": "PAYMENT_RECEIPT",
            "document_reference": document_reference.unwrap_or_else(|| format!("SEL-PAY-{}", payment_id.simple().to_string().to_uppercase())),
            "qr_internal_reference": format!("SEL:PAYMENT:{payment_id}"),
            "organization_name": organization_name,
            "payment": {
                "payment_id": payment_id,
                "customer_id": customer_id,
                "amount_received": format!("{:.2}", amount.round_dp(2)),
                "currency": currency,
                "method": method,
                "paid_at": paid_at.to_rfc3339(),
                "issued_at": issued_at.map(|value| value.to_rfc3339()),
                "status": status
            },
            "customer": {
                "customer_id": customer_id,
                "customer_code": customer_code,
                "legal_name": customer_name,
                "address": contact_fields.get("address").cloned().unwrap_or(serde_json::Value::Null),
                "phone": contact_fields.get("phone").cloned().unwrap_or(serde_json::Value::Null)
            },
            "allocations": allocations
        }));
        if let Some(object) = document.as_object_mut() {
            object.insert("reversals".to_owned(), serde_json::Value::Array(reversals));
            object.insert("origin".to_owned(), json!("CENTRAL"));
        }
        Ok(Some(document))
    }

    pub async fn get_receivable_summary(
        &self,
        id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT r.id, r.customer_id, r.currency, r.principal_total, r.status, r.source_type,
                    CASE WHEN r.source_type = 'INVOICE' THEN COALESCE((SELECT i.invoice_number::text FROM invoices i WHERE i.id = r.source_id), r.source_id::text)
                         WHEN r.source_type = 'SALE' THEN COALESCE((SELECT s.document_reference FROM sales s WHERE s.id = r.source_id), r.source_id::text)
                         ELSE r.source_id::text END AS source_reference,
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
                    ), 0) AS paid,
                    COALESCE((SELECT SUM(amount) FROM credit_notes WHERE receivable_id = r.id), 0) AS credited
             FROM receivables r
             WHERE r.id = $1 AND r.customer_id IN
                 (SELECT id FROM customers WHERE organization_id = $2)
             GROUP BY r.id, r.customer_id, r.currency, r.principal_total, r.status",
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
        let credited: Decimal = row.try_get("credited")?;
        let principal = principal.round_dp(2);
        let mut paid = paid.round_dp(2);
        paid.rescale(2);
        let credited = credited.round_dp(2);
        let balance = (principal - paid - credited).round_dp(2);
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
             ), 0), 0)), 0)
             FROM installment_schedule s
             JOIN installment_plans ip ON ip.id = s.plan_id
             WHERE ip.receivable_id = $1 AND s.due_date < CURRENT_DATE",
        )
        .bind(receivable_id)
        .fetch_one(&self.pool)
        .await?;
        let overdue = overdue.round_dp(2);
        let stored_status: String = row.try_get("status")?;
        let status = if paid + credited >= principal {
            "SETTLED"
        } else if stored_status == "CREDITED" {
            "CREDITED"
        } else if overdue > Decimal::ZERO {
            "OVERDUE"
        } else {
            "OPEN"
        };
        Ok(Some(json!({
            "id": receivable_id,
            "customer_id": row.try_get::<Uuid, _>("customer_id")?,
            "source_type": row.try_get::<String, _>("source_type")?,
            "source_reference": row.try_get::<String, _>("source_reference")?,
            "currency": row.try_get::<String, _>("currency")?,
            "principal": format!("{principal:.2}"),
            "paid": format!("{paid:.2}"),
            "credited": format!("{credited:.2}"),
            "balance": format!("{balance:.2}"),
            "overdue_amount": format!("{overdue:.2}"),
            "overdue": overdue > Decimal::ZERO,
            "status": status
        })))
    }

    /// Organization-wide receivables register with balances and counters derived by PostgreSQL.
    pub async fn list_receivables(&self) -> Result<serde_json::Value, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let rows = sqlx::query(
            "SELECT r.id, r.customer_id, c.customer_code, c.legal_name,
                    r.source_type, r.currency, r.principal_total,
                    CASE WHEN r.source_type = 'INVOICE' THEN COALESCE(i.invoice_number, r.source_id::text)
                         WHEN r.source_type = 'SALE' THEN COALESCE((SELECT s.document_reference FROM sales s WHERE s.id = r.source_id), r.source_id::text)
                         ELSE r.source_id::text END AS source_reference,
                    COALESCE((
                        SELECT SUM(pa.allocated_amount * GREATEST(
                            p.amount_received - COALESCE(rev.reversed_amount, 0), 0
                        ) / NULLIF(p.amount_received, 0))
                        FROM payment_allocations pa
                        JOIN payments p ON p.id = pa.payment_id
                        LEFT JOIN (
                            SELECT payment_id, SUM(amount) AS reversed_amount
                            FROM payment_reversals GROUP BY payment_id
                        ) rev ON rev.payment_id = p.id
                        WHERE pa.receivable_id = r.id AND p.status = 'CENTRAL_ACCEPTED'
                    ), 0) AS paid,
                    COALESCE((SELECT SUM(cn.amount) FROM credit_notes cn WHERE cn.receivable_id = r.id), 0) AS credited,
                    COALESCE((
                        SELECT SUM(GREATEST(s.amount_due - COALESCE((
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
                        ), 0), 0))
                        FROM installment_plans ip
                        JOIN installment_schedule s ON s.plan_id = ip.id
                        WHERE ip.receivable_id = r.id AND s.due_date < CURRENT_DATE
                    ), 0) AS overdue_amount,
                    (SELECT COUNT(*) FROM installment_plans ip
                     JOIN installment_schedule s ON s.plan_id = ip.id
                     WHERE ip.receivable_id = r.id AND s.amount_due > COALESCE((
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
                     ), 0)) AS open_installments
             FROM receivables r
             JOIN customers c ON c.id = r.customer_id
             LEFT JOIN invoices i ON i.id = r.source_id AND r.source_type = 'INVOICE'
             WHERE c.organization_id = $1 AND r.status <> 'VOID'
             ORDER BY c.customer_code, r.source_type, r.source_id, r.id",
        )
        .bind(self.organization_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut receivables = Vec::with_capacity(rows.len());
        let mut open_installments = 0_i64;
        let mut overdue_count = 0_i64;
        for row in &rows {
            let principal = row.try_get::<Decimal, _>("principal_total")?.round_dp(2);
            let paid = row.try_get::<Decimal, _>("paid")?.round_dp(2);
            let credited = row.try_get::<Decimal, _>("credited")?.round_dp(2);
            let balance = (principal - paid - credited).round_dp(2);
            let overdue = row.try_get::<Decimal, _>("overdue_amount")?.round_dp(2);
            let status = if balance <= Decimal::ZERO {
                "SETTLED"
            } else if overdue > Decimal::ZERO {
                overdue_count += 1;
                "OVERDUE"
            } else {
                "OPEN"
            };
            open_installments += row.try_get::<i64, _>("open_installments")?;
            receivables.push(json!({
                "receivable_id": row.try_get::<Uuid, _>("id")?,
                "customer_id": row.try_get::<Uuid, _>("customer_id")?,
                "customer_code": row.try_get::<String, _>("customer_code")?,
                "customer_name": row.try_get::<String, _>("legal_name")?,
                "source_type": row.try_get::<String, _>("source_type")?,
                "source_reference": row.try_get::<String, _>("source_reference")?,
                "currency": row.try_get::<String, _>("currency")?,
                "principal": format!("{principal:.2}"),
                "paid": format!("{paid:.2}"),
                "credited": format!("{credited:.2}"),
                "balance": format!("{balance:.2}"),
                "overdue_amount": format!("{overdue:.2}"),
                "status": status
            }));
        }
        let payment_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM payments p
             JOIN customers c ON c.id = p.customer_id
             WHERE c.organization_id = $1 AND p.status = 'CENTRAL_ACCEPTED'",
        )
        .bind(self.organization_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(json!({
            "receivables": receivables,
            "metrics": {
                "payment_count": payment_count,
                "receivable_count": receivables.len(),
                "open_installment_count": open_installments,
                "overdue_count": overdue_count
            }
        }))
    }

    pub async fn get_installment_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let plan = sqlx::query(
            "SELECT ip.id, ip.receivable_id, ip.contract_id, ip.principal_amount,
                    ip.currency, ip.start_date, ip.status, ip.terms_snapshot,
                    r.customer_id
             FROM installment_plans ip
             JOIN receivables r ON r.id = ip.receivable_id
             JOIN customers c ON c.id = r.customer_id
             WHERE ip.id = $1 AND c.organization_id = $2",
        )
        .bind(plan_id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(plan) = plan else {
            return Ok(None);
        };

        let receivable_id: Uuid = plan.try_get("receivable_id")?;
        let rows = sqlx::query(
            "SELECT s.id, s.sequence, s.due_date, s.amount_due, s.schedule_status,
                    COALESCE((
                        SELECT SUM(
                            pa.allocated_amount * GREATEST(
                                p.amount_received - COALESCE((
                                    SELECT SUM(pr.amount)
                                    FROM payment_reversals pr
                                    WHERE pr.payment_id = p.id
                                ), 0),
                                0
                            ) / NULLIF(p.amount_received, 0)
                        )
                        FROM payment_allocations pa
                        JOIN payments p ON p.id = pa.payment_id
                        WHERE pa.schedule_id = s.id
                          AND pa.receivable_id = $2
                          AND p.status = 'CENTRAL_ACCEPTED'
                    ), 0) AS paid
             FROM installment_schedule s
             WHERE s.plan_id = $1
             ORDER BY s.sequence",
        )
        .bind(plan_id)
        .bind(receivable_id)
        .fetch_all(&self.pool)
        .await?;

        let today = Utc::now().date_naive();
        let mut total_paid = Decimal::ZERO;
        let mut total_overdue = Decimal::ZERO;
        let schedule = rows
            .into_iter()
            .map(|row| {
                let due_date: NaiveDate = row.try_get("due_date")?;
                let amount_due: Decimal = row.try_get("amount_due")?;
                let paid: Decimal = row.try_get("paid")?;
                let amount_due = amount_due.round_dp(2);
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
                Ok::<_, sqlx::Error>(json!({
                    "id": row.try_get::<Uuid, _>("id")?,
                    "sequence": row.try_get::<i32, _>("sequence")?,
                    "due_date": due_date.to_string(),
                    "amount_due": format!("{amount_due:.2}"),
                    "paid": format!("{paid:.2}"),
                    "remaining": format!("{remaining:.2}"),
                    "stored_status": row.try_get::<String, _>("schedule_status")?,
                    "status": status,
                    "overdue": status == "OVERDUE"
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let principal: Decimal = plan.try_get("principal_amount")?;
        let total_paid = total_paid.round_dp(2);
        let remaining = (principal - total_paid).round_dp(2).max(Decimal::ZERO);
        let plan_status: String = plan.try_get("status")?;

        Ok(Some(json!({
            "id": plan.try_get::<Uuid, _>("id")?,
            "receivable_id": receivable_id,
            "contract_id": plan.try_get::<Uuid, _>("contract_id")?,
            "customer_id": plan.try_get::<Uuid, _>("customer_id")?,
            "principal": format!("{:.2}", principal.round_dp(2)),
            "paid": format!("{total_paid:.2}"),
            "remaining": format!("{remaining:.2}"),
            "overdue_amount": format!("{:.2}", total_overdue.round_dp(2)),
            "currency": plan.try_get::<String, _>("currency")?,
            "start_date": plan.try_get::<NaiveDate, _>("start_date")?.to_string(),
            "stored_status": plan_status,
            "terms_snapshot": plan.try_get::<serde_json::Value, _>("terms_snapshot")?,
            "schedule": schedule
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
        let outcome = self
            .void_invoice_with_id_in_tx(
                &mut tx,
                invoice_id,
                void_id,
                actor_id,
                reason,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    async fn void_invoice_with_id_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        invoice_id: Uuid,
        void_id: Uuid,
        actor_id: Uuid,
        reason: &str,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<InvoiceVoidOutcome, InvoiceVoidError> {
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
        let outcome = self
            .record_payment_with_id_in_tx(
                &mut tx,
                input,
                payment_id,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    async fn record_payment_with_id_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        input: &RecordPaymentRequest,
        payment_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentWriteOutcome, PaymentWriteError> {
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
        let mut receipt_allocations = Vec::with_capacity(grouped_allocations.len());
        for (receivable_id, allocated_amount) in &grouped_allocations {
            let row = sqlx::query(
                "SELECT r.principal_total, r.currency, r.customer_id, r.source_type,
                        CASE WHEN r.source_type = 'INVOICE' THEN COALESCE(i.invoice_number::text, r.source_id::text)
                             WHEN r.source_type = 'SALE' THEN COALESCE(s.document_reference, r.source_id::text)
                             ELSE r.source_id::text END AS source_reference,
                        COALESCE((SELECT string_agg(DISTINCT p.sku, ', ' ORDER BY p.sku)
                                  FROM sale_lines sl JOIN products p ON p.id = sl.product_id
                                  WHERE sl.sale_id = r.source_id AND r.source_type = 'SALE'), '') AS source_description,
                        COALESCE((SELECT sl.tax_snapshot FROM sale_lines sl
                                   WHERE sl.sale_id = r.source_id AND r.source_type = 'SALE'
                                   ORDER BY sl.id LIMIT 1), '{}'::jsonb) AS source_tax_snapshot
                 FROM receivables r
                 LEFT JOIN invoices i ON i.id = r.source_id AND r.source_type = 'INVOICE'
                 LEFT JOIN sales s ON s.id = r.source_id AND r.source_type = 'SALE'
                 WHERE r.id = $1 AND r.customer_id = $2
                   AND r.customer_id IN (SELECT id FROM customers WHERE organization_id = $3)
                 FOR UPDATE OF r",
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
                "SELECT COALESCE(SUM(
                     pa.allocated_amount * GREATEST(p.amount_received - COALESCE(rev.reversed_amount, 0), 0)
                     / NULLIF(p.amount_received, 0)
                 ), 0)
                 FROM payment_allocations pa
                 JOIN payments p ON p.id = pa.payment_id
                 LEFT JOIN (
                     SELECT payment_id, SUM(amount) AS reversed_amount
                     FROM payment_reversals GROUP BY payment_id
                 ) rev ON rev.payment_id = p.id
                 WHERE pa.receivable_id = $1 AND p.status = 'CENTRAL_ACCEPTED'",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            let credited: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount), 0) FROM credit_notes WHERE receivable_id = $1",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            if paid + credited + allocated_amount > principal {
                return Err(PaymentWriteError::Overallocated);
            }
            let has_installment_plan = receivable_has_installment_plan(tx, *receivable_id).await?;
            if has_installment_plan && persisted_allocations.contains_key(&(*receivable_id, None)) {
                return Err(PaymentWriteError::InstallmentRequired);
            }
            for ((_allocation_receivable_id, schedule_id), _) in persisted_allocations
                .iter()
                .filter(|((allocation_receivable_id, _), _)| {
                    allocation_receivable_id == receivable_id
                })
            {
                if let Some(schedule_id) = schedule_id {
                    match validate_installment_allocation(
                        tx,
                        *schedule_id,
                        *receivable_id,
                        *persisted_allocations
                            .get(&(*receivable_id, Some(*schedule_id)))
                            .expect("grouped schedule allocation exists"),
                    )
                    .await?
                    {
                        None => return Err(PaymentWriteError::ContextInvalid),
                        Some(false) => {}
                        Some(true) => return Err(PaymentWriteError::InstallmentOverallocated),
                    }
                }
            }
            let balance_before = (principal - paid - credited).round_dp(2);
            let balance_after = (balance_before - allocated_amount).round_dp(2);
            let mut schedule_allocations = Vec::new();
            for ((allocation_receivable_id, schedule_id), schedule_amount) in &persisted_allocations
            {
                if allocation_receivable_id != receivable_id {
                    continue;
                }
                let Some(schedule_id) = schedule_id else {
                    continue;
                };
                let schedule = sqlx::query(
                    "SELECT s.sequence, s.due_date
                     FROM installment_schedule s
                     JOIN installment_plans ip ON ip.id = s.plan_id
                     WHERE s.id = $1 AND ip.receivable_id = $2",
                )
                .bind(schedule_id)
                .bind(receivable_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(PaymentWriteError::ContextInvalid)?;
                schedule_allocations.push(json!({
                    "sequence": schedule.try_get::<i32, _>("sequence")?,
                    "due_date": schedule.try_get::<NaiveDate, _>("due_date")?.to_string(),
                    "amount": format!("{:.2}", schedule_amount.round_dp(2))
                }));
            }
            receipt_allocations.push(json!({
                "receivable_id": receivable_id,
                "source_type": row.try_get::<String, _>("source_type")?,
                "source_reference": row.try_get::<String, _>("source_reference")?,
                "source_description": row.try_get::<String, _>("source_description")?,
                "tax_snapshot": row.try_get::<serde_json::Value, _>("source_tax_snapshot")?,
                "currency": currency,
                "principal": format!("{:.2}", principal.round_dp(2)),
                "paid_before": format!("{:.2}", paid.round_dp(2)),
                "credited_before": format!("{:.2}", credited.round_dp(2)),
                "balance_before": format!("{balance_before:.2}"),
                "amount": format!("{:.2}", allocated_amount.round_dp(2)),
                "balance_after": format!("{balance_after:.2}"),
                "has_installment_plan": has_installment_plan,
                "schedule_allocations": schedule_allocations
            }));
        }
        let issued_at: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(&mut *tx)
            .await?;
        let document_reference = issued_document_reference(issued_at, payment_id);
        let customer = sqlx::query(
            "SELECT c.customer_code, c.legal_name, c.contact_fields::text AS contact_fields_json,
                    o.legal_name AS organization_name
             FROM customers c JOIN organizations o ON o.id = c.organization_id
             WHERE c.id = $1 AND c.organization_id = $2",
        )
        .bind(input.customer_id)
        .bind(self.organization_id)
        .fetch_one(&mut *tx)
        .await?;
        let contact_fields: serde_json::Value =
            serde_json::from_str(&customer.try_get::<String, _>("contact_fields_json")?).map_err(
                |error| sqlx::Error::Protocol(format!("invalid customer contact fields: {error}")),
            )?;
        let receipt_snapshot = json!({
            "document_schema_version": 2,
            "document_type": "PAYMENT_RECEIPT",
            "document_reference": document_reference,
            "qr_internal_reference": format!("SEL:PAYMENT:{payment_id}"),
            "organization_name": customer.try_get::<String, _>("organization_name")?,
            "issued_at": issued_at.to_rfc3339(),
            "payment": {
                "payment_id": payment_id,
                "customer_id": input.customer_id,
                "amount_received": format!("{:.2}", input.received_amount.round_dp(2)),
                "amount_allocated": format!("{:.2}", allocated_total.round_dp(2)),
                "amount_unallocated": format!("{:.2}", (input.received_amount - allocated_total).round_dp(2)),
                "currency": input.currency,
                "method": input.method,
                "paid_at": input.occurred_at.to_rfc3339(),
                "issued_at": issued_at.to_rfc3339(),
                "status": "CENTRAL_ACCEPTED"
            },
            "customer": {
                "customer_id": input.customer_id,
                "customer_code": customer.try_get::<String, _>("customer_code")?,
                "legal_name": customer.try_get::<String, _>("legal_name")?,
                "address": contact_fields.get("address").cloned().unwrap_or(serde_json::Value::Null),
                "phone": contact_fields.get("phone").cloned().unwrap_or(serde_json::Value::Null)
            },
            "allocations": receipt_allocations
        });
        sqlx::query(
            "INSERT INTO payments
             (id, customer_id, amount_received, currency, method, paid_at, issued_at, status, document_reference, receipt_snapshot)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'CENTRAL_ACCEPTED', $8, $9)",
        )
        .bind(payment_id)
        .bind(input.customer_id)
        .bind(input.received_amount)
        .bind(&input.currency)
        .bind(&input.method)
        .bind(input.occurred_at)
        .bind(issued_at)
        .bind(&document_reference)
        .bind(&receipt_snapshot)
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
            "payment_number": document_reference,
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
        Ok(PaymentWriteOutcome::Created(body))
    }

    pub async fn allocate_payment(
        &self,
        payment_id: Uuid,
        allocations: &[sel_contracts::PaymentAllocationRequest],
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentAllocationWriteOutcome, PaymentAllocationWriteError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .allocate_payment_in_tx(
                &mut tx,
                payment_id,
                allocations,
                actor_id,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    async fn allocate_payment_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        payment_id: Uuid,
        allocations: &[sel_contracts::PaymentAllocationRequest],
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentAllocationWriteOutcome, PaymentAllocationWriteError> {
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'payment.allocate' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(PaymentAllocationWriteError::IdempotencyPayloadMismatch);
            }
            let original: serde_json::Value = row.try_get("response_json")?;
            return Ok(PaymentAllocationWriteOutcome::Replay(
                serde_json::from_value(original)?,
            ));
        }
        if allocations.is_empty() || allocations.iter().any(|item| item.amount <= Decimal::ZERO) {
            return Err(PaymentAllocationWriteError::InvalidAllocation);
        }

        let mut grouped_allocations = BTreeMap::<Uuid, Decimal>::new();
        let mut persisted_allocations = BTreeMap::<(Uuid, Option<Uuid>), Decimal>::new();
        for allocation in allocations {
            *grouped_allocations
                .entry(allocation.receivable_id)
                .or_insert(Decimal::ZERO) += allocation.amount;
            *persisted_allocations
                .entry((allocation.receivable_id, allocation.schedule_id))
                .or_insert(Decimal::ZERO) += allocation.amount;
        }
        let requested_total: Decimal = grouped_allocations.values().copied().sum();

        let Some(payment) = sqlx::query(
            "SELECT p.amount_received, p.currency, p.customer_id, p.status
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
            return Err(PaymentAllocationWriteError::PaymentNotFound);
        };
        let payment_status: String = payment.try_get("status")?;
        if payment_status != "CENTRAL_ACCEPTED" {
            return Err(PaymentAllocationWriteError::PaymentNotAccepted);
        }
        let amount_received: Decimal = payment.try_get("amount_received")?;
        let payment_currency: String = payment.try_get("currency")?;
        let payment_customer_id: Uuid = payment.try_get("customer_id")?;
        let prior_payment_allocated: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(allocated_amount), 0)
             FROM payment_allocations WHERE payment_id = $1",
        )
        .bind(payment_id)
        .fetch_one(&mut *tx)
        .await?;
        if prior_payment_allocated + requested_total > amount_received {
            return Err(PaymentAllocationWriteError::AllocationExceedsPayment);
        }

        for (receivable_id, allocated_amount) in &grouped_allocations {
            let Some(receivable) = sqlx::query(
                "SELECT r.principal_total, r.currency, r.customer_id
                 FROM receivables r
                 JOIN customers c ON c.id = r.customer_id
                 WHERE r.id = $1 AND r.customer_id = $2
                   AND c.organization_id = $3
                 FOR UPDATE OF r",
            )
            .bind(receivable_id)
            .bind(payment_customer_id)
            .bind(self.organization_id)
            .fetch_optional(&mut *tx)
            .await?
            else {
                return Err(PaymentAllocationWriteError::ContextInvalid);
            };
            let currency: String = receivable.try_get("currency")?;
            if currency != payment_currency {
                return Err(PaymentAllocationWriteError::ContextInvalid);
            }
            let principal: Decimal = receivable.try_get("principal_total")?;
            let paid: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(
                     pa.allocated_amount * GREATEST(p.amount_received - COALESCE(rev.reversed_amount, 0), 0)
                     / NULLIF(p.amount_received, 0)
                 ), 0)
                 FROM payment_allocations pa
                 JOIN payments p ON p.id = pa.payment_id
                 LEFT JOIN (
                     SELECT payment_id, SUM(amount) AS reversed_amount
                     FROM payment_reversals GROUP BY payment_id
                 ) rev ON rev.payment_id = p.id
                 WHERE pa.receivable_id = $1 AND p.status = 'CENTRAL_ACCEPTED'",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            let credited: Decimal = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount), 0) FROM credit_notes WHERE receivable_id = $1",
            )
            .bind(receivable_id)
            .fetch_one(&mut *tx)
            .await?;
            if paid + credited + allocated_amount > principal {
                return Err(PaymentAllocationWriteError::Overallocated);
            }
            if receivable_has_installment_plan(tx, *receivable_id).await?
                && persisted_allocations.contains_key(&(*receivable_id, None))
            {
                return Err(PaymentAllocationWriteError::InstallmentRequired);
            }
            for ((_, schedule_id), _) in persisted_allocations
                .iter()
                .filter(|((id, _), _)| id == receivable_id)
            {
                if let Some(schedule_id) = schedule_id {
                    let requested = *persisted_allocations
                        .get(&(*receivable_id, Some(*schedule_id)))
                        .expect("grouped schedule allocation exists");
                    match validate_installment_allocation(
                        tx,
                        *schedule_id,
                        *receivable_id,
                        requested,
                    )
                    .await?
                    {
                        None => return Err(PaymentAllocationWriteError::ContextInvalid),
                        Some(false) => {}
                        Some(true) => {
                            return Err(PaymentAllocationWriteError::InstallmentOverallocated)
                        }
                    }
                }
            }
        }

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
            .bind(&payment_currency)
            .execute(&mut *tx)
            .await?;
        }

        let allocation_rows = sqlx::query(
            "SELECT id, receivable_id, schedule_id, allocated_amount
             FROM payment_allocations WHERE payment_id = $1
             ORDER BY id",
        )
        .bind(payment_id)
        .fetch_all(&mut *tx)
        .await?;
        let all_allocations = allocation_rows
            .iter()
            .map(|row| {
                Ok(json!({
                    "allocation_id": row.try_get::<Uuid, _>("id")?,
                    "receivable_id": row.try_get::<Uuid, _>("receivable_id")?,
                    "schedule_id": row.try_get::<Option<Uuid>, _>("schedule_id")?,
                    "amount": format!("{:.2}", row.try_get::<Decimal, _>("allocated_amount")?)
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let body = json!({
            "payment_id": payment_id,
            "customer_id": payment_customer_id,
            "payment": {
                "payment_id": payment_id,
                "status": "CENTRAL_ACCEPTED",
                "amount": format!("{:.2}", amount_received),
                "currency": payment_currency
            },
            "allocations": all_allocations
        });
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, event_type, subject_id, occurred_at, redacted_metadata)
             VALUES ($1, $2, 'PAYMENT_ALLOCATED', $3, now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(payment_id)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO central_changes
             (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'payments', $2, 2, $3)",
        )
        .bind(self.organization_id)
        .bind(payment_id)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'payment.allocate', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        Ok(PaymentAllocationWriteOutcome::Created(body))
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
        let outcome = self
            .reverse_payment_with_id_in_tx(
                &mut tx,
                payment_id,
                input,
                actor_id,
                reversal_id,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    async fn reverse_payment_with_id_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        payment_id: Uuid,
        input: &ReversePaymentRequest,
        actor_id: Uuid,
        reversal_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<PaymentReverseOutcome, PaymentReverseError> {
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
        Ok(PaymentReverseOutcome::Created(body))
    }

    pub async fn create_credit_note(
        &self,
        receivable_id: Uuid,
        input: &CreditNoteRequest,
        actor_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CreditNoteWriteOutcome, CreditNoteWriteError> {
        self.create_credit_note_with_id(
            receivable_id,
            input,
            actor_id,
            Uuid::new_v4(),
            idempotency_key,
            request_digest,
        )
        .await
    }

    pub async fn create_credit_note_with_id(
        &self,
        receivable_id: Uuid,
        input: &CreditNoteRequest,
        actor_id: Uuid,
        credit_note_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CreditNoteWriteOutcome, CreditNoteWriteError> {
        let mut tx = self.pool.begin().await?;
        let outcome = self
            .create_credit_note_with_id_in_tx(
                &mut tx,
                receivable_id,
                input,
                actor_id,
                credit_note_id,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_credit_note_with_id_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        receivable_id: Uuid,
        input: &CreditNoteRequest,
        actor_id: Uuid,
        credit_note_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<CreditNoteWriteOutcome, CreditNoteWriteError> {
        if let Some(row) = sqlx::query(
            "SELECT request_digest, response_json FROM idempotency_keys
             WHERE organization_id = $1 AND principal_id = $2
               AND operation = 'receivable.credit' AND idempotency_key = $3",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let original_digest: String = row.try_get("request_digest")?;
            if original_digest != request_digest {
                return Err(CreditNoteWriteError::IdempotencyPayloadMismatch);
            }
            let response: serde_json::Value = row.try_get("response_json")?;
            return Ok(CreditNoteWriteOutcome::Replay(response));
        }
        if input.amount <= Decimal::ZERO {
            return Err(CreditNoteWriteError::InvalidAmount);
        }
        if input.reason.trim().is_empty() {
            return Err(CreditNoteWriteError::ReasonRequired);
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
            return Err(CreditNoteWriteError::ActorNotActive);
        };
        if !matches!(role.as_str(), "ADMIN" | "SUPERVISOR") {
            return Err(CreditNoteWriteError::ActorNotAuthorized);
        }
        let Some(row) = sqlx::query(
            "SELECT r.status, r.currency, r.principal_total,
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
                            FROM payment_reversals GROUP BY payment_id
                        ) rev ON rev.payment_id = p.id
                        WHERE pa.receivable_id = r.id AND p.status = 'CENTRAL_ACCEPTED'
                    ), 0) AS paid,
                    COALESCE((
                        SELECT SUM(amount) FROM credit_notes WHERE receivable_id = r.id
                    ), 0) AS credited
             FROM receivables r
             JOIN customers c ON c.id = r.customer_id
             WHERE r.id = $1 AND c.organization_id = $2
             FOR UPDATE OF r",
        )
        .bind(receivable_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(CreditNoteWriteError::ReceivableNotFound);
        };
        let status: String = row.try_get("status")?;
        if matches!(status.as_str(), "VOID" | "REJECTED" | "SETTLED") {
            return Err(CreditNoteWriteError::ReceivableNotCreditable);
        }
        let currency: String = row.try_get("currency")?;
        let principal: Decimal = row.try_get("principal_total")?;
        let paid: Decimal = row.try_get("paid")?;
        let credited: Decimal = row.try_get("credited")?;
        let outstanding = principal - paid - credited;
        if input.amount > outstanding {
            return Err(CreditNoteWriteError::AmountExceedsBalance);
        }
        sqlx::query(
            "INSERT INTO credit_notes (id, receivable_id, amount, reason, initiated_by)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(credit_note_id)
        .bind(receivable_id)
        .bind(input.amount)
        .bind(input.reason.trim())
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
        let remaining = outstanding - input.amount;
        let new_status = if remaining <= Decimal::ZERO {
            "SETTLED"
        } else {
            "CREDITED"
        };
        sqlx::query("UPDATE receivables SET status = $2 WHERE id = $1")
            .bind(receivable_id)
            .bind(new_status)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO audit_events
             (id, actor, event_type, subject_id, occurred_at, redacted_metadata)
             VALUES ($1, $2, 'CREDIT_NOTE_RECORDED', $3, now(), $4)",
        )
        .bind(Uuid::new_v4())
        .bind(actor_id)
        .bind(receivable_id)
        .bind(json!({
            "credit_note_id": credit_note_id,
            "amount": input.amount,
            "reason": input.reason.trim()
        }))
        .execute(&mut *tx)
        .await?;
        let body = json!({
            "credit_note_id": credit_note_id,
            "receivable_id": receivable_id,
            "amount": format!("{:.2}", input.amount),
            "currency": currency,
            "reason": input.reason.trim(),
            "status": "CENTRAL_ACCEPTED",
            "receivable_status": new_status,
            "remaining_balance": format!("{remaining:.2}"),
            "actor_id": actor_id
        });
        sqlx::query(
            "INSERT INTO central_changes
             (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'credit_notes', $2, 1, $3)",
        )
        .bind(self.organization_id)
        .bind(credit_note_id)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO idempotency_keys
             (organization_id, principal_id, operation, idempotency_key,
              request_digest, response_status, response_json)
             VALUES ($1, $2, 'receivable.credit', $3, $4, 201, $5)",
        )
        .bind(self.organization_id)
        .bind(actor_id)
        .bind(idempotency_key)
        .bind(request_digest)
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        Ok(CreditNoteWriteOutcome::Created(body))
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
            "SELECT r.id, r.source_type, r.source_id, r.currency, r.principal_total,
                    CASE WHEN r.source_type = 'INVOICE' THEN COALESCE(i.invoice_number, r.source_id::text)
                         WHEN r.source_type = 'SALE' THEN COALESCE((SELECT s.document_reference FROM sales s WHERE s.id = r.source_id), r.source_id::text)
                         ELSE r.source_id::text END AS source_reference,
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
                    ), 0) AS paid,
                    COALESCE((SELECT SUM(amount) FROM credit_notes WHERE receivable_id = r.id), 0) AS credited
             FROM receivables r
             LEFT JOIN invoices i ON i.id = r.source_id AND r.source_type = 'INVOICE'
             WHERE r.customer_id = $1
             GROUP BY r.id, r.source_type, r.source_id, r.currency, r.principal_total, i.invoice_number
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
                let credited: Decimal = row.try_get("credited")?;
                let principal = principal.round_dp(2);
                let mut paid = paid.round_dp(2);
                paid.rescale(2);
                let credited = credited.round_dp(2);
                let balance = (principal - paid - credited).round_dp(2);
                Ok(json!({
                    "receivable_id": row.try_get::<Uuid, _>("id")?,
                    "source_type": row.try_get::<String, _>("source_type")?,
                    "source_reference": row.try_get::<String, _>("source_reference")?,
                    "currency": row.try_get::<String, _>("currency")?,
                    "principal": format!("{principal:.2}"),
                    "paid": format!("{paid:.2}"),
                    "credited": format!("{credited:.2}"),
                    "balance": format!("{balance:.2}")
                }))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let invoices = self
            .list_invoice_responses(Some(customer_id), None, None, 100)
            .await?;
        let payment_rows = sqlx::query(
            "SELECT id, amount_received, currency, method, paid_at, status
             FROM payments WHERE customer_id = $1
             ORDER BY paid_at DESC, id DESC LIMIT 100",
        )
        .bind(customer_id)
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
        .bind(customer_id)
        .fetch_all(&self.pool)
        .await?;
        let mut allocations_by_payment: BTreeMap<Uuid, Vec<serde_json::Value>> = BTreeMap::new();
        for allocation in allocation_rows {
            let payment_id: Uuid = allocation.try_get("payment_id")?;
            let amount: Decimal = allocation.try_get("allocated_amount")?;
            allocations_by_payment
                .entry(payment_id)
                .or_default()
                .push(json!({
                    "receivable_id": allocation.try_get::<Uuid, _>("receivable_id")?,
                    "schedule_id": allocation.try_get::<Option<Uuid>, _>("schedule_id")?,
                    "amount": format!("{:.2}", amount.round_dp(2)),
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
        .bind(customer_id)
        .fetch_all(&self.pool)
        .await?;
        let mut reversals_by_payment: BTreeMap<Uuid, Vec<serde_json::Value>> = BTreeMap::new();
        for reversal in reversal_rows {
            let payment_id: Uuid = reversal.try_get("payment_id")?;
            let amount: Decimal = reversal.try_get("amount")?;
            let created_at: DateTime<Utc> = reversal.try_get("created_at")?;
            reversals_by_payment
                .entry(payment_id)
                .or_default()
                .push(json!({
                    "amount": format!("{:.2}", amount.round_dp(2)),
                    "reason": reversal.try_get::<String, _>("reason")?,
                    "created_at": created_at.to_rfc3339()
                }));
        }
        let mut payments = Vec::with_capacity(payment_rows.len());
        for row in payment_rows {
            let payment_id: Uuid = row.try_get("id")?;
            let amount_received: Decimal = row.try_get("amount_received")?;
            let paid_at: DateTime<Utc> = row.try_get("paid_at")?;
            payments.push(json!({
                "payment_id": payment_id,
                "amount_received": format!("{:.2}", amount_received.round_dp(2)),
                "currency": row.try_get::<String, _>("currency")?,
                "method": row.try_get::<String, _>("method")?,
                "paid_at": paid_at.to_rfc3339(),
                "status": row.try_get::<String, _>("status")?,
                "allocations": allocations_by_payment.remove(&payment_id).unwrap_or_default(),
                "reversals": reversals_by_payment.remove(&payment_id).unwrap_or_default()
            }));
        }
        Ok(Some(json!({
            "customer_id": customer_id,
            "receivables": receivables,
            "invoices": invoices,
            "payments": payments
        })))
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
        let outcome = self
            .create_solar_sale_with_ids_in_tx(
                &mut tx,
                input,
                sale_id,
                receivable_id,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    /// Lists solar sales from the central ledger. The desktop client never
    /// keeps a separate sales cache in online-only mode.
    pub async fn list_solar_sales(
        &self,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT s.id, s.document_reference, s.customer_id, s.contract_id, s.currency, s.sold_at,
                    s.sale_type, s.status, r.id AS receivable_id, r.principal_total,
                    ip.id AS installment_plan_id,
                    (SELECT p.sku FROM sale_lines sl JOIN products p ON p.id = sl.product_id
                     WHERE sl.sale_id = s.id ORDER BY sl.id LIMIT 1) AS product_code
             FROM sales s
             JOIN customers c ON c.id = s.customer_id
             LEFT JOIN receivables r ON r.source_type = 'SALE' AND r.source_id = s.id
             LEFT JOIN installment_plans ip ON ip.receivable_id = r.id
             WHERE c.organization_id = $1 AND s.sale_type = 'SOLAR_KIT'
             ORDER BY s.sold_at DESC, s.id DESC
             LIMIT $2",
        )
        .bind(self.organization_id)
        .bind(i64::from(limit.clamp(1, 100)))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let sold_at: DateTime<Utc> = row.try_get("sold_at")?;
                let principal: Option<Decimal> = row.try_get("principal_total")?;
                let sale_id: Uuid = row.try_get("id")?;
                let document_reference: Option<String> = row.try_get("document_reference")?;
                Ok(json!({
                    "sale_id": sale_id,
                    "sale_number": document_reference.unwrap_or_else(|| sale_id.to_string()),
                    "customer_id": row.try_get::<Uuid, _>("customer_id")?,
                    "contract_id": row.try_get::<Option<Uuid>, _>("contract_id")?,
                    "installment_plan_id": row.try_get::<Option<Uuid>, _>("installment_plan_id")?,
                    "receivable_id": row.try_get::<Option<Uuid>, _>("receivable_id")?,
                    "principal": principal.map(|value| format!("{:.2}", value.round_dp(2))),
                    "currency": row.try_get::<String, _>("currency")?,
                    "product_code": row.try_get::<Option<String>, _>("product_code")?,
                    "sold_at": sold_at.to_rfc3339(),
                    "sale_type": row.try_get::<String, _>("sale_type")?,
                    "status": row.try_get::<String, _>("status")?
                }))
            })
            .collect()
    }

    /// Returns structured data for a locally generated solar confirmation PDF.
    /// No document bytes are written to the server.
    pub async fn solar_sale_document_data(
        &self,
        sale_id: Uuid,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let sale = sqlx::query(
            "SELECT s.id, s.document_reference, s.customer_id, s.contract_id, s.currency, s.sold_at, s.status,
                    r.id AS receivable_id, r.principal_total,
                    c.customer_code, c.legal_name, c.contact_fields,
                    o.legal_name AS organization_name
             FROM sales s
             JOIN customers c ON c.id = s.customer_id
             JOIN organizations o ON o.id = c.organization_id
             LEFT JOIN receivables r ON r.source_type = 'SALE' AND r.source_id = s.id
             WHERE s.id = $1 AND c.organization_id = $2 AND s.sale_type = 'SOLAR_KIT'",
        )
        .bind(sale_id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(sale) = sale else {
            return Ok(None);
        };

        let lines = sqlx::query(
            "SELECT p.sku AS product_reference, sl.quantity, sl.unit_price,
                    sl.tax_snapshot, sl.total
             FROM sale_lines sl
             JOIN products p ON p.id = sl.product_id
             WHERE sl.sale_id = $1 ORDER BY sl.id",
        )
        .bind(sale_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|line| {
            let quantity: Decimal = line.try_get("quantity")?;
            let unit_price: Decimal = line.try_get("unit_price")?;
            let total: Decimal = line.try_get("total")?;
            let tax_snapshot: serde_json::Value = line.try_get("tax_snapshot")?;
            let tax_amount = tax_snapshot["amount"]
                .as_str()
                .and_then(|value| value.parse::<Decimal>().ok())
                .unwrap_or(Decimal::ZERO);
            let subtotal = (unit_price * quantity).round_dp(2);
            Ok(json!({
                "product_reference": line.try_get::<String, _>("product_reference")?,
                "quantity": quantity.normalize().to_string(),
                "unit_price": format!("{:.2}", unit_price.round_dp(2)),
                "subtotal": format!("{:.2}", subtotal),
                "tax_snapshot": tax_snapshot,
                "tax_amount": format!("{:.2}", tax_amount),
                "total": format!("{:.2}", total.round_dp(2))
            }))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let receivable_id: Option<Uuid> = sale.try_get("receivable_id")?;
        let plan = if let Some(receivable_id) = receivable_id {
            if let Some(plan) = sqlx::query(
                "SELECT id, principal_amount, currency, start_date, status
                 FROM installment_plans WHERE receivable_id = $1",
            )
            .bind(receivable_id)
            .fetch_optional(&self.pool)
            .await?
            {
                let plan_id: Uuid = plan.try_get("id")?;
                let schedule = sqlx::query(
                    "SELECT id, sequence, due_date, amount_due, schedule_status
                     FROM installment_schedule WHERE plan_id = $1 ORDER BY sequence",
                )
                .bind(plan_id)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|item| {
                    let amount: Decimal = item.try_get("amount_due")?;
                    Ok(json!({
                        "id": item.try_get::<Uuid, _>("id")?,
                        "sequence": item.try_get::<i32, _>("sequence")?,
                        "due_date": item.try_get::<NaiveDate, _>("due_date")?.to_string(),
                        "amount_due": format!("{:.2}", amount.round_dp(2)),
                        "status": item.try_get::<String, _>("schedule_status")?
                    }))
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
                json!({
                    "id": plan_id,
                    "principal": format!("{:.2}", plan.try_get::<Decimal, _>("principal_amount")?.round_dp(2)),
                    "currency": plan.try_get::<String, _>("currency")?,
                    "start_date": plan.try_get::<NaiveDate, _>("start_date")?.to_string(),
                    "status": plan.try_get::<String, _>("status")?,
                    "schedule": schedule
                })
            } else {
                json!({"id": null, "schedule": []})
            }
        } else {
            json!({"id": null, "schedule": []})
        };

        let sold_at: DateTime<Utc> = sale.try_get("sold_at")?;
        let status: String = sale.try_get("status")?;
        let contacts: serde_json::Value = sale.try_get("contact_fields")?;
        let principal: Option<Decimal> = sale.try_get("principal_total")?;
        let document_reference: Option<String> = sale.try_get("document_reference")?;
        Ok(Some(json!({
            "document_schema_version": 1,
            "document_type": "SOLAR_SALE_CONFIRMATION",
            "document_reference": document_reference.unwrap_or_else(|| format!("SEL-SOLAR-{}", sale_id.simple().to_string().to_uppercase())),
            "qr_internal_reference": format!("SEL:SOLAR-SALE:{sale_id}"),
            "organization_name": sale.try_get::<String, _>("organization_name")?,
            "sale": {
                "sale_id": sale_id,
                "receivable_id": receivable_id,
                "customer_id": sale.try_get::<Uuid, _>("customer_id")?,
                "contract_id": sale.try_get::<Option<Uuid>, _>("contract_id")?,
                "subtotal": lines.iter().filter_map(|line| line["subtotal"].as_str()?.parse::<Decimal>().ok()).sum::<Decimal>().round_dp(2).to_string(),
                "tax_amount": lines.iter().filter_map(|line| line["tax_amount"].as_str()?.parse::<Decimal>().ok()).sum::<Decimal>().round_dp(2).to_string(),
                "principal": principal.map(|value| format!("{:.2}", value.round_dp(2))),
                "currency": sale.try_get::<String, _>("currency")?,
                "sold_at": sold_at.to_rfc3339(),
                "status": status
            },
            "customer": {
                "customer_code": sale.try_get::<String, _>("customer_code")?,
                "legal_name": sale.try_get::<String, _>("legal_name")?,
                "address": contacts.get("address").cloned().unwrap_or(serde_json::Value::Null),
                "phone": contacts.get("phone").cloned().unwrap_or(serde_json::Value::Null)
            },
            "lines": lines,
            "installment_plan": plan,
            "origin": "CENTRAL_ACCEPTED"
        })))
    }

    async fn create_solar_sale_with_ids_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        input: &CreateSolarSaleRequest,
        sale_id: Uuid,
        receivable_id: Uuid,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarSaleWriteOutcome, SolarSaleWriteError> {
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
        // Serialize solar-tax selection with administrative approval/revocation.
        // This closes the phantom-row window that row locks alone cannot guard.
        sqlx::query("SELECT pg_advisory_xact_lock(1249681746, 1)")
            .execute(&mut *tx)
            .await?;
        let product_rows = sqlx::query(
            "SELECT id, price_reference FROM products
             WHERE sku = $1 AND product_type = 'SOLAR_KIT' AND currency = $2
               AND active = TRUE AND status = 'APPROVED'
               AND effective_from <= CURRENT_DATE
               AND (effective_to IS NULL OR effective_to >= CURRENT_DATE)
             ORDER BY effective_from DESC, id LIMIT 2 FOR SHARE",
        )
        .bind(input.product_code.trim())
        .bind(&input.currency)
        .fetch_all(&mut *tx)
        .await?;
        let Some(product_row) = product_rows.first() else {
            return Err(SolarSaleWriteError::ProductNotFound);
        };
        if product_rows.len() != 1 {
            return Err(SolarSaleWriteError::Database(sqlx::Error::Protocol(
                "multiple approved solar products apply to this SKU and date".into(),
            )));
        }
        let product_id: Uuid = product_row.try_get("id")?;
        let subtotal = product_row
            .try_get::<Decimal, _>("price_reference")?
            .round_dp(2);
        let tax_rows = sqlx::query(
            "SELECT id, jurisdiction, base, rate, source, approval_ref
             FROM tax_profile_versions
             WHERE service_code = 'SOLAR_KIT' AND status = 'APPROVED'
               AND effective_from <= CURRENT_DATE
               AND (effective_to IS NULL OR effective_to >= CURRENT_DATE)
             ORDER BY effective_from DESC, jurisdiction, base, id LIMIT 2
             FOR SHARE",
        )
        .fetch_all(&mut *tx)
        .await?;
        if tax_rows.len() != 1 {
            return Err(SolarSaleWriteError::TaxProfileUnavailable);
        }
        let tax_row = &tax_rows[0];
        let tax_base: String = tax_row.try_get("base")?;
        if !matches!(tax_base.as_str(), "SUBTOTAL" | "SERVICE") {
            return Err(SolarSaleWriteError::TaxProfileUnavailable);
        }
        let tax_profile_id: Uuid = tax_row.try_get("id")?;
        let tax_rate: Decimal = tax_row.try_get("rate")?;
        let tax_amount = (subtotal * tax_rate).round_dp(2);
        let total = subtotal + tax_amount;
        if input.principal.round_dp(2) != total {
            return Err(SolarSaleWriteError::PriceMismatch);
        }
        let sold_at: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
            .fetch_one(&mut *tx)
            .await?;
        let document_reference = issued_document_reference(sold_at, sale_id);
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
            status: "CENTRAL_ACCEPTED".into(),
            product_code: Some(input.product_code.trim().to_owned()),
        };
        let snapshot = serde_json::to_value(&response)?;
        sqlx::query(
            "INSERT INTO sales
             (id, customer_id, currency, sold_at, sale_type, status, document_reference)
             VALUES ($1, $2, $3, $4, 'SOLAR_KIT', 'CENTRAL_ACCEPTED', $5)",
        )
        .bind(sale_id)
        .bind(input.customer_id)
        .bind(&input.currency)
        .bind(sold_at)
        .bind(&document_reference)
        .execute(&mut *tx)
        .await?;
        let tax_snapshot = serde_json::to_value(sel_contracts::SolarTaxSnapshot {
            schema_version: 1,
            service_code: "SOLAR_KIT".into(),
            tax_profile_id,
            jurisdiction: tax_row.try_get("jurisdiction")?,
            base: tax_base,
            rate: tax_rate,
            amount: tax_amount,
            source: tax_row.try_get("source")?,
            approval_ref: tax_row.try_get("approval_ref")?,
        })?;
        sqlx::query(
            "INSERT INTO sale_lines
             (id, sale_id, product_id, quantity, unit_price, tax_snapshot, total)
             VALUES ($1, $2, $3, 1, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(sale_id)
        .bind(product_id)
        .bind(response.subtotal)
        .bind(tax_snapshot)
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
        let outcome = self
            .activate_solar_contract_with_plan_id_in_tx(
                &mut tx,
                contract_id,
                plan_id,
                input,
                idempotency_key,
                request_digest,
            )
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }

    async fn activate_solar_contract_with_plan_id_in_tx(
        &self,
        tx: &mut sqlx::PgConnection,
        contract_id: Uuid,
        plan_id: Uuid,
        input: &ActivateSolarContractRequest,
        idempotency_key: &str,
        request_digest: &str,
    ) -> Result<SolarContractWriteOutcome, SolarContractWriteError> {
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
        Ok(SolarContractWriteOutcome::Created(response))
    }

    pub async fn apply_sync_event(
        &self,
        event: &SyncEnvelope,
    ) -> Result<SyncEventResult, SyncApplyError> {
        let mut tx = self.pool.begin().await?;
        // Serialize only concurrent submissions of the same device/event pair.
        // Without this transaction-scoped lock, two requests can both observe
        // an empty sync_inbox before either one commits its durable effect.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("sync-event:{}:{}", event.device_id, event.event_id))
            .execute(&mut *tx)
            .await?;
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
            "SERVICE_ACCOUNT_CREATED"
            | "SERVICE_ACCOUNT_UPDATED"
            | "METER_CREATED"
            | "METER_READING_CREATED"
            | "METER_READING_CORRECTED" => {
                let result = self.apply_meter_sync_event(&mut tx, event).await?;
                tx.commit().await?;
                Ok(result)
            }
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
                     (id, organization_id, customer_code, legal_name, customer_category, contact_fields, status, revision)
                     VALUES ($1, $2, $3, $4, $5, $6, 'ACTIVE', 1)",
                )
                .bind(event.aggregate_id)
                .bind(self.organization_id)
                .bind(customer_code)
                .bind(legal_name)
                .bind(event.payload.get("customer_category").and_then(serde_json::Value::as_str).map(str::trim).map(str::to_ascii_uppercase))
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
                sqlx::query(
                    "UPDATE customers SET
                         legal_name = CASE
                           WHEN $1 ? 'legal_name' AND jsonb_typeof($1->'legal_name') <> 'null'
                           THEN $1->>'legal_name' ELSE legal_name END,
                         customer_category = CASE
                           WHEN $1 ? 'customer_category'
                           THEN NULLIF(upper(trim($1->>'customer_category')), '')
                           ELSE customer_category END,
                         contact_fields = jsonb_build_object(
                           'address', CASE
                             WHEN $1 ? 'address' AND ($1->>'patch_format_version' = '2' OR jsonb_typeof($1->'address') <> 'null')
                             THEN $1->'address' ELSE contact_fields->'address' END,
                           'phone', CASE
                             WHEN $1 ? 'phone' AND ($1->>'patch_format_version' = '2' OR jsonb_typeof($1->'phone') <> 'null')
                             THEN $1->'phone' ELSE contact_fields->'phone' END
                         ),
                         status = CASE
                           WHEN $1 ? 'status' AND jsonb_typeof($1->'status') <> 'null'
                           THEN $1->>'status' ELSE status END,
                         revision = revision + 1
                     WHERE id = $2 AND organization_id = $3",
                )
                .bind(&event.payload)
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
                    identity_snapshot,
                } = payload;
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
                let customer_category = match self.get_customer(input.customer_id).await? {
                    Some(customer) => match customer.customer_category {
                        Some(category) if !category.trim().is_empty() => category,
                        _ => {
                            return self
                                .persist_sync_rejection(
                                    event,
                                    "CUSTOMER_CATEGORY_REQUIRED",
                                    "assign_a_customer_category_before_replaying_the_invoice",
                                )
                                .await;
                        }
                    },
                    None => {
                        return self
                            .persist_sync_rejection(
                                event,
                                "CUSTOMER_NOT_FOUND",
                                "synchronize_the_customer_before_the_invoice",
                            )
                            .await;
                    }
                };
                let tariffs = self
                    .load_tariffs(
                        &input.tariff_revision,
                        &input.currency,
                        &input.billing_period,
                        &customer_category,
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
                    communication_charge: match self
                        .load_communication_charge(&input.currency, Utc::now().date_naive())
                        .await?
                    {
                        Some(amount) => amount,
                        None => {
                            return self
                                .persist_sync_rejection(
                                    event,
                                    "COMMUNICATION_CHARGE_UNAVAILABLE",
                                    "approve_a_communication_charge_for_the_invoice_currency",
                                )
                                .await;
                        }
                    },
                    communication_tax_rate: electricity_tax_rate,
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
                    billing_period: input.billing_period.clone(),
                    due_date: input.due_date,
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
                    .issue_utility_invoice_in_tx(
                        &mut tx,
                        &input,
                        &response,
                        identity_snapshot.as_ref(),
                        &operation_key,
                        &event.payload_sha256,
                    )
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
                let result = persist_sync_result(
                    &mut tx,
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
                            "response": accepted_response,
                            "identity_snapshot": identity_snapshot
                        }),
                    )),
                )
                .await?;
                tx.commit().await?;
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
                let operation_key = format!("sync-solar-sale-{}", event.event_id);
                let outcome = self
                    .create_solar_sale_with_ids_in_tx(
                        &mut tx,
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
                let result = persist_sync_result(
                    &mut tx,
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
                tx.commit().await?;
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
                let operation_key = format!("sync-solar-contract-{}", event.event_id);
                let outcome = self
                    .activate_solar_contract_with_plan_id_in_tx(
                        &mut tx,
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
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: entity_id,
                    },
                    Some(("installment_plans", entity_id, 1, body)),
                )
                .await?;
                tx.commit().await?;
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
                let outcome = self
                    .void_invoice_with_id_in_tx(
                        &mut tx,
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
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: payload.invoice_id,
                    },
                    Some(("invoices", payload.invoice_id, 2, body)),
                )
                .await?;
                tx.commit().await?;
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
                let operation_key = format!("sync-payment-{}", event.event_id);
                let outcome = self
                    .record_payment_with_id_in_tx(
                        &mut tx,
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
                        let result = SyncEventResult::Rejected {
                            code: "PAYMENT_NOT_ACCEPTED".into(),
                            required_action: "review_payment_and_retry".into(),
                        };
                        persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                            .await?;
                        tx.commit().await?;
                        return Ok(result);
                    }
                };
                sqlx::query(
                    "UPDATE payments SET origin_event_id = $1 WHERE id = $2 AND origin_event_id IS NULL",
                )
                .bind(event.event_id)
                .bind(payment_id)
                .execute(&mut *tx)
                .await?;
                let allocation_rows = sqlx::query(
                    "SELECT id, receivable_id, schedule_id, allocated_amount, currency
                     FROM payment_allocations WHERE payment_id = $1
                     ORDER BY id",
                )
                .bind(payment_id)
                .fetch_all(&mut *tx)
                .await?;
                let allocations = allocation_rows
                    .iter()
                    .map(|row| {
                        Ok(json!({
                            "allocation_id": row.try_get::<Uuid, _>("id")?,
                            "receivable_id": row.try_get::<Uuid, _>("receivable_id")?,
                            "schedule_id": row.try_get::<Option<Uuid>, _>("schedule_id")?,
                            "amount": format!("{:.2}", row.try_get::<Decimal, _>("allocated_amount")?),
                            "currency": row.try_get::<String, _>("currency")?
                        }))
                    })
                    .collect::<Result<Vec<_>, sqlx::Error>>()?;
                let result = persist_sync_result(
                    &mut tx,
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
                            "allocations": allocations,
                        }),
                    )),
                )
                .await?;
                tx.commit().await?;
                Ok(result)
            }
            "PAYMENT_ALLOCATED" => {
                let payload: PaymentAllocationSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "PAYMENT_ALLOCATION_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_payment_allocation_payload".into(),
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
                let operation_key = format!("sync-payment-allocation-{}", event.event_id);
                let outcome = self
                    .allocate_payment_in_tx(
                        &mut tx,
                        payload.payment_id,
                        &payload.allocations,
                        event.actor_id,
                        &operation_key,
                        &event.payload_sha256,
                    )
                    .await;
                match outcome {
                    Ok(PaymentAllocationWriteOutcome::Created(_))
                    | Ok(PaymentAllocationWriteOutcome::Replay(_)) => {}
                    Err(_) => {
                        let result = SyncEventResult::Rejected {
                            code: "PAYMENT_ALLOCATION_NOT_ACCEPTED".into(),
                            required_action: "review_payment_allocation_and_retry".into(),
                        };
                        persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                            .await?;
                        tx.commit().await?;
                        return Ok(result);
                    }
                };
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: payload.payment_id,
                    },
                    None,
                )
                .await?;
                tx.commit().await?;
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
                let input = ReversePaymentRequest { amount, reason };
                let operation_key = format!("sync-payment-reversal-{}", event.event_id);
                let outcome = self
                    .reverse_payment_with_id_in_tx(
                        &mut tx,
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
                        let result = SyncEventResult::Rejected {
                            code: "PAYMENT_REVERSAL_NOT_ACCEPTED".into(),
                            required_action: "review_payment_reversal_and_retry".into(),
                        };
                        persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                            .await?;
                        tx.commit().await?;
                        return Ok(result);
                    }
                };
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: reversal_id,
                    },
                    Some(("payment_reversals", reversal_id, 1, body)),
                )
                .await?;
                tx.commit().await?;
                Ok(result)
            }
            "CREDIT_NOTE_RECORDED" => {
                let payload: CreditNoteSyncPayload =
                    match serde_json::from_value(event.payload.clone()) {
                        Ok(payload) => payload,
                        Err(_) => {
                            let result = SyncEventResult::Rejected {
                                code: "CREDIT_NOTE_PAYLOAD_INVALID".into(),
                                required_action: "send_a_valid_credit_note_payload".into(),
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
                if payload.actor_id != event.actor_id {
                    let result = SyncEventResult::Rejected {
                        code: "CREDIT_NOTE_ACTOR_MISMATCH".into(),
                        required_action: "recreate_credit_note_with_the_authenticated_actor".into(),
                    };
                    persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                        .await?;
                    tx.commit().await?;
                    return Ok(result);
                }
                let input = CreditNoteRequest {
                    amount: payload.amount,
                    reason: payload.reason,
                };
                let outcome = self
                    .create_credit_note_with_id_in_tx(
                        &mut tx,
                        payload.receivable_id,
                        &input,
                        event.actor_id,
                        payload.credit_note_id,
                        &format!("sync-credit-note-{}", event.event_id),
                        &event.payload_sha256,
                    )
                    .await;
                let body = match outcome {
                    Ok(CreditNoteWriteOutcome::Created(body))
                    | Ok(CreditNoteWriteOutcome::Replay(body)) => body,
                    Err(_) => {
                        let result = SyncEventResult::Rejected {
                            code: "CREDIT_NOTE_NOT_ACCEPTED".into(),
                            required_action: "review_credit_note_and_receivable_balance".into(),
                        };
                        persist_sync_result(&mut tx, self.organization_id, event, &result, None)
                            .await?;
                        tx.commit().await?;
                        return Ok(result);
                    }
                };
                let result = persist_sync_result(
                    &mut tx,
                    self.organization_id,
                    event,
                    &SyncEventResult::Accepted {
                        server_sequence: 0,
                        server_entity_id: payload.credit_note_id,
                    },
                    Some(("credit_notes", payload.credit_note_id, 1, body)),
                )
                .await?;
                tx.commit().await?;
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
        let actor_role = sqlx::query_scalar::<_, String>(
            "SELECT role FROM users WHERE id = $1 AND organization_id = $2 AND status = 'ACTIVE'",
        )
        .bind(event.actor_id)
        .bind(self.organization_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(actor_role) = actor_role else {
            return Ok(false);
        };
        if !offline_role_allows_command(&actor_role, &event.command_type) {
            return Ok(false);
        }
        let Some(grant) = sqlx::query(
            "SELECT scopes, limits, issued_at, expires_at, signature
             FROM offline_grants
             WHERE id = $1 AND user_id = $2 AND device_id = $3
               AND revoked_at IS NULL AND issued_at <= now() AND expires_at > now()",
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

    async fn apply_meter_sync_event(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &SyncEnvelope,
    ) -> Result<SyncEventResult, sqlx::Error> {
        let rejected = |code: &str, required_action: &str| SyncEventResult::Rejected {
            code: code.into(),
            required_action: required_action.into(),
        };
        let conflict = |reason: &str| SyncEventResult::Conflict {
            reason: reason.into(),
            central_revision: None,
        };
        let (entity, payload_id) = match event.command_type.as_str() {
            "SERVICE_ACCOUNT_CREATED" => {
                let Ok(account) = serde_json::from_value::<ServiceAccount>(event.payload.clone())
                else {
                    let result = rejected(
                        "SERVICE_ACCOUNT_PAYLOAD_INVALID",
                        "recreate_the_service_account_command",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                if account.id != event.aggregate_id
                    || !matches!(account.service_code.as_str(), "ELECTRICITY" | "WATER")
                    || !matches!(
                        account.connection_status.as_str(),
                        "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN"
                    )
                {
                    let result = rejected(
                        "SERVICE_ACCOUNT_INVALID",
                        "provide_a_valid_service_account_snapshot",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let customer_exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND organization_id = $2)",
                )
                .bind(account.customer_id)
                .bind(self.organization_id)
                .fetch_one(&mut **tx)
                .await?;
                if !customer_exists {
                    let result = rejected(
                        "CUSTOMER_NOT_FOUND",
                        "sync_the_customer_before_the_service_account",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM service_accounts WHERE id = $1)",
                )
                .bind(account.id)
                .fetch_one(&mut **tx)
                .await?;
                if exists {
                    let result = conflict("SERVICE_ACCOUNT_ALREADY_EXISTS");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                sqlx::query(
                    "INSERT INTO service_accounts (id, customer_id, service_code, contract_ref, connection_status, activated_at, revision)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(account.id)
                .bind(account.customer_id)
                .bind(&account.service_code)
                .bind(&account.contract_ref)
                .bind(&account.connection_status)
                .bind(account.activated_at)
                .bind(account.revision)
                .execute(&mut **tx)
                .await?;
                ("service_accounts", account.id)
            }
            "SERVICE_ACCOUNT_UPDATED" => {
                let Ok(payload) = serde_json::from_value::<ServiceAccountUpdateSyncPayload>(
                    event.payload.clone(),
                ) else {
                    let result = rejected(
                        "SERVICE_ACCOUNT_UPDATE_INVALID",
                        "send_a_valid_revisioned_service_account_update",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                let account = payload.account;
                if account.id != event.aggregate_id
                    || payload.expected_revision < 1
                    || account.revision != payload.expected_revision + 1
                    || !matches!(account.service_code.as_str(), "ELECTRICITY" | "WATER")
                    || !matches!(
                        account.connection_status.as_str(),
                        "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN"
                    )
                    || account
                        .contract_ref
                        .as_deref()
                        .is_some_and(|value| value.trim().is_empty() || value.chars().count() > 128)
                {
                    let result = rejected(
                        "SERVICE_ACCOUNT_UPDATE_INVALID",
                        "send_a_valid_service_account_update",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let current = sqlx::query(
                    "SELECT sa.customer_id, sa.service_code, sa.revision FROM service_accounts sa
                     JOIN customers c ON c.id = sa.customer_id
                     WHERE sa.id = $1 AND c.organization_id = $2 FOR UPDATE OF sa",
                )
                .bind(account.id)
                .bind(self.organization_id)
                .fetch_optional(&mut **tx)
                .await?;
                let Some(current) = current else {
                    let result = rejected(
                        "SERVICE_ACCOUNT_NOT_FOUND",
                        "sync_the_service_account_before_its_update",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                let current_customer: Uuid = current.try_get("customer_id")?;
                let current_service: String = current.try_get("service_code")?;
                let current_revision: i64 = current.try_get("revision")?;
                if current_customer != account.customer_id
                    || current_service != account.service_code
                {
                    let result = conflict("SERVICE_ACCOUNT_IDENTITY_CHANGED");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                if current_revision != payload.expected_revision {
                    let mut result = conflict("SERVICE_ACCOUNT_REVISION_CONFLICT");
                    if let SyncEventResult::Conflict {
                        central_revision, ..
                    } = &mut result
                    {
                        *central_revision = Some(current_revision);
                    }
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                sqlx::query(
                    "UPDATE service_accounts SET contract_ref = $1, connection_status = $2,
                     activated_at = $3, revision = $4 WHERE id = $5 AND revision = $6",
                )
                .bind(&account.contract_ref)
                .bind(&account.connection_status)
                .bind(account.activated_at)
                .bind(account.revision)
                .bind(account.id)
                .bind(payload.expected_revision)
                .execute(&mut **tx)
                .await?;
                ("service_accounts", account.id)
            }
            "METER_CREATED" => {
                let Ok(meter) = serde_json::from_value::<Meter>(event.payload.clone()) else {
                    let result = rejected("METER_PAYLOAD_INVALID", "recreate_the_meter_command");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                if meter.id != event.aggregate_id || meter.serial.trim().is_empty() || !meter.active
                {
                    let result = rejected("METER_INVALID", "provide_a_valid_active_meter_snapshot");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let expected_unit: Option<String> = sqlx::query_scalar(
                    "SELECT CASE sa.service_code WHEN 'ELECTRICITY' THEN 'KWH' WHEN 'WATER' THEN 'M3' END
                     FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
                     WHERE sa.id = $1 AND c.organization_id = $2",
                )
                .bind(meter.service_account_id)
                .bind(self.organization_id)
                .fetch_optional(&mut **tx)
                .await?;
                let Some(expected_unit) = expected_unit else {
                    let result = rejected(
                        "SERVICE_ACCOUNT_NOT_FOUND",
                        "sync_the_service_account_before_the_meter",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                if meter.unit != expected_unit {
                    let result =
                        rejected("METER_UNIT_MISMATCH", "use_the_unit_defined_by_the_service");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM meters WHERE id = $1 OR (service_account_id = $2 AND serial = $3))",
                )
                .bind(meter.id)
                .bind(meter.service_account_id)
                .bind(meter.serial.trim())
                .fetch_one(&mut **tx)
                .await?;
                if exists {
                    let result = conflict("METER_ALREADY_EXISTS");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                sqlx::query("INSERT INTO meters (id, service_account_id, serial, unit, active) VALUES ($1, $2, $3, $4, TRUE)")
                    .bind(meter.id)
                    .bind(meter.service_account_id)
                    .bind(meter.serial.trim())
                    .bind(&meter.unit)
                    .execute(&mut **tx)
                    .await?;
                ("meters", meter.id)
            }
            "METER_READING_CREATED" | "METER_READING_CORRECTED" => {
                let Ok(reading) = serde_json::from_value::<MeterReading>(event.payload.clone())
                else {
                    let result = rejected(
                        "METER_READING_PAYLOAD_INVALID",
                        "recreate_the_meter_reading_command",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                let correction = event.command_type == "METER_READING_CORRECTED";
                let valid_period =
                    NaiveDate::parse_from_str(&format!("{}-01", reading.period), "%Y-%m-%d")
                        .is_ok();
                if reading.id != event.aggregate_id
                    || !valid_period
                    || reading.previous < Decimal::ZERO
                    || reading.current < reading.previous
                    || reading.measured_quantity != reading.current - reading.previous
                    || reading.source.trim().is_empty()
                    || reading.source.trim().chars().count() > 64
                    || (correction
                        && (reading.corrected_from_id.is_none()
                            || reading
                                .correction_reason
                                .as_deref()
                                .unwrap_or_default()
                                .trim()
                                .is_empty()))
                    || (!correction
                        && (reading.corrected_from_id.is_some()
                            || reading.correction_reason.is_some()))
                {
                    let result = rejected(
                        "METER_READING_INVALID",
                        "provide_a_consistent_period_and_meter_index_snapshot",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                let meter = sqlx::query(
                    "SELECT m.unit, m.active, sa.service_code FROM meters m
                     JOIN service_accounts sa ON sa.id = m.service_account_id
                     JOIN customers c ON c.id = sa.customer_id
                     WHERE m.id = $1 AND c.organization_id = $2 FOR UPDATE OF m",
                )
                .bind(reading.meter_id)
                .bind(self.organization_id)
                .fetch_optional(&mut **tx)
                .await?;
                let Some(meter) = meter else {
                    let result = rejected("METER_NOT_FOUND", "sync_the_meter_before_its_reading");
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                };
                let unit: String = meter.try_get("unit")?;
                let active: bool = meter.try_get("active")?;
                let service: String = meter.try_get("service_code")?;
                let correct_unit = (service == "WATER" && unit == "M3")
                    || (service == "ELECTRICITY" && unit == "KWH");
                if !active || !correct_unit {
                    let result = rejected(
                        "METER_NOT_BILLABLE",
                        "use_an_active_meter_with_a_matching_service_unit",
                    );
                    persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                    return Ok(result);
                }
                if correction {
                    let parent_id = reading
                        .corrected_from_id
                        .expect("validated correction parent");
                    let parent = sqlx::query(
                        "SELECT r.meter_id, r.period, r.previous, r.current FROM meter_readings r
                         JOIN meters m ON m.id = r.meter_id JOIN service_accounts sa ON sa.id = m.service_account_id
                         JOIN customers c ON c.id = sa.customer_id
                         WHERE r.id = $1 AND c.organization_id = $2 FOR UPDATE OF r",
                    )
                    .bind(parent_id)
                    .bind(self.organization_id)
                    .fetch_optional(&mut **tx)
                    .await?;
                    let Some(parent) = parent else {
                        let result = rejected(
                            "METER_READING_PARENT_NOT_FOUND",
                            "sync_the_original_reading_before_its_correction",
                        );
                        persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                        return Ok(result);
                    };
                    let parent_meter: Uuid = parent.try_get("meter_id")?;
                    let parent_period: String = parent.try_get("period")?;
                    let parent_previous: Decimal = parent.try_get("previous")?;
                    let parent_invoiced: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM invoice_meter_readings WHERE meter_reading_id = $1)",
                    )
                    .bind(parent_id)
                    .fetch_one(&mut **tx)
                    .await?;
                    let has_child: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM meter_readings WHERE corrected_from_id = $1)",
                    )
                    .bind(parent_id)
                    .fetch_one(&mut **tx)
                    .await?;
                    let next_previous: Option<Decimal> = sqlx::query_scalar(
                        "SELECT previous FROM meter_readings WHERE meter_id = $1 AND period > $2 ORDER BY period, created_at, id LIMIT 1",
                    )
                    .bind(reading.meter_id)
                    .bind(&reading.period)
                    .fetch_optional(&mut **tx)
                    .await?;
                    if parent_meter != reading.meter_id
                        || parent_period != reading.period
                        || parent_previous != reading.previous
                        || parent_invoiced
                        || has_child
                        || next_previous.is_some_and(|previous| previous != reading.current)
                    {
                        let result = conflict("METER_READING_CORRECTION_CONFLICT");
                        persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                        return Ok(result);
                    }
                } else {
                    if sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM meter_readings WHERE meter_id = $1 AND period = $2 AND corrected_from_id IS NULL)",
                    )
                    .bind(reading.meter_id)
                    .bind(&reading.period)
                    .fetch_one(&mut **tx)
                    .await?
                    {
                        let result = conflict("METER_READING_ALREADY_EXISTS");
                        persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                        return Ok(result);
                    }
                    let prior_current: Option<Decimal> = sqlx::query_scalar(
                        "SELECT current FROM meter_readings r WHERE meter_id = $1 AND period < $2
                         AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
                         ORDER BY period DESC, created_at DESC, id DESC LIMIT 1",
                    )
                    .bind(reading.meter_id)
                    .bind(&reading.period)
                    .fetch_optional(&mut **tx)
                    .await?;
                    if prior_current.is_some_and(|current| current != reading.previous) {
                        let result = conflict("METER_PREVIOUS_INDEX_CONFLICT");
                        persist_sync_result(tx, self.organization_id, event, &result, None).await?;
                        return Ok(result);
                    }
                }
                sqlx::query(
                    "INSERT INTO meter_readings
                     (id, meter_id, period, previous, current, measured_quantity, source, corrected_from_id, correction_reason)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(reading.id)
                .bind(reading.meter_id)
                .bind(&reading.period)
                .bind(reading.previous)
                .bind(reading.current)
                .bind(reading.measured_quantity)
                .bind(reading.source.trim())
                .bind(reading.corrected_from_id)
                .bind(&reading.correction_reason)
                .execute(&mut **tx)
                .await?;
                ("meter_readings", reading.id)
            }
            _ => unreachable!("only meter commands are dispatched here"),
        };
        let (accepted_revision, accepted_change) =
            if event.command_type == "SERVICE_ACCOUNT_UPDATED" {
                let payload: ServiceAccountUpdateSyncPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|error| {
                        sqlx::Error::Protocol(format!("invalid service account update: {error}"))
                    })?;
                (payload.account.revision, json!(payload.account))
            } else {
                (1, event.payload.clone())
            };
        let result = persist_sync_result(
            tx,
            self.organization_id,
            event,
            &SyncEventResult::Accepted {
                server_sequence: 0,
                server_entity_id: payload_id,
            },
            Some((entity, payload_id, accepted_revision, accepted_change)),
        )
        .await?;
        Ok(result)
    }
}

fn offline_role_allows_command(role: &str, command_type: &str) -> bool {
    match command_type {
        "CUSTOMER_CREATED"
        | "CUSTOMER_UPDATED"
        | "SERVICE_ACCOUNT_CREATED"
        | "SERVICE_ACCOUNT_UPDATED"
        | "METER_CREATED"
        | "METER_READING_CREATED"
        | "UTILITY_INVOICE_ISSUED"
        | "SOLAR_SALE_CREATED"
        | "SOLAR_CONTRACT_ACTIVATED" => {
            matches!(role, "ADMIN" | "BILLING_AGENT" | "SUPERVISOR")
        }
        "PAYMENT_RECORDED" | "PAYMENT_ALLOCATED" | "PAYMENT_REVERSED" => {
            matches!(role, "ADMIN" | "CASHIER" | "SUPERVISOR")
        }
        "INVOICE_VOIDED" | "CREDIT_NOTE_RECORDED" => {
            matches!(role, "ADMIN" | "SUPERVISOR")
        }
        "METER_READING_CORRECTED" => matches!(role, "ADMIN" | "SUPERVISOR"),
        _ => false,
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

fn postgres_timestamp_precision(value: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(value.timestamp_micros())
        .expect("a valid UTC timestamp remains valid at PostgreSQL precision")
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

impl CentralStore {
    pub async fn create_service_account(
        &self,
        customer_id: Uuid,
        input: &CreateServiceAccount,
        actor_id: Option<Uuid>,
    ) -> Result<ServiceAccount, MeterWriteError> {
        if !matches!(input.service_code.as_str(), "ELECTRICITY" | "WATER")
            || !matches!(
                input.connection_status.as_str(),
                "CONNECTED" | "NOT_CONNECTED" | "UNKNOWN"
            )
        {
            return Err(MeterWriteError::InvalidConfiguration);
        }
        let mut tx = self.pool.begin().await?;
        let customer_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND organization_id = $2)",
        )
        .bind(customer_id)
        .bind(self.organization_id)
        .fetch_one(&mut *tx)
        .await?;
        if !customer_exists {
            return Err(MeterWriteError::CustomerNotFound);
        }
        let id = Uuid::new_v4();
        let row = sqlx::query(
            "INSERT INTO service_accounts
             (id, customer_id, service_code, contract_ref, connection_status, activated_at, revision)
             VALUES ($1, $2, $3, $4, $5, $6, 1)
             RETURNING id, customer_id, service_code, contract_ref, connection_status, activated_at, revision",
        )
        .bind(id)
        .bind(customer_id)
        .bind(&input.service_code)
        .bind(&input.contract_ref)
        .bind(&input.connection_status)
        .bind(input.activated_at)
        .fetch_one(&mut *tx)
        .await?;
        let account = service_account_from_pg_row(&row)?;
        sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'service_accounts', $2, 1, $3)",
        )
        .bind(self.organization_id)
        .bind(id)
        .bind(json!(account))
        .execute(&mut *tx)
        .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "SERVICE_ACCOUNT_CREATED",
            id,
            json!({"service_code": account.service_code, "connection_status": account.connection_status}),
        )
        .await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn list_service_accounts(
        &self,
        customer_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, MeterWriteError> {
        let rows = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref,
                    sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.customer_id = $1 AND c.organization_id = $2
             ORDER BY sa.service_code, sa.id",
        )
        .bind(customer_id)
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(service_account_from_pg_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(MeterWriteError::from)
    }

    pub async fn get_service_account(
        &self,
        id: Uuid,
    ) -> Result<Option<ServiceAccount>, MeterWriteError> {
        let row = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref,
                    sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref()
            .map(service_account_from_pg_row)
            .transpose()
            .map_err(MeterWriteError::from)
    }

    pub async fn update_service_account(
        &self,
        id: Uuid,
        expected_revision: i64,
        input: &UpdateServiceAccount,
        actor_id: Option<Uuid>,
    ) -> Result<Option<ServiceAccount>, MeterWriteError> {
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
            return Err(MeterWriteError::InvalidConfiguration);
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT sa.id, sa.customer_id, sa.service_code, sa.contract_ref, sa.connection_status, sa.activated_at, sa.revision
             FROM service_accounts sa JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2 FOR UPDATE OF sa",
        )
        .bind(id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Err(MeterWriteError::ServiceAccountNotFound);
        };
        let current = service_account_from_pg_row(&row)?;
        if current.revision != expected_revision {
            return Err(MeterWriteError::ServiceAccountRevisionConflict);
        }
        let account = ServiceAccount {
            contract_ref: input.contract_ref.apply_to(current.contract_ref.clone()),
            connection_status: input
                .connection_status
                .clone()
                .unwrap_or(current.connection_status.clone()),
            activated_at: input.activated_at.apply_to(current.activated_at),
            revision: expected_revision + 1,
            ..current
        };
        sqlx::query("UPDATE service_accounts SET contract_ref = $1, connection_status = $2, activated_at = $3, revision = $4 WHERE id = $5 AND revision = $6")
            .bind(&account.contract_ref)
            .bind(&account.connection_status)
            .bind(account.activated_at)
            .bind(account.revision)
            .bind(id)
            .bind(expected_revision)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json) VALUES ($1, 'service_accounts', $2, $3, $4)")
            .bind(self.organization_id)
            .bind(id)
            .bind(account.revision)
            .bind(json!(account))
            .execute(&mut *tx)
            .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "SERVICE_ACCOUNT_UPDATED",
            id,
            json!({"revision": account.revision}),
        )
        .await?;
        tx.commit().await?;
        Ok(Some(account))
    }

    pub async fn create_meter(
        &self,
        service_account_id: Uuid,
        input: &CreateMeter,
        actor_id: Option<Uuid>,
    ) -> Result<Meter, MeterWriteError> {
        let serial = input.serial.trim();
        if serial.is_empty()
            || serial.chars().count() > 128
            || !matches!(input.unit.as_str(), "KWH" | "M3")
        {
            return Err(MeterWriteError::InvalidInput);
        }
        let mut tx = self.pool.begin().await?;
        let service_code: Option<String> = sqlx::query_scalar(
            "SELECT sa.service_code FROM service_accounts sa
             JOIN customers c ON c.id = sa.customer_id
             WHERE sa.id = $1 AND c.organization_id = $2 FOR UPDATE OF sa",
        )
        .bind(service_account_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        let service_code = service_code.ok_or(MeterWriteError::ServiceAccountNotFound)?;
        let expected_unit = match service_code.as_str() {
            "ELECTRICITY" => "KWH",
            "WATER" => "M3",
            _ => return Err(MeterWriteError::InvalidConfiguration),
        };
        if input.unit != expected_unit {
            return Err(MeterWriteError::InvalidConfiguration);
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO meters (id, service_account_id, serial, unit, active)
             VALUES ($1, $2, $3, $4, TRUE)",
        )
        .bind(id)
        .bind(service_account_id)
        .bind(serial)
        .bind(&input.unit)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                MeterWriteError::InvalidInput
            } else {
                MeterWriteError::Database(error)
            }
        })?;
        sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'meters', $2, 1, $3)",
        )
        .bind(self.organization_id)
        .bind(id)
        .bind(json!(Meter {
            id,
            service_account_id,
            serial: serial.to_owned(),
            unit: input.unit.clone(),
            active: true,
        }))
        .execute(&mut *tx)
        .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "METER_CREATED",
            id,
            json!({"serial": serial, "unit": input.unit}),
        )
        .await?;
        tx.commit().await?;
        Ok(Meter {
            id,
            service_account_id,
            serial: serial.to_owned(),
            unit: input.unit.clone(),
            active: true,
        })
    }

    pub async fn replace_meter(
        &self,
        meter_id: Uuid,
        input: &ReplaceMeter,
        actor_id: Option<Uuid>,
    ) -> Result<MeterReplacement, MeterWriteError> {
        let serial = input.serial.trim();
        let reason = input.reason.trim();
        if serial.is_empty()
            || serial.chars().count() > 128
            || reason.is_empty()
            || reason.chars().count() > 500
        {
            return Err(MeterWriteError::InvalidInput);
        }

        let mut tx = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT m.service_account_id, m.serial, m.unit, m.active
             FROM meters m JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.id = $1 AND c.organization_id = $2
             FOR UPDATE OF m",
        )
        .bind(meter_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(MeterWriteError::MeterNotFound)?;
        let service_account_id: Uuid = existing.try_get("service_account_id")?;
        let old_serial: String = existing.try_get("serial")?;
        let unit: String = existing.try_get("unit")?;
        let active: bool = existing.try_get("active")?;
        if !active {
            return Err(MeterWriteError::MeterNotActive);
        }

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meters WHERE service_account_id = $1 AND serial = $2)",
        )
        .bind(service_account_id)
        .bind(serial)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            return Err(MeterWriteError::InvalidInput);
        }

        let replacement_id = Uuid::new_v4();
        let retired = Meter {
            id: meter_id,
            service_account_id,
            serial: old_serial.clone(),
            unit: unit.clone(),
            active: false,
        };
        let replacement = Meter {
            id: replacement_id,
            service_account_id,
            serial: serial.to_owned(),
            unit,
            active: true,
        };

        sqlx::query("UPDATE meters SET active = FALSE WHERE id = $1 AND active = TRUE")
            .bind(meter_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO meters (id, service_account_id, serial, unit, active)
             VALUES ($1, $2, $3, $4, TRUE)",
        )
        .bind(replacement_id)
        .bind(service_account_id)
        .bind(serial)
        .bind(&replacement.unit)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                MeterWriteError::InvalidInput
            } else {
                MeterWriteError::Database(error)
            }
        })?;

        sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'meters', $2, 2, $3), ($1, 'meters', $4, 1, $5)",
        )
        .bind(self.organization_id)
        .bind(meter_id)
        .bind(json!(retired))
        .bind(replacement_id)
        .bind(json!(replacement))
        .execute(&mut *tx)
        .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "METER_REPLACED",
            meter_id,
            json!({"old_serial": old_serial, "replacement_meter_id": replacement_id, "replacement_serial": serial, "reason": reason}),
        )
        .await?;
        tx.commit().await?;

        Ok(MeterReplacement {
            retired_meter: retired,
            replacement_meter: replacement,
        })
    }

    pub async fn list_meters(
        &self,
        service_account_id: Uuid,
    ) -> Result<Vec<Meter>, MeterWriteError> {
        let rows = sqlx::query(
            "SELECT m.id, m.service_account_id, m.serial, m.unit, m.active
             FROM meters m JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.service_account_id = $1 AND c.organization_id = $2
             ORDER BY m.serial, m.id",
        )
        .bind(service_account_id)
        .bind(self.organization_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(meter_from_pg_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(MeterWriteError::from)
    }

    pub async fn list_meter_readings(
        &self,
        meter_id: Uuid,
        period: Option<&str>,
    ) -> Result<Vec<MeterReading>, MeterWriteError> {
        let rows = sqlx::query(
            "SELECT r.id, r.meter_id, r.period, r.previous, r.current,
                    r.measured_quantity, r.source, r.corrected_from_id, r.correction_reason
             FROM meter_readings r JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.meter_id = $1 AND c.organization_id = $2
               AND ($3::text IS NULL OR r.period = $3)
             ORDER BY r.period, CASE WHEN r.corrected_from_id IS NULL THEN 0 ELSE 1 END,
                      r.created_at, r.id",
        )
        .bind(meter_id)
        .bind(self.organization_id)
        .bind(period)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(meter_reading_from_pg_row)
            .collect::<Result<Vec<_>, _>>()
            .map_err(MeterWriteError::from)
    }

    pub async fn create_meter_reading(
        &self,
        meter_id: Uuid,
        input: &CreateMeterReading,
        actor_id: Option<Uuid>,
    ) -> Result<MeterReading, MeterWriteError> {
        validate_pg_reading_input(
            &input.period,
            &input.previous,
            &input.current,
            &input.source,
        )?;
        let mut tx = self.pool.begin().await?;
        let active: Option<bool> = sqlx::query_scalar(
            "SELECT m.active FROM meters m JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE m.id = $1 AND c.organization_id = $2 FOR UPDATE OF m",
        )
        .bind(meter_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?;
        if !active.ok_or(MeterWriteError::MeterNotFound)? {
            return Err(MeterWriteError::InvalidConfiguration);
        }
        let previous_current: Option<Decimal> = sqlx::query_scalar(
            "SELECT r.current FROM meter_readings r
             WHERE r.meter_id = $1 AND r.period < $2
               AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
             ORDER BY r.period DESC, r.created_at DESC, r.id DESC LIMIT 1",
        )
        .bind(meter_id)
        .bind(&input.period)
        .fetch_optional(&mut *tx)
        .await?;
        if previous_current.is_some_and(|value| value != input.previous) {
            return Err(MeterWriteError::PreviousIndexMismatch);
        }
        let id = Uuid::new_v4();
        let quantity = input.current - input.previous;
        let insert = sqlx::query(
            "INSERT INTO meter_readings
             (id, meter_id, period, previous, current, measured_quantity, source)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(meter_id)
        .bind(&input.period)
        .bind(input.previous)
        .bind(input.current)
        .bind(quantity)
        .bind(input.source.trim())
        .execute(&mut *tx)
        .await;
        if let Err(error) = insert {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                return Err(MeterWriteError::DuplicateReading);
            }
            return Err(MeterWriteError::Database(error));
        }
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
        let change = json!(reading);
        sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'meter_readings', $2, 1, $3)",
        )
        .bind(self.organization_id)
        .bind(id)
        .bind(change)
        .execute(&mut *tx)
        .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "METER_READING_CREATED",
            id,
            json!({"period": reading.period, "measured_quantity": reading.measured_quantity}),
        )
        .await?;
        tx.commit().await?;
        Ok(reading)
    }

    pub async fn correct_meter_reading(
        &self,
        reading_id: Uuid,
        input: &CorrectMeterReading,
        actor_id: Option<Uuid>,
    ) -> Result<MeterReading, MeterWriteError> {
        if input.reason.trim().is_empty() {
            return Err(MeterWriteError::CorrectionReasonRequired);
        }
        if input.reason.trim().chars().count() > 500 || input.source.trim().is_empty() {
            return Err(MeterWriteError::InvalidInput);
        }
        let mut tx = self.pool.begin().await?;
        let Some(row) = sqlx::query(
            "SELECT r.id, r.meter_id, r.period, r.previous, r.current,
                    r.measured_quantity, r.source, r.corrected_from_id, r.correction_reason
             FROM meter_readings r JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.id = $1 AND c.organization_id = $2 FOR UPDATE OF r",
        )
        .bind(reading_id)
        .bind(self.organization_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Err(MeterWriteError::MeterNotFound);
        };
        let target = meter_reading_from_pg_row(&row)?;
        let invoiced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM invoice_meter_readings WHERE meter_reading_id = $1)",
        )
        .bind(reading_id)
        .fetch_one(&mut *tx)
        .await?;
        if invoiced {
            return Err(MeterWriteError::ReadingAlreadyInvoiced);
        }
        let has_correction: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM meter_readings WHERE corrected_from_id = $1)",
        )
        .bind(reading_id)
        .fetch_one(&mut *tx)
        .await?;
        if has_correction {
            return Err(MeterWriteError::ReadingNotLatest);
        }
        if input.current < target.previous {
            return Err(MeterWriteError::InvalidIndex);
        }
        let next_period_previous: Option<Decimal> = sqlx::query_scalar(
            "SELECT previous FROM meter_readings
             WHERE meter_id = $1 AND period > $2
             ORDER BY period, created_at, id LIMIT 1",
        )
        .bind(target.meter_id)
        .bind(&target.period)
        .fetch_optional(&mut *tx)
        .await?;
        if next_period_previous.is_some_and(|value| value != input.current) {
            return Err(MeterWriteError::PreviousIndexMismatch);
        }
        let id = Uuid::new_v4();
        let quantity = input.current - target.previous;
        let corrected = MeterReading {
            id,
            meter_id: target.meter_id,
            period: target.period,
            previous: target.previous,
            current: input.current,
            measured_quantity: quantity,
            source: input.source.trim().to_owned(),
            corrected_from_id: Some(reading_id),
            correction_reason: Some(input.reason.trim().to_owned()),
        };
        sqlx::query(
            "INSERT INTO meter_readings
             (id, meter_id, period, previous, current, measured_quantity, source,
              corrected_from_id, correction_reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(corrected.meter_id)
        .bind(&corrected.period)
        .bind(corrected.previous)
        .bind(corrected.current)
        .bind(corrected.measured_quantity)
        .bind(&corrected.source)
        .bind(reading_id)
        .bind(&corrected.correction_reason)
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|database| database.is_unique_violation())
            {
                MeterWriteError::ReadingNotLatest
            } else {
                MeterWriteError::Database(error)
            }
        })?;
        sqlx::query(
            "INSERT INTO central_changes (organization_id, entity, entity_id, revision, change_json)
             VALUES ($1, 'meter_readings', $2, 2, $3)",
        )
        .bind(self.organization_id)
        .bind(id)
        .bind(json!(corrected))
        .execute(&mut *tx)
        .await?;
        audit_meter_mutation(
            &mut tx,
            actor_id,
            "METER_READING_CORRECTED",
            id,
            json!({"corrected_from_id": reading_id, "reason": input.reason.trim()}),
        )
        .await?;
        tx.commit().await?;
        Ok(corrected)
    }
}

async fn audit_meter_mutation(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Option<Uuid>,
    event_type: &str,
    subject_id: Uuid,
    metadata: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events
         (id, actor, event_type, subject_id, occurred_at, redacted_metadata)
         VALUES ($1, $2, $3, $4, now(), $5)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_id)
    .bind(event_type)
    .bind(subject_id)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_pg_reading_input(
    period: &str,
    previous: &Decimal,
    current: &Decimal,
    source: &str,
) -> Result<(), MeterWriteError> {
    if chrono::NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d").is_err()
        || source.trim().is_empty()
        || source.trim().chars().count() > 64
    {
        return Err(MeterWriteError::InvalidInput);
    }
    if previous.is_sign_negative() || current < previous {
        return Err(MeterWriteError::InvalidIndex);
    }
    Ok(())
}

fn service_account_from_pg_row(row: &sqlx::postgres::PgRow) -> Result<ServiceAccount, sqlx::Error> {
    Ok(ServiceAccount {
        id: row.try_get("id")?,
        customer_id: row.try_get("customer_id")?,
        service_code: row.try_get("service_code")?,
        contract_ref: row.try_get("contract_ref")?,
        connection_status: row.try_get("connection_status")?,
        activated_at: row.try_get("activated_at")?,
        revision: row.try_get("revision")?,
    })
}

fn meter_from_pg_row(row: &sqlx::postgres::PgRow) -> Result<Meter, sqlx::Error> {
    Ok(Meter {
        id: row.try_get("id")?,
        service_account_id: row.try_get("service_account_id")?,
        serial: row.try_get("serial")?,
        unit: row.try_get("unit")?,
        active: row.try_get("active")?,
    })
}

async fn load_invoice_meter_readings_in_tx(
    tx: &mut sqlx::PgConnection,
    organization_id: Uuid,
    customer_id: Uuid,
    period: &str,
    refs: &UtilityMeterReadingRefs,
) -> Result<(Decimal, Decimal), UtilityIssueError> {
    if refs.electricity_reading_id == refs.water_reading_id {
        return Err(UtilityIssueError::MeterReadingInvalid);
    }
    let mut quantities = [Decimal::ZERO; 2];
    for (index, (reading_id, service, unit)) in [
        (refs.electricity_reading_id, "ELECTRICITY", "KWH"),
        (refs.water_reading_id, "WATER", "M3"),
    ]
    .into_iter()
    .enumerate()
    {
        let quantity = sqlx::query_scalar::<_, Decimal>(
            "SELECT r.measured_quantity FROM meter_readings r
             JOIN meters m ON m.id = r.meter_id
             JOIN service_accounts sa ON sa.id = m.service_account_id
             JOIN customers c ON c.id = sa.customer_id
             WHERE r.id = $1 AND c.id = $2 AND c.organization_id = $3
               AND sa.service_code = $4 AND m.unit = $5 AND r.period = $6
               AND NOT EXISTS (SELECT 1 FROM meter_readings child WHERE child.corrected_from_id = r.id)
               AND NOT EXISTS (SELECT 1 FROM invoice_meter_readings imr WHERE imr.meter_reading_id = r.id)
             FOR UPDATE OF r",
        )
        .bind(reading_id)
        .bind(customer_id)
        .bind(organization_id)
        .bind(service)
        .bind(unit)
        .bind(period)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(UtilityIssueError::MeterReadingInvalid)?;
        quantities[index] = quantity;
    }
    Ok((quantities[0], quantities[1]))
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

fn meter_reading_from_pg_row(row: &sqlx::postgres::PgRow) -> Result<MeterReading, sqlx::Error> {
    Ok(MeterReading {
        id: row.try_get("id")?,
        meter_id: row.try_get("meter_id")?,
        period: row.try_get("period")?,
        previous: row.try_get("previous")?,
        current: row.try_get("current")?,
        measured_quantity: row.try_get("measured_quantity")?,
        source: row.try_get("source")?,
        corrected_from_id: row.try_get("corrected_from_id")?,
        correction_reason: row.try_get("correction_reason")?,
    })
}

fn customer_from_row(row: &sqlx::postgres::PgRow) -> Result<Customer, sqlx::Error> {
    let contact_fields: serde_json::Value = row.try_get("contact_fields")?;
    Ok(Customer {
        id: row.try_get("id")?,
        organization_id: row.try_get("organization_id")?,
        customer_code: row.try_get("customer_code")?,
        legal_name: row.try_get("legal_name")?,
        customer_category: row.try_get("customer_category")?,
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

/// Validates an allocation against the outstanding value of one installment.
/// The schedule row lock serializes writers; its receivable lock is acquired by
/// callers first in stable UUID order. Reversals reduce the net allocation in
/// the same proportion used by the installment-plan projection.
async fn validate_installment_allocation(
    tx: &mut sqlx::PgConnection,
    schedule_id: Uuid,
    receivable_id: Uuid,
    requested: Decimal,
) -> Result<Option<bool>, sqlx::Error> {
    let amount_due = sqlx::query_scalar::<_, Decimal>(
        "SELECT s.amount_due
         FROM installment_schedule s
         JOIN installment_plans p ON p.id = s.plan_id
         WHERE s.id = $1 AND p.receivable_id = $2
         FOR UPDATE OF s",
    )
    .bind(schedule_id)
    .bind(receivable_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(amount_due) = amount_due else {
        return Ok(None);
    };
    let already_paid = sqlx::query_scalar::<_, Decimal>(
        "SELECT COALESCE(SUM(
             pa.allocated_amount * GREATEST(
                 p.amount_received - COALESCE((
                     SELECT SUM(pr.amount)
                     FROM payment_reversals pr
                     WHERE pr.payment_id = p.id
                 ), 0),
                 0
             ) / NULLIF(p.amount_received, 0)
         ), 0)
         FROM payment_allocations pa
         JOIN payments p ON p.id = pa.payment_id
         WHERE pa.schedule_id = $1
           AND pa.receivable_id = $2
           AND p.status = 'CENTRAL_ACCEPTED'",
    )
    .bind(schedule_id)
    .bind(receivable_id)
    .fetch_one(&mut *tx)
    .await?;
    Ok(Some(already_paid + requested > amount_due))
}

async fn receivable_has_installment_plan(
    tx: &mut sqlx::PgConnection,
    receivable_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM installment_plans WHERE receivable_id = $1)")
        .bind(receivable_id)
        .fetch_one(&mut *tx)
        .await
}

async fn load_invoice_identity_snapshot(
    tx: &mut sqlx::PgConnection,
    organization_id: Uuid,
    customer_id: Uuid,
) -> Result<InvoiceIdentitySnapshot, sqlx::Error> {
    let row = sqlx::query(
        "SELECT o.legal_name AS organization_name, c.customer_code, c.legal_name,
                c.contact_fields::text AS contact_fields_json
         FROM organizations o JOIN customers c ON c.organization_id = o.id
         WHERE o.id = $1 AND c.id = $2",
    )
    .bind(organization_id)
    .bind(customer_id)
    .fetch_one(&mut *tx)
    .await?;
    let contacts: serde_json::Value = serde_json::from_str(
        &row.try_get::<String, _>("contact_fields_json")?,
    )
    .map_err(|error| sqlx::Error::Protocol(format!("invalid customer contacts: {error}")))?;
    Ok(InvoiceIdentitySnapshot {
        organization_name: row.try_get("organization_name")?,
        customer_code: row.try_get("customer_code")?,
        legal_name: row.try_get("legal_name")?,
        address: contacts
            .get("address")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        phone: contacts
            .get("phone")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod offline_role_tests {
    use super::{offline_role_allows_command, AuthError, CentralStore};

    #[test]
    fn password_policy_accepts_eight_characters_and_rejects_seven() {
        assert!(CentralStore::hash_user_password("Eight!88").is_ok());
        assert!(matches!(
            CentralStore::hash_user_password("Seven!7"),
            Err(AuthError::PasswordHash(message)) if message.contains("8 characters")
        ));
    }

    #[test]
    fn offline_grants_cannot_expand_actor_business_roles() {
        assert!(offline_role_allows_command("CASHIER", "PAYMENT_RECORDED"));
        assert!(offline_role_allows_command("CASHIER", "PAYMENT_ALLOCATED"));
        assert!(!offline_role_allows_command(
            "CASHIER",
            "UTILITY_INVOICE_ISSUED"
        ));
        assert!(!offline_role_allows_command(
            "CASHIER",
            "CREDIT_NOTE_RECORDED"
        ));
        assert!(offline_role_allows_command(
            "BILLING_AGENT",
            "UTILITY_INVOICE_ISSUED"
        ));
        assert!(!offline_role_allows_command(
            "BILLING_AGENT",
            "PAYMENT_RECORDED"
        ));
        assert!(!offline_role_allows_command(
            "AUDITOR_READONLY",
            "CUSTOMER_UPDATED"
        ));
        assert!(!offline_role_allows_command("ADMIN", "UNSUPPORTED_COMMAND"));
    }
}
