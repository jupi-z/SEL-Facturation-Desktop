use chrono::{DateTime, NaiveDate, Timelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const CUSTOMER_PATCH_FORMAT_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub code: String,
    pub detail: String,
    pub request_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Customer {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub customer_code: String,
    pub legal_name: String,
    /// `None` is retained for older records and must be resolved before billing.
    #[serde(default)]
    pub customer_category: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub status: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCustomer {
    pub customer_code: String,
    pub legal_name: String,
    #[serde(default)]
    pub customer_category: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PatchValue<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<T> PatchValue<T> {
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl<T: Clone> PatchValue<T> {
    pub fn apply_to(&self, current: Option<T>) -> Option<T> {
        match self {
            Self::Missing => current,
            Self::Null => None,
            Self::Value(value) => Some(value.clone()),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for PatchValue<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            Some(value) => Self::Value(value),
            None => Self::Null,
        })
    }
}

impl<T: Serialize> Serialize for PatchValue<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Missing | Self::Null => serializer.serialize_none(),
            Self::Value(value) => value.serialize(serializer),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateCustomer {
    pub legal_name: Option<String>,
    #[serde(default, skip_serializing_if = "PatchValue::is_missing")]
    pub customer_category: PatchValue<String>,
    #[serde(default, skip_serializing_if = "PatchValue::is_missing")]
    pub address: PatchValue<String>,
    #[serde(default, skip_serializing_if = "PatchValue::is_missing")]
    pub phone: PatchValue<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceAccount {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub service_code: String,
    pub contract_ref: Option<String>,
    pub connection_status: String,
    pub activated_at: Option<NaiveDate>,
    #[serde(default = "default_revision")]
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateServiceAccount {
    pub service_code: String,
    pub contract_ref: Option<String>,
    pub connection_status: String,
    pub activated_at: Option<NaiveDate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateServiceAccount {
    #[serde(default, skip_serializing_if = "PatchValue::is_missing")]
    pub contract_ref: PatchValue<String>,
    pub connection_status: Option<String>,
    #[serde(default, skip_serializing_if = "PatchValue::is_missing")]
    pub activated_at: PatchValue<NaiveDate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAccountUpdateSyncPayload {
    pub expected_revision: i64,
    pub account: ServiceAccount,
}

fn default_revision() -> i64 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Meter {
    pub id: Uuid,
    pub service_account_id: Uuid,
    pub serial: String,
    pub unit: String,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateMeter {
    pub serial: String,
    pub unit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplaceMeter {
    pub serial: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MeterReplacement {
    pub retired_meter: Meter,
    pub replacement_meter: Meter,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MeterReading {
    pub id: Uuid,
    pub meter_id: Uuid,
    pub period: String,
    pub previous: Decimal,
    pub current: Decimal,
    pub measured_quantity: Decimal,
    pub source: String,
    pub corrected_from_id: Option<Uuid>,
    pub correction_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MeterReadingPreview {
    pub meter_id: Uuid,
    pub period: String,
    pub previous: Decimal,
    pub current: Decimal,
    pub measured_quantity: Decimal,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateMeterReading {
    pub period: String,
    pub previous: Decimal,
    pub current: Decimal,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CorrectMeterReading {
    pub current: Decimal,
    pub source: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityServiceInput {
    pub quantity: Decimal,
    pub unit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityBillingRequest {
    pub customer_id: Uuid,
    pub billing_period: String,
    pub electricity: UtilityServiceInput,
    pub water: UtilityServiceInput,
    pub water_zero_confirmed: bool,
    pub currency: String,
    pub tariff_revision: String,
    pub due_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meter_readings: Option<UtilityMeterReadingRefs>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityMeterReadingRefs {
    pub electricity_reading_id: Uuid,
    pub water_reading_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvoiceLineResponse {
    pub service_code: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityBillingResponse {
    pub invoice_id: Uuid,
    pub receivable_id: Option<Uuid>,
    pub invoice_number: Option<String>,
    pub status: String,
    #[serde(default)]
    pub billing_period: String,
    #[serde(default)]
    pub due_date: Option<NaiveDate>,
    pub currency: String,
    pub lines: Vec<InvoiceLineResponse>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
    pub snapshot_sha256: String,
}

#[derive(Serialize)]
struct CanonicalInvoiceLine {
    service_code: String,
    quantity: String,
    unit: String,
    unit_price: Option<String>,
    subtotal: String,
    tax: String,
    total: String,
}

#[derive(Serialize)]
struct CanonicalUtilitySnapshot {
    schema_version: u32,
    invoice_id: Uuid,
    receivable_id: Option<Uuid>,
    currency: String,
    lines: Vec<CanonicalInvoiceLine>,
    subtotal: String,
    tax: String,
    total: String,
}

/// Returns the canonical decimal representation used by invoice snapshots.
///
/// PostgreSQL `NUMERIC(20,6)` values are returned with six fractional digits,
/// while SQLite may return the same value with the scale originally written by
/// the caller. Six fixed digits make the hash independent of that storage
/// detail while preserving the database precision used by both stores.
fn canonical_decimal(value: Decimal) -> String {
    format!("{value:.6}")
}

/// Hashes the immutable financial portion of a utility invoice.
///
/// Synchronisation lifecycle fields (`status`, `invoice_number` and the hash
/// itself) are intentionally excluded. This lets a locally issued invoice be
/// confirmed centrally without manufacturing a financial conflict.
pub fn utility_snapshot_sha256(
    response: &UtilityBillingResponse,
) -> Result<String, serde_json::Error> {
    let mut lines: Vec<_> = response
        .lines
        .iter()
        .map(|line| CanonicalInvoiceLine {
            service_code: line.service_code.clone(),
            quantity: canonical_decimal(line.quantity),
            unit: line.unit.clone(),
            unit_price: line.unit_price.map(canonical_decimal),
            subtotal: canonical_decimal(line.subtotal),
            tax: canonical_decimal(line.tax),
            total: canonical_decimal(line.total),
        })
        .collect();
    // The line sequence is presentation-level data. Sort it in the canonical
    // representation so local and central stores hash the same invoice even
    // if a serializer or projection returns the two service lines in a
    // different order.
    lines.sort_by(|left, right| {
        left.service_code
            .cmp(&right.service_code)
            .then_with(|| left.unit.cmp(&right.unit))
    });
    let canonical = CanonicalUtilitySnapshot {
        schema_version: DOCUMENT_SCHEMA_VERSION,
        invoice_id: response.invoice_id,
        receivable_id: response.receivable_id,
        currency: response.currency.clone(),
        lines,
        subtotal: canonical_decimal(response.subtotal),
        tax: canonical_decimal(response.tax),
        total: canonical_decimal(response.total),
    };
    let bytes = serde_json::to_vec(&canonical)?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_qr_is_versioned_self_contained_and_excludes_customer_contact_data() {
        let invoice = UtilityBillingResponse {
            invoice_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            receivable_id: Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()),
            invoice_number: Some("SEL-20261112-143025".into()),
            status: "CENTRAL_ACCEPTED".into(),
            billing_period: "2026-10".into(),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()),
            currency: "USD".into(),
            lines: vec![InvoiceLineResponse {
                service_code: "COMMUNICATION".into(),
                quantity: Decimal::ONE,
                unit: "FORFAIT".into(),
                unit_price: Some(Decimal::new(10, 0)),
                subtotal: Decimal::new(10, 0),
                tax: Decimal::new(1, 0),
                total: Decimal::new(11, 0),
            }],
            subtotal: Decimal::new(10, 0),
            tax: Decimal::ONE,
            total: Decimal::new(11, 0),
            snapshot_sha256: "snapshot-hash".into(),
        };
        let identity = InvoiceIdentitySnapshot {
            organization_name: "Société d’Énergie du Lualaba".into(),
            customer_code: "SEL-C-0001".into(),
            legal_name: "Client".into(),
            address: Some("private address".into()),
            phone: Some("private phone".into()),
        };
        let payload: serde_json::Value = serde_json::from_str(&invoice_qr_payload(
            &invoice,
            &identity,
            Some("2026-11-12T14:30:25Z"),
        ))
        .unwrap();

        assert_eq!(payload["schema"], "SEL-INVOICE");
        assert_eq!(payload["version"], 1);
        assert_eq!(payload["issuer"]["code"], "SEL");
        assert_eq!(payload["issuer"]["name"], "Société d’Énergie du Lualaba");
        assert_eq!(payload["invoice"]["id"], invoice.invoice_id.to_string());
        assert_eq!(payload["invoice"]["reference"], "SEL-20261112-143025");
        assert_eq!(payload["invoice"]["period"], "2026-10");
        assert_eq!(payload["customer"]["code"], "SEL-C-0001");
        assert_eq!(payload["lines"][0]["service_code"], "COMMUNICATION");
        assert_eq!(payload["totals"]["total"], "11");
        assert_eq!(
            payload["notice"],
            "Identification interne SEL; sans authentification fiscale"
        );
        let encoded = payload.to_string();
        assert!(!encoded.contains("private address"));
        assert!(!encoded.contains("private phone"));
    }

    #[test]
    fn utility_snapshot_hash_is_shared_and_ignores_sync_status() {
        let mut pending = UtilityBillingResponse {
            invoice_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            receivable_id: Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()),
            invoice_number: None,
            status: "LOCAL_PENDING".into(),
            billing_period: String::new(),
            due_date: None,
            currency: "USD".into(),
            lines: vec![InvoiceLineResponse {
                service_code: "ELECTRICITY".into(),
                quantity: Decimal::new(250, 0),
                unit: "KWH".into(),
                unit_price: Some(Decimal::new(95, 3)),
                subtotal: Decimal::new(2375, 2),
                tax: Decimal::new(38, 2),
                total: Decimal::new(2413, 2),
            }],
            subtotal: Decimal::new(2375, 2),
            tax: Decimal::new(38, 2),
            total: Decimal::new(2413, 2),
            snapshot_sha256: String::new(),
        };
        let accepted = UtilityBillingResponse {
            status: "CENTRAL_ACCEPTED".into(),
            ..pending.clone()
        };

        let pending_hash = utility_snapshot_sha256(&pending).unwrap();
        let accepted_hash = utility_snapshot_sha256(&accepted).unwrap();

        assert_eq!(pending_hash, accepted_hash);
        assert_eq!(pending_hash.len(), 64);
        pending.snapshot_sha256 = pending_hash.clone();
        assert_eq!(pending.snapshot_sha256, pending_hash);
    }

    #[test]
    fn utility_snapshot_hash_ignores_decimal_storage_scale() {
        let base = UtilityBillingResponse {
            invoice_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            receivable_id: Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()),
            invoice_number: None,
            status: "LOCAL_PENDING".into(),
            billing_period: String::new(),
            due_date: None,
            currency: "USD".into(),
            lines: vec![InvoiceLineResponse {
                service_code: "ELECTRICITY".into(),
                quantity: Decimal::new(250, 0),
                unit: "KWH".into(),
                unit_price: Some(Decimal::new(95, 3)),
                subtotal: Decimal::new(2375, 2),
                tax: Decimal::new(38, 2),
                total: Decimal::new(2413, 2),
            }],
            subtotal: Decimal::new(2375, 2),
            tax: Decimal::new(38, 2),
            total: Decimal::new(2413, 2),
            snapshot_sha256: String::new(),
        };
        let scaled = UtilityBillingResponse {
            lines: vec![InvoiceLineResponse {
                quantity: Decimal::new(250000, 3),
                unit_price: Some(Decimal::new(95000, 6)),
                subtotal: Decimal::new(2375000, 5),
                tax: Decimal::new(3800, 4),
                total: Decimal::new(2413000, 5),
                ..base.lines[0].clone()
            }],
            subtotal: Decimal::new(2375000, 5),
            tax: Decimal::new(3800, 4),
            total: Decimal::new(2413000, 5),
            ..base.clone()
        };

        assert_eq!(
            utility_snapshot_sha256(&base).unwrap(),
            utility_snapshot_sha256(&scaled).unwrap()
        );
    }

    #[test]
    fn utility_snapshot_hash_ignores_service_line_order() {
        let electricity = InvoiceLineResponse {
            service_code: "ELECTRICITY".into(),
            quantity: Decimal::new(100, 0),
            unit: "KWH".into(),
            unit_price: Some(Decimal::new(10, 2)),
            subtotal: Decimal::new(10, 0),
            tax: Decimal::ZERO,
            total: Decimal::new(10, 0),
        };
        let water = InvoiceLineResponse {
            service_code: "WATER".into(),
            quantity: Decimal::ZERO,
            unit: "M3".into(),
            unit_price: None,
            subtotal: Decimal::ZERO,
            tax: Decimal::ZERO,
            total: Decimal::ZERO,
        };
        let ordered = UtilityBillingResponse {
            invoice_id: Uuid::new_v4(),
            receivable_id: None,
            invoice_number: None,
            status: "LOCAL_PENDING".into(),
            billing_period: String::new(),
            due_date: None,
            currency: "USD".into(),
            lines: vec![electricity.clone(), water.clone()],
            subtotal: Decimal::new(10, 0),
            tax: Decimal::ZERO,
            total: Decimal::new(10, 0),
            snapshot_sha256: String::new(),
        };
        let reversed = UtilityBillingResponse {
            lines: vec![water, electricity],
            ..ordered.clone()
        };

        assert_eq!(
            utility_snapshot_sha256(&ordered).unwrap(),
            utility_snapshot_sha256(&reversed).unwrap()
        );
    }
}

/// Versioned offline payload carrying the client-generated identifiers that
/// must remain stable when PostgreSQL accepts the command after a retry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityInvoiceSyncPayload {
    pub request: UtilityBillingRequest,
    pub invoice_id: Uuid,
    pub receivable_id: Uuid,
    #[serde(default)]
    pub identity_snapshot: Option<InvoiceIdentitySnapshot>,
}

/// Non-financial identity data captured when the document is issued. This is
/// deliberately excluded from the invoice's financial SHA-256 canonical form.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct InvoiceIdentitySnapshot {
    pub organization_name: String,
    pub customer_code: String,
    pub legal_name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentAllocationRequest {
    pub receivable_id: Uuid,
    pub amount: Decimal,
    #[serde(default)]
    pub schedule_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecordPaymentRequest {
    pub customer_id: Uuid,
    pub received_amount: Decimal,
    pub currency: String,
    pub method: String,
    pub occurred_at: DateTime<Utc>,
    pub allocations: Vec<PaymentAllocationRequest>,
}

/// Append-only financial reversal for an accepted payment.
///
/// The actor is deliberately not part of this payload: the API resolves it
/// from the authenticated request context and the storage layer verifies that
/// the user is active in the same organization as the payment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReversePaymentRequest {
    pub amount: Decimal,
    pub reason: String,
}

/// Append-only credit applied to an existing receivable. A credit reduces the
/// balance without deleting or rewriting the original invoice or payment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreditNoteRequest {
    pub amount: Decimal,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreditNoteSyncPayload {
    pub receivable_id: Uuid,
    pub credit_note_id: Uuid,
    pub amount: Decimal,
    pub reason: String,
    pub actor_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VoidInvoiceRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvoiceVoidSyncPayload {
    pub invoice_id: Uuid,
    pub void_id: Uuid,
    pub reason: String,
    pub actor_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentSyncPayload {
    pub request: RecordPaymentRequest,
    pub payment_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentAllocationSyncPayload {
    pub payment_id: Uuid,
    pub allocations: Vec<PaymentAllocationRequest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentReversalSyncPayload {
    pub payment_id: Uuid,
    pub reversal_id: Uuid,
    pub amount: Decimal,
    pub reason: String,
    pub actor_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstallmentRequest {
    pub sequence: u32,
    pub due_date: NaiveDate,
    pub amount: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActivateSolarContractRequest {
    pub sale_id: Uuid,
    pub receivable_id: Uuid,
    pub schedule: Vec<InstallmentRequest>,
    pub currency: String,
    pub approved_terms_revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncEnvelope {
    pub protocol_version: u32,
    pub event_id: Uuid,
    pub device_id: Uuid,
    pub organization_id: Uuid,
    pub actor_id: Uuid,
    pub command_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_expected_version: i64,
    pub occurred_at: DateTime<Utc>,
    pub offline_grant_id: Uuid,
    pub payload: serde_json::Value,
    pub payload_sha256: String,
    pub client_schema_version: u32,
}

/// Canonical, versioned bytes signed by the central authority for an offline
/// authorization grant. The exact field order is part of the protocol; never
/// sign a database JSON rendering whose key order may vary between stores.
pub fn offline_grant_canonical_bytes(
    grant_id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    scopes: &serde_json::Value,
    limits: &serde_json::Value,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(Serialize)]
    struct Grant<'a> {
        protocol_version: u32,
        grant_id: Uuid,
        user_id: Uuid,
        device_id: Uuid,
        scopes: &'a serde_json::Value,
        limits: &'a serde_json::Value,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    }
    let issued_at = issued_at
        .with_nanosecond(issued_at.timestamp_subsec_micros() * 1_000)
        .expect("microsecond timestamp is valid");
    let expires_at = expires_at
        .with_nanosecond(expires_at.timestamp_subsec_micros() * 1_000)
        .expect("microsecond timestamp is valid");
    serde_json::to_vec(&Grant {
        protocol_version: PROTOCOL_VERSION,
        grant_id,
        user_id,
        device_id,
        scopes,
        limits,
        issued_at,
        expires_at,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncPushRequest {
    pub device_id: Uuid,
    pub events: Vec<SyncEnvelope>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncEventResult {
    Accepted {
        server_sequence: i64,
        server_entity_id: Uuid,
    },
    Duplicate {
        original: Box<SyncEventResult>,
    },
    Conflict {
        reason: String,
        central_revision: Option<i64>,
    },
    Rejected {
        code: String,
        required_action: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncPushResponse {
    pub results: Vec<SyncEventResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SyncPullResponse {
    pub changes: Vec<serde_json::Value>,
    pub next_cursor: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogSnapshotPayload {
    pub schema_version: u32,
    pub products: Vec<serde_json::Value>,
    pub tariffs: Vec<serde_json::Value>,
    pub tax_profiles: Vec<serde_json::Value>,
    pub fx_rates: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub communication_charges: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogSnapshot {
    pub payload: CatalogSnapshotPayload,
    pub sha256: String,
}

pub fn catalog_snapshot_sha256(payload: &CatalogSnapshotPayload) -> String {
    let canonical = serde_json::to_vec(payload).expect("catalog snapshot payload is serializable");
    hex::encode(Sha256::digest(canonical))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateSolarSaleRequest {
    pub customer_id: Uuid,
    pub product_code: String,
    pub principal: Decimal,
    pub currency: String,
    pub terms_revision: String,
}

/// Audited replacement of a solar sale whose immutable financial snapshot is
/// incorrect. The original rows remain in the ledger and are balanced by
/// reversal/credit entries inside one central transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarSaleCorrectionRequest {
    pub due_date: NaiveDate,
    pub approved_terms_revision: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateSolarQuoteRequest {
    pub product_code: String,
    pub currency: String,
    pub terms_revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarQuoteResponse {
    pub product_code: String,
    #[serde(default)]
    pub subtotal: Decimal,
    #[serde(default)]
    pub tax_rate: Decimal,
    #[serde(default)]
    pub tax_amount: Decimal,
    #[serde(default)]
    pub total: Decimal,
    pub principal: Decimal,
    pub currency: String,
    pub terms_revision: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarTaxSnapshot {
    pub schema_version: u32,
    pub service_code: String,
    pub tax_profile_id: Uuid,
    pub jurisdiction: String,
    pub base: String,
    pub rate: Decimal,
    pub amount: Decimal,
    pub source: String,
    pub approval_ref: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarSaleResponse {
    pub sale_id: Uuid,
    pub receivable_id: Uuid,
    pub customer_id: Uuid,
    #[serde(default)]
    pub subtotal: Decimal,
    #[serde(default)]
    pub tax_rate: Decimal,
    #[serde(default)]
    pub tax_amount: Decimal,
    #[serde(default)]
    pub total: Decimal,
    pub principal: Decimal,
    pub currency: String,
    pub status: String,
    #[serde(default)]
    pub product_code: Option<String>,
    #[serde(default)]
    pub tax_snapshot: Option<SolarTaxSnapshot>,
}

/// Stable local/central identifier for one solar installment row.
pub fn solar_schedule_id(plan_id: Uuid, sequence: u32) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("SEL:SOLAR_SCHEDULE:{plan_id}:{sequence}").as_bytes(),
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarSaleSyncPayload {
    pub request: CreateSolarSaleRequest,
    pub sale_id: Uuid,
    pub receivable_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarContractSyncPayload {
    pub request: ActivateSolarContractRequest,
    pub contract_id: Uuid,
    pub installment_plan_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentData {
    pub document_schema_version: u32,
    pub template_version: String,
    pub document_reference: String,
    pub invoice: UtilityBillingResponse,
    #[serde(default)]
    pub identity_snapshot: InvoiceIdentitySnapshot,
    #[serde(default)]
    pub issued_at: Option<String>,
    pub qr_internal_reference: String,
    /// Compact versioned payload encoded directly in the client-generated QR.
    #[serde(default)]
    pub qr_payload: Option<String>,
    pub origin: String,
}

/// Builds the versioned, self-contained payload encoded in an invoice QR.
/// Only the subscriber code is included; personal contact details are excluded.
pub fn invoice_qr_payload(
    invoice: &UtilityBillingResponse,
    identity: &InvoiceIdentitySnapshot,
    issued_at: Option<&str>,
) -> String {
    let issuer_name = if identity.organization_name.is_empty() {
        "Société d’Énergie du Lualaba"
    } else {
        identity.organization_name.as_str()
    };
    let payload = serde_json::json!({
        "schema": "SEL-INVOICE",
        "version": 1,
        "issuer": { "code": "SEL", "name": issuer_name },
        "invoice": {
            "id": invoice.invoice_id,
            "reference": invoice.invoice_number,
            "issued_at": issued_at,
            "period": invoice.billing_period,
            "due_date": invoice.due_date
        },
        "customer": { "code": identity.customer_code },
        "currency": invoice.currency,
        "lines": invoice.lines,
        "totals": {
            "subtotal": invoice.subtotal,
            "tax": invoice.tax,
            "total": invoice.total
        },
        "notice": "Identification interne SEL; sans authentification fiscale"
    });
    serde_json::to_string(&payload).expect("invoice QR payload is JSON serializable")
}
