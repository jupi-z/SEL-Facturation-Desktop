use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const DOCUMENT_SCHEMA_VERSION: u32 = 1;

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
    pub address: Option<String>,
    pub phone: Option<String>,
    pub status: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateCustomer {
    pub customer_code: String,
    pub legal_name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateCustomer {
    pub legal_name: Option<String>,
    pub address: Option<String>,
    pub phone: Option<String>,
    pub status: Option<String>,
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
    pub currency: String,
    pub lines: Vec<InvoiceLineResponse>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
    pub snapshot_sha256: String,
}

#[derive(Serialize)]
struct CanonicalUtilitySnapshot<'a> {
    schema_version: u32,
    invoice_id: Uuid,
    receivable_id: Option<Uuid>,
    currency: &'a str,
    lines: &'a [InvoiceLineResponse],
    subtotal: Decimal,
    tax: Decimal,
    total: Decimal,
}

/// Hashes the immutable financial portion of a utility invoice.
///
/// Synchronisation lifecycle fields (`status`, `invoice_number` and the hash
/// itself) are intentionally excluded. This lets a locally issued invoice be
/// confirmed centrally without manufacturing a financial conflict.
pub fn utility_snapshot_sha256(
    response: &UtilityBillingResponse,
) -> Result<String, serde_json::Error> {
    let canonical = CanonicalUtilitySnapshot {
        schema_version: DOCUMENT_SCHEMA_VERSION,
        invoice_id: response.invoice_id,
        receivable_id: response.receivable_id,
        currency: &response.currency,
        lines: &response.lines,
        subtotal: response.subtotal,
        tax: response.tax,
        total: response.total,
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
    fn utility_snapshot_hash_is_shared_and_ignores_sync_status() {
        let mut pending = UtilityBillingResponse {
            invoice_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            receivable_id: Some(
                Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            ),
            invoice_number: None,
            status: "LOCAL_PENDING".into(),
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
}

/// Versioned offline payload carrying the client-generated identifiers that
/// must remain stable when PostgreSQL accepts the command after a retry.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtilityInvoiceSyncPayload {
    pub request: UtilityBillingRequest,
    pub invoice_id: Uuid,
    pub receivable_id: Uuid,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentSyncPayload {
    pub request: RecordPaymentRequest,
    pub payment_id: Uuid,
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
pub struct CreateSolarSaleRequest {
    pub customer_id: Uuid,
    pub product_code: String,
    pub principal: Decimal,
    pub currency: String,
    pub terms_revision: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolarSaleResponse {
    pub sale_id: Uuid,
    pub receivable_id: Uuid,
    pub customer_id: Uuid,
    pub principal: Decimal,
    pub currency: String,
    pub status: String,
    #[serde(default)]
    pub product_code: Option<String>,
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
    pub qr_internal_reference: String,
    pub origin: String,
}
