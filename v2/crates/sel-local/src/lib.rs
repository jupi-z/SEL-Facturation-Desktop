use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use sel_contracts::{
    CreateCustomer, Customer, InvoiceLineResponse, RecordPaymentRequest, UtilityBillingRequest,
    UtilityBillingResponse,
};
use sel_domain::billing;
use sel_storage::LocalStore;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use uuid::Uuid;

pub fn router() -> Router {
    Router::new()
        .route("/local/v1/sync/status", get(status))
        .route("/local/v1/sync/trigger", post(trigger_sync))
        .route("/local/v1/updates/status", get(update_status))
}

#[derive(Clone)]
pub struct LocalAppState {
    pub store: LocalStore,
}

pub fn router_with_store(store: LocalStore) -> Router {
    Router::new()
        .route(
            "/local/v1/customers",
            get(list_customers).post(create_customer),
        )
        .route("/local/v1/customers/:id", get(get_customer))
        .route(
            "/local/v1/billing/utility/issue",
            post(issue_utility_invoice),
        )
        .route("/local/v1/invoices/:id", get(get_invoice))
        .route("/local/v1/receivables/:id", get(get_receivable))
        .route("/local/v1/payments", post(record_payment))
        .route("/local/v1/sync/status", get(status_with_store))
        .route("/local/v1/sync/trigger", post(trigger_sync_with_store))
        .route("/local/v1/updates/status", get(update_status))
        .with_state(LocalAppState { store })
}

async fn status() -> Json<serde_json::Value> {
    Json(
        json!({"status":"LOCAL_ONLY","pending":0,"conflicts":0,"rejected":0,"protocol_version":1,"offline_grant":"NOT_CONFIGURED"}),
    )
}

async fn trigger_sync() -> Json<serde_json::Value> {
    Json(
        json!({"status":"PENDING_NETWORK","accepted":0,"duplicates":0,"conflicts":0,"message":"No remote session is configured; local data was not removed."}),
    )
}

async fn update_status() -> Json<serde_json::Value> {
    Json(
        json!({"status":"TRUSTED_RESOURCES_ONLY","safe_update_required":false,"last_verified_sequence":0}),
    )
}

async fn status_with_store(
    State(state): State<LocalAppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let pending = state.store.pending_count().await.map_err(internal_error)?;
    Ok(Json(json!({
        "status": if pending == 0 { "LOCAL_READY" } else { "LOCAL_PENDING" },
        "pending": pending,
        "conflicts": 0,
        "rejected": 0,
        "protocol_version": 1,
        "offline_grant": "NOT_CONFIGURED"
    })))
}

async fn create_customer(
    State(state): State<LocalAppState>,
    Json(input): Json<CreateCustomer>,
) -> Result<(StatusCode, Json<Customer>), (StatusCode, String)> {
    let (customer, _) = state
        .store
        .create_customer(&input)
        .await
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(customer)))
}

async fn list_customers(
    State(state): State<LocalAppState>,
) -> Result<Json<Vec<Customer>>, (StatusCode, String)> {
    Ok(Json(
        state.store.list_customers().await.map_err(internal_error)?,
    ))
}

async fn get_customer(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Customer>, (StatusCode, String)> {
    state
        .store
        .get_customer(id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "customer not found".into()))
}

async fn issue_utility_invoice(
    State(state): State<LocalAppState>,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<(StatusCode, Json<UtilityBillingResponse>), (StatusCode, String)> {
    if input.electricity.unit != "KWH" || input.water.unit != "M3" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "utility units must be KWH and M3".into(),
        ));
    }
    if input.water.quantity.is_zero() && !input.water_zero_confirmed {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "zero water consumption must be confirmed".into(),
        ));
    }
    let tariffs = state
        .store
        .load_tariffs(
            &input.tariff_revision,
            &input.currency,
            &input.billing_period,
        )
        .await
        .map_err(internal_error)?;
    let water = if input.water.quantity.is_zero() {
        billing::WaterInput::ZeroConfirmed
    } else {
        billing::WaterInput::Positive(input.water.quantity)
    };
    let result = billing::calculate_utility(billing::UtilityInput {
        electricity_kwh: input.electricity.quantity,
        water_m3: water,
        electricity_unit_price: tariffs.electricity_unit_price.ok_or_else(|| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "approved electricity tariff is unavailable".into(),
            )
        })?,
        electricity_tax_rate: tariffs.electricity_tax_rate.ok_or_else(|| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "approved electricity tax profile is unavailable".into(),
            )
        })?,
        water_tariff: tariffs.water_unit_price,
        water_tax_rate: tariffs.water_tax_rate,
        currency: input.currency.clone(),
    })
    .map_err(|error| (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    let lines = result
        .lines
        .into_iter()
        .map(|line| InvoiceLineResponse {
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
        invoice_id: Uuid::new_v4(),
        receivable_id: Some(Uuid::new_v4()),
        invoice_number: None,
        status: "LOCAL_PENDING".into(),
        currency: result.currency,
        lines,
        subtotal: result.subtotal,
        tax: result.tax,
        total: result.total,
        snapshot_sha256: String::new(),
    };
    let snapshot = json!({"input": input, "response": response});
    response.snapshot_sha256 = digest_json(&snapshot)?;
    state
        .store
        .issue_utility_invoice(&input, &response)
        .await
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn get_invoice(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<UtilityBillingResponse>, (StatusCode, String)> {
    state
        .store
        .get_invoice_response(id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "invoice not found".into()))
}

async fn get_receivable(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .store
        .get_receivable_summary(id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "receivable not found".into()))
}

async fn record_payment(
    State(state): State<LocalAppState>,
    Json(input): Json<RecordPaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let (body, _) = state
        .store
        .record_payment(&input)
        .await
        .map_err(local_payment_error)?;
    Ok((StatusCode::CREATED, Json(body)))
}

async fn trigger_sync_with_store(
    State(state): State<LocalAppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let pending = state.store.pending_count().await.map_err(internal_error)?;
    Ok(Json(json!({
        "status": if pending == 0 { "IDLE" } else { "PENDING_NETWORK" },
        "pending": pending,
        "accepted": 0,
        "duplicates": 0,
        "conflicts": 0,
        "message": "No remote session is configured; local data was not removed."
    })))
}

fn internal_error(error: sqlx::Error) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        error.to_string(),
    )
}

fn local_payment_error(error: sel_storage::sqlite::LocalPaymentError) -> (StatusCode, String) {
    let status = match error {
        sel_storage::sqlite::LocalPaymentError::InvalidAllocation
        | sel_storage::sqlite::LocalPaymentError::AllocationExceedsReceipt => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        sel_storage::sqlite::LocalPaymentError::ContextInvalid
        | sel_storage::sqlite::LocalPaymentError::Overallocated => StatusCode::CONFLICT,
        sel_storage::sqlite::LocalPaymentError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

fn digest_json(value: &serde_json::Value) -> Result<String, (StatusCode, String)> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

pub async fn run(addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router()).await
}

pub async fn run_with_store(addr: SocketAddr, store: LocalStore) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router_with_store(store)).await
}
