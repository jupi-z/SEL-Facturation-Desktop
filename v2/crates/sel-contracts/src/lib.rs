use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentAllocationRequest {
    pub receivable_id: Uuid,
    pub amount: Decimal,
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
