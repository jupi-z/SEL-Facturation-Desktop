use axum::{
    extract::{Path, Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use sel_contracts::{
    utility_snapshot_sha256, ActivateSolarContractRequest, CatalogSnapshot, CorrectMeterReading,
    CreateCustomer, CreateMeter, CreateMeterReading, CreateServiceAccount, CreateSolarQuoteRequest,
    CreateSolarSaleRequest, CreditNoteRequest, Customer, DocumentData, InvoiceLineResponse,
    PaymentAllocationRequest, RecordPaymentRequest, ReversePaymentRequest, SolarSaleResponse,
    SyncEventResult, SyncPushRequest, SyncPushResponse, UpdateCustomer, UpdateServiceAccount,
    UtilityBillingRequest, UtilityBillingResponse, VoidInvoiceRequest,
};
use sel_domain::billing;
use sel_storage::{LocalMeterError, LocalSolarSaleError, LocalStore};
use sel_sync::payload_digest;
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
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

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SyncRunSummary {
    pub attempted: usize,
    pub pulled: usize,
    pub accepted: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub rejected: usize,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AllocatePaymentRequest {
    pub allocations: Vec<PaymentAllocationRequest>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct ListQuery {
    limit: Option<u32>,
    customer_id: Option<Uuid>,
    status: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct InvoiceHistoryQuery {
    customer_id: Option<Uuid>,
    period: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
}

impl ListQuery {
    fn bounded_limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 100)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncWorkerError {
    #[error("local storage failed: {0}")]
    Storage(#[from] sqlx::Error),
    #[error("sync request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("sync server returned HTTP {status}: {body}")]
    Server { status: u16, body: String },
    #[error("sync response did not contain one result per event")]
    ResponseCardinality,
}

pub async fn sync_once(
    store: &LocalStore,
    endpoint: &str,
    bearer_token: &str,
) -> Result<SyncRunSummary, SyncWorkerError> {
    let events = store.claim_pending(100).await?;
    let mut summary = SyncRunSummary {
        attempted: events.len(),
        ..SyncRunSummary::default()
    };
    if !events.is_empty() {
        let request = SyncPushRequest {
            device_id: store.device_id,
            events: events.clone(),
        };
        let response = reqwest::Client::new()
            .post(format!("{}/v1/sync/push", endpoint.trim_end_matches('/')))
            .bearer_auth(bearer_token)
            .header("Idempotency-Key", format!("local-sync-{}", Uuid::new_v4()))
            .json(&request)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                for event in &events {
                    store.retry(event.event_id, &error.to_string()).await?;
                }
                return Err(error.into());
            }
        };
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            for event in &events {
                store
                    .retry(event.event_id, &format!("HTTP {status}: {body}"))
                    .await?;
            }
            return Err(SyncWorkerError::Server { status, body });
        }
        let payload: SyncPushResponse = match response.json().await {
            Ok(payload) => payload,
            Err(error) => {
                for event in &events {
                    store.retry(event.event_id, &error.to_string()).await?;
                }
                return Err(error.into());
            }
        };
        if payload.results.len() != events.len() {
            for event in &events {
                store
                    .retry(event.event_id, "sync response cardinality mismatch")
                    .await?;
            }
            return Err(SyncWorkerError::ResponseCardinality);
        }
        for (event, result) in events.iter().zip(payload.results.iter()) {
            match result {
                SyncEventResult::Accepted { .. } => summary.accepted += 1,
                SyncEventResult::Duplicate { .. } => summary.duplicates += 1,
                SyncEventResult::Conflict { .. } => summary.conflicts += 1,
                SyncEventResult::Rejected { .. } => summary.rejected += 1,
            }
            store.acknowledge(event.event_id, result).await?;
        }
    }
    summary.pulled = pull_once(store, endpoint, bearer_token).await?;
    Ok(summary)
}

pub async fn pull_once(
    store: &LocalStore,
    endpoint: &str,
    bearer_token: &str,
) -> Result<usize, SyncWorkerError> {
    let cursor = store.central_cursor().await?;
    let response = reqwest::Client::new()
        .get(format!(
            "{}/v1/sync/pull?cursor={cursor}&limit=100",
            endpoint.trim_end_matches('/')
        ))
        .bearer_auth(bearer_token)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(SyncWorkerError::Server {
            status: response.status().as_u16(),
            body: response.text().await.unwrap_or_default(),
        });
    }
    let payload: sel_contracts::SyncPullResponse = response.json().await?;
    let count = payload.changes.len();
    store.apply_pulled_changes(&payload).await?;
    Ok(count)
}

pub async fn refresh_catalog_once(
    store: &LocalStore,
    endpoint: &str,
    bearer_token: &str,
) -> Result<bool, SyncWorkerError> {
    let response = reqwest::Client::new()
        .get(format!(
            "{}/v1/catalog/snapshot",
            endpoint.trim_end_matches('/')
        ))
        .bearer_auth(bearer_token)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(SyncWorkerError::Server {
            status: response.status().as_u16(),
            body: response.text().await.unwrap_or_default(),
        });
    }
    let snapshot: CatalogSnapshot = response.json().await?;
    if store.catalog_snapshot_hash().await?.as_deref() == Some(snapshot.sha256.as_str()) {
        return Ok(false);
    }
    store.apply_catalog_snapshot(&snapshot).await?;
    Ok(true)
}

/// Runs one push/pull cycle immediately and retries on a fixed interval. A
/// failed cycle leaves the outbox untouched, so reconnecting automatically
/// resumes transmission without losing or duplicating business effects.
pub fn spawn_sync_worker(
    store: LocalStore,
    endpoint: impl Into<String>,
    bearer_token: impl Into<String>,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    let endpoint = endpoint.into();
    let bearer_token = bearer_token.into();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval.max(std::time::Duration::from_millis(50)));
        loop {
            ticker.tick().await;
            if let Err(error) = sync_once(&store, &endpoint, &bearer_token).await {
                eprintln!("SEL local synchronization deferred: {error}");
            } else if let Err(error) = refresh_catalog_once(&store, &endpoint, &bearer_token).await
            {
                eprintln!("SEL local catalog refresh deferred: {error}");
            }
        }
    })
}

pub fn router_with_store(store: LocalStore) -> Router {
    router_with_store_base(store)
}

/// Builds the local API with the same routes used by the desktop client, but
/// requires a process-local bearer token on every non-health request.
///
/// `router_with_store` intentionally remains unauthenticated for in-process
/// unit/integration tests. The executable calls this production constructor
/// after validating `SEL_LOCAL_AUTH_TOKEN`.
pub fn router_with_store_and_auth(store: LocalStore, auth_token: impl Into<String>) -> Router {
    let expected = Arc::<str>::from(auth_token.into());
    router_with_store_base(store).layer(middleware::from_fn(move |request, next| {
        let expected = Arc::clone(&expected);
        async move { local_auth_guard(request, next, expected).await }
    }))
}

fn router_with_store_base(store: LocalStore) -> Router {
    Router::new()
        .route(
            "/local/v1/customers",
            get(list_customers).post(create_customer),
        )
        .route(
            "/local/v1/customers/:id",
            get(get_customer).patch(update_customer),
        )
        .route(
            "/local/v1/customers/:id/service-accounts",
            get(list_service_accounts).post(create_service_account),
        )
        .route(
            "/local/v1/service-accounts/:id",
            get(get_service_account).patch(update_service_account),
        )
        .route(
            "/local/v1/service-accounts/:id/meters",
            get(list_meters).post(create_meter),
        )
        .route(
            "/local/v1/meters/:id/readings",
            get(list_meter_readings).post(create_meter_reading),
        )
        .route(
            "/local/v1/meters/:id/readings/preview",
            post(preview_meter_reading),
        )
        .route(
            "/local/v1/meter-readings/:id/corrections",
            post(correct_meter_reading),
        )
        .route(
            "/local/v1/billing/utility/issue",
            post(issue_utility_invoice),
        )
        .route(
            "/local/v1/billing/utility/preview",
            post(preview_utility_invoice),
        )
        .route("/local/v1/invoices", get(list_invoices))
        .route("/local/v1/invoices/:id", get(get_invoice))
        .route(
            "/local/v1/invoices/:id/document-data",
            get(get_invoice_document_data),
        )
        .route("/local/v1/invoices/:id/void", post(void_invoice))
        .route("/local/v1/receivables/:id", get(get_receivable))
        .route("/local/v1/customers/:id/statement", get(get_statement))
        .route("/local/v1/installment-plans/:id", get(get_installment_plan))
        .route("/local/v1/receivables/:id/credit", post(create_credit_note))
        .route(
            "/local/v1/payments",
            get(list_payments).post(record_payment),
        )
        .route(
            "/local/v1/payments/:id/document-data",
            get(payment_document_data),
        )
        .route("/local/v1/payments/:id/allocate", post(allocate_payment))
        .route("/local/v1/payments/:id/reverse", post(reverse_payment))
        .route(
            "/local/v1/solar/sales",
            get(list_solar_sales).post(create_solar_sale),
        )
        .route("/local/v1/solar/products", get(list_solar_products))
        .route("/local/v1/solar/quotes", post(create_solar_quote))
        .route("/local/v1/solar/contracts", post(create_solar_contract))
        .route(
            "/local/v1/solar/contracts/:id/activate",
            post(activate_solar_contract),
        )
        .route("/local/v1/sync/status", get(status_with_store))
        .route("/local/v1/sync/trigger", post(trigger_sync_with_store))
        .route("/local/v1/updates/status", get(update_status))
        .with_state(LocalAppState { store })
}

async fn local_auth_guard(
    request: Request<axum::body::Body>,
    next: Next,
    expected: Arc<str>,
) -> Response {
    let presented = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !constant_time_equal(presented.as_bytes(), expected.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "local bearer token is required").into_response();
    }
    next.run(request).await
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
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
    let offline_grant =
        if state.store.actor_id != Uuid::nil() && state.store.offline_grant_id != Uuid::nil() {
            "CONFIGURED"
        } else {
            "NOT_CONFIGURED"
        };
    Ok(Json(json!({
        "status": if pending == 0 { "LOCAL_READY" } else { "LOCAL_PENDING" },
        "pending": pending,
        "conflicts": 0,
        "rejected": 0,
        "protocol_version": 1,
        "offline_grant": offline_grant
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

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct CustomerListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    status: Option<String>,
    q: Option<String>,
}

async fn list_customers(
    State(state): State<LocalAppState>,
    Query(query): Query<CustomerListQuery>,
) -> Result<Response, (StatusCode, String)> {
    if query
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "ACTIVE" | "ARCHIVED"))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "status must be ACTIVE or ARCHIVED".into(),
        ));
    }
    let search = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if search.is_some_and(|value| value.chars().count() > 128) {
        return Err((
            StatusCode::BAD_REQUEST,
            "customer search is limited to 128 characters".into(),
        ));
    }
    let limit = query.limit.unwrap_or(100).clamp(1, 100) as i64;
    let (customers, next_cursor) = state
        .store
        .list_customers_page(
            query.cursor.as_deref(),
            query.status.as_deref(),
            search,
            limit,
        )
        .await
        .map_err(internal_error)?;
    let mut response = Json(customers).into_response();
    if let Some(cursor) = next_cursor {
        response.headers_mut().insert(
            "x-next-cursor",
            axum::http::HeaderValue::from_str(&cursor)
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?,
        );
    }
    Ok(response)
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

async fn update_customer(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(input): Json<UpdateCustomer>,
) -> Result<Json<Customer>, (StatusCode, String)> {
    if input
        .legal_name
        .as_deref()
        .is_some_and(|legal_name| legal_name.trim().is_empty())
    {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "legal name cannot be empty".into(),
        ));
    }
    if input
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "ACTIVE" | "ARCHIVED"))
    {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "status must be ACTIVE or ARCHIVED".into(),
        ));
    }
    let expected_revision = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse::<i64>().ok())
        .ok_or_else(|| {
            (
                StatusCode::PRECONDITION_REQUIRED,
                "If-Match revision is required".into(),
            )
        })?;
    state
        .store
        .update_customer(id, expected_revision, &input)
        .await
        .map_err(internal_error)?
        .map(|(customer, _)| Json(customer))
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "customer is missing or its revision has changed".into(),
            )
        })
}

async fn create_service_account(
    State(state): State<LocalAppState>,
    Path(customer_id): Path<Uuid>,
    Json(input): Json<CreateServiceAccount>,
) -> Result<(StatusCode, Json<sel_contracts::ServiceAccount>), (StatusCode, String)> {
    state
        .store
        .create_service_account(customer_id, &input)
        .await
        .map(|account| (StatusCode::CREATED, Json(account)))
        .map_err(local_meter_error)
}

async fn list_service_accounts(
    State(state): State<LocalAppState>,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<Vec<sel_contracts::ServiceAccount>>, (StatusCode, String)> {
    state
        .store
        .list_service_accounts(customer_id)
        .await
        .map(Json)
        .map_err(local_meter_error)
}

async fn get_service_account(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<sel_contracts::ServiceAccount>, (StatusCode, String)> {
    state
        .store
        .get_service_account(id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "service account not found".into()))
}

async fn update_service_account(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(input): Json<UpdateServiceAccount>,
) -> Result<Json<sel_contracts::ServiceAccount>, (StatusCode, String)> {
    let expected_revision = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse::<i64>().ok())
        .ok_or_else(|| {
            (
                StatusCode::PRECONDITION_REQUIRED,
                "If-Match revision is required".into(),
            )
        })?;
    state
        .store
        .update_service_account(id, expected_revision, &input)
        .await
        .map_err(local_meter_error)?
        .map(|(account, _)| Json(account))
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "service account is missing or its revision has changed".into(),
            )
        })
}

async fn create_meter(
    State(state): State<LocalAppState>,
    Path(service_account_id): Path<Uuid>,
    Json(input): Json<CreateMeter>,
) -> Result<(StatusCode, Json<sel_contracts::Meter>), (StatusCode, String)> {
    state
        .store
        .create_meter(service_account_id, &input)
        .await
        .map(|meter| (StatusCode::CREATED, Json(meter)))
        .map_err(local_meter_error)
}

async fn list_meters(
    State(state): State<LocalAppState>,
    Path(service_account_id): Path<Uuid>,
) -> Result<Json<Vec<sel_contracts::Meter>>, (StatusCode, String)> {
    state
        .store
        .list_meters(service_account_id)
        .await
        .map(Json)
        .map_err(local_meter_error)
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct MeterReadingQuery {
    period: Option<String>,
}

async fn list_meter_readings(
    State(state): State<LocalAppState>,
    Path(meter_id): Path<Uuid>,
    Query(query): Query<MeterReadingQuery>,
) -> Result<Json<Vec<sel_contracts::MeterReading>>, (StatusCode, String)> {
    state
        .store
        .list_meter_readings(meter_id, query.period.as_deref())
        .await
        .map(Json)
        .map_err(local_meter_error)
}

async fn create_meter_reading(
    State(state): State<LocalAppState>,
    Path(meter_id): Path<Uuid>,
    Json(input): Json<CreateMeterReading>,
) -> Result<(StatusCode, Json<sel_contracts::MeterReading>), (StatusCode, String)> {
    state
        .store
        .create_meter_reading(meter_id, &input)
        .await
        .map(|reading| (StatusCode::CREATED, Json(reading)))
        .map_err(local_meter_error)
}

async fn preview_meter_reading(
    State(state): State<LocalAppState>,
    Path(meter_id): Path<Uuid>,
    Json(input): Json<CreateMeterReading>,
) -> Result<Json<sel_contracts::MeterReadingPreview>, (StatusCode, String)> {
    state
        .store
        .preview_meter_reading(meter_id, &input)
        .await
        .map(Json)
        .map_err(local_meter_error)
}

async fn correct_meter_reading(
    State(state): State<LocalAppState>,
    Path(reading_id): Path<Uuid>,
    Json(input): Json<CorrectMeterReading>,
) -> Result<(StatusCode, Json<sel_contracts::MeterReading>), (StatusCode, String)> {
    state
        .store
        .correct_meter_reading(reading_id, &input)
        .await
        .map(|reading| (StatusCode::CREATED, Json(reading)))
        .map_err(local_meter_error)
}

async fn issue_utility_invoice(
    State(state): State<LocalAppState>,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<(StatusCode, Json<UtilityBillingResponse>), (StatusCode, String)> {
    let input = resolve_utility_input(&state.store, input).await?;
    let response =
        calculate_utility_response(&state.store, &input, "LOCAL_PENDING", Some(Uuid::new_v4()))
            .await?;
    state
        .store
        .issue_utility_invoice(&input, &response)
        .await
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn preview_utility_invoice(
    State(state): State<LocalAppState>,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<Json<UtilityBillingResponse>, (StatusCode, String)> {
    let input = resolve_utility_input(&state.store, input).await?;
    let response = calculate_utility_response(&state.store, &input, "LOCAL_PREVIEW", None).await?;
    Ok(Json(response))
}

async fn resolve_utility_input(
    store: &LocalStore,
    mut input: UtilityBillingRequest,
) -> Result<UtilityBillingRequest, (StatusCode, String)> {
    if let Some(refs) = &input.meter_readings {
        let (electricity, water) = store
            .utility_meter_reading_quantities(input.customer_id, &input.billing_period, refs)
            .await
            .map_err(local_meter_error)?;
        input.electricity.quantity = electricity;
        input.water.quantity = water;
    }
    Ok(input)
}

async fn calculate_utility_response(
    store: &LocalStore,
    input: &UtilityBillingRequest,
    status: &str,
    receivable_id: Option<Uuid>,
) -> Result<UtilityBillingResponse, (StatusCode, String)> {
    if input.electricity.unit != "KWH" || input.water.unit != "M3" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "utility units must be KWH and M3".into(),
        ));
    }
    if input.billing_period.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "billing period is required".into(),
        ));
    }
    if input.water.quantity.is_zero() && !input.water_zero_confirmed {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "zero water consumption must be confirmed".into(),
        ));
    }
    let tariffs = store
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
        receivable_id,
        invoice_number: None,
        status: status.into(),
        billing_period: input.billing_period.clone(),
        due_date: input.due_date,
        currency: result.currency,
        lines,
        subtotal: result.subtotal,
        tax: result.tax,
        total: result.total,
        snapshot_sha256: String::new(),
    };
    response.snapshot_sha256 = utility_snapshot_sha256(&response)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(response)
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

async fn get_invoice_document_data(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentData>, (StatusCode, String)> {
    let invoice = state
        .store
        .get_invoice_response(id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "invoice not found".into()))?;
    let reference = format!("SEL-V2-{id}");
    let origin = if invoice.status == "LOCAL_PENDING" {
        "LOCAL_PENDING"
    } else {
        "CENTRAL_ACCEPTED"
    };
    Ok(Json(DocumentData {
        document_schema_version: 1,
        template_version: "v2-a4-1".into(),
        document_reference: reference.clone(),
        invoice,
        qr_internal_reference: reference,
        origin: origin.into(),
    }))
}

async fn list_invoices(
    State(state): State<LocalAppState>,
    Query(query): Query<InvoiceHistoryQuery>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let period = query
        .period
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if period.is_some_and(|value| {
        chrono::NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").is_err()
    }) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "period must use YYYY-MM format".into(),
        ));
    }
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "LOCAL_PENDING" | "CENTRAL_ACCEPTED" | "VOID" | "CONFLICT_REQUIRES_REVIEW"
        )
    }) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "status must be LOCAL_PENDING, CENTRAL_ACCEPTED, VOID or CONFLICT_REQUIRES_REVIEW"
                .into(),
        ));
    }
    Ok(Json(
        state
            .store
            .list_invoice_responses(
                query.customer_id,
                period,
                query.status.as_deref(),
                query.limit.unwrap_or(50).clamp(1, 100),
            )
            .await
            .map_err(internal_error)?,
    ))
}

async fn void_invoice(
    State(state): State<LocalAppState>,
    Path(invoice_id): Path<Uuid>,
    Json(input): Json<VoidInvoiceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let event = state
        .store
        .void_invoice(invoice_id, &input.reason)
        .await
        .map_err(internal_error)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "invoice_id": invoice_id,
            "status": "LOCAL_PENDING",
            "reason": input.reason.trim(),
            "event_id": event.event_id
        })),
    ))
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

async fn get_statement(
    State(state): State<LocalAppState>,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .store
        .customer_statement(customer_id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "customer not found".into()))
}

async fn get_installment_plan(
    State(state): State<LocalAppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .store
        .get_installment_plan(id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "installment plan not found".into()))
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

async fn list_payments(
    State(state): State<LocalAppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "LOCAL_PENDING" | "CENTRAL_ACCEPTED" | "CONFLICT_REQUIRES_REVIEW" | "REJECTED"
        )
    }) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "status must be LOCAL_PENDING, CENTRAL_ACCEPTED, CONFLICT_REQUIRES_REVIEW or REJECTED"
                .into(),
        ));
    }
    Ok(Json(
        state
            .store
            .list_payments(
                query.bounded_limit(),
                query.customer_id,
                query.status.as_deref(),
            )
            .await
            .map_err(internal_error)?,
    ))
}

async fn payment_document_data(
    State(state): State<LocalAppState>,
    Path(payment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .store
        .payment_document_data(payment_id)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "payment not found".into()))
}

async fn allocate_payment(
    State(state): State<LocalAppState>,
    Path(payment_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(input): Json<AllocatePaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() >= 8)
        .ok_or_else(|| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Idempotency-Key is required".into(),
            )
        })?;
    let digest = payload_digest(
        &serde_json::to_value(&input)
            .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?,
    );
    let body = state
        .store
        .allocate_payment(payment_id, &input.allocations, key, &digest)
        .await
        .map_err(local_payment_allocation_error)?;
    Ok((StatusCode::CREATED, Json(body)))
}

async fn reverse_payment(
    State(state): State<LocalAppState>,
    Path(payment_id): Path<Uuid>,
    Json(input): Json<ReversePaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let (body, _) = state
        .store
        .reverse_payment(payment_id, &input)
        .await
        .map_err(local_payment_reverse_error)?;
    Ok((StatusCode::CREATED, Json(body)))
}

async fn create_credit_note(
    State(state): State<LocalAppState>,
    Path(receivable_id): Path<Uuid>,
    Json(input): Json<CreditNoteRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let (body, _) = state
        .store
        .create_credit_note_offline(receivable_id, &input)
        .await
        .map_err(local_credit_note_error)?;
    Ok((StatusCode::CREATED, Json(body)))
}

async fn create_solar_sale(
    State(state): State<LocalAppState>,
    Json(input): Json<CreateSolarSaleRequest>,
) -> Result<(StatusCode, Json<SolarSaleResponse>), (StatusCode, String)> {
    let (response, _) = state
        .store
        .create_solar_sale_offline(&input)
        .await
        .map_err(local_solar_sale_error)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn list_solar_sales(
    State(state): State<LocalAppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    Ok(Json(
        state
            .store
            .list_solar_sales(query.bounded_limit())
            .await
            .map_err(internal_error)?,
    ))
}

async fn list_solar_products(
    State(state): State<LocalAppState>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let products = state
        .store
        .list_active_solar_products()
        .await
        .map_err(internal_error)?;
    Ok(Json(products))
}

async fn create_solar_quote(
    State(state): State<LocalAppState>,
    Json(input): Json<CreateSolarQuoteRequest>,
) -> Result<Json<sel_contracts::SolarQuoteResponse>, (StatusCode, String)> {
    state
        .store
        .create_solar_quote(&input)
        .await
        .map_err(internal_error)?
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "active solar product was not found".into(),
            )
        })
}

async fn activate_solar_contract(
    State(state): State<LocalAppState>,
    Path(contract_id): Path<Uuid>,
    Json(input): Json<ActivateSolarContractRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let (response, _) = state
        .store
        .activate_solar_contract_offline(contract_id, &input)
        .await
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn create_solar_contract(
    State(state): State<LocalAppState>,
    Json(input): Json<ActivateSolarContractRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let (response, _) = state
        .store
        .activate_solar_contract_offline(Uuid::new_v4(), &input)
        .await
        .map_err(internal_error)?;
    Ok((StatusCode::CREATED, Json(response)))
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

fn local_meter_error(error: LocalMeterError) -> (StatusCode, String) {
    let status = match error {
        LocalMeterError::CustomerNotFound
        | LocalMeterError::ServiceAccountNotFound
        | LocalMeterError::MeterNotFound => StatusCode::NOT_FOUND,
        LocalMeterError::DuplicateReading
        | LocalMeterError::PreviousIndexMismatch
        | LocalMeterError::ReadingNotLatest
        | LocalMeterError::ReadingAlreadyInvoiced => StatusCode::CONFLICT,
        LocalMeterError::InvalidConfiguration
        | LocalMeterError::InvalidInput
        | LocalMeterError::InvalidIndex
        | LocalMeterError::CorrectionReasonRequired
        | LocalMeterError::ReadingUnavailable => StatusCode::UNPROCESSABLE_ENTITY,
        LocalMeterError::Database(_) | LocalMeterError::InvalidPayload(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}

fn local_solar_sale_error(error: LocalSolarSaleError) -> (StatusCode, String) {
    let status = match error {
        LocalSolarSaleError::CustomerNotFound => StatusCode::NOT_FOUND,
        LocalSolarSaleError::InvalidAmount
        | LocalSolarSaleError::ProductNotFound
        | LocalSolarSaleError::PriceMismatch => StatusCode::UNPROCESSABLE_ENTITY,
        LocalSolarSaleError::Database(_)
        | LocalSolarSaleError::InvalidProductPrice(_)
        | LocalSolarSaleError::InvalidPayload(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
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

fn local_payment_reverse_error(
    error: sel_storage::sqlite::LocalPaymentReverseError,
) -> (StatusCode, String) {
    let status = match error {
        sel_storage::sqlite::LocalPaymentReverseError::PaymentNotFound => StatusCode::NOT_FOUND,
        sel_storage::sqlite::LocalPaymentReverseError::PaymentNotReversible
        | sel_storage::sqlite::LocalPaymentReverseError::AmountExceedsRemaining => {
            StatusCode::CONFLICT
        }
        sel_storage::sqlite::LocalPaymentReverseError::InvalidAmount
        | sel_storage::sqlite::LocalPaymentReverseError::ReasonRequired => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        sel_storage::sqlite::LocalPaymentReverseError::Database(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}

fn local_credit_note_error(
    error: sel_storage::sqlite::LocalCreditNoteError,
) -> (StatusCode, String) {
    let status = match error {
        sel_storage::sqlite::LocalCreditNoteError::ReceivableNotFound => StatusCode::NOT_FOUND,
        sel_storage::sqlite::LocalCreditNoteError::ReceivableNotCreditable
        | sel_storage::sqlite::LocalCreditNoteError::AmountExceedsBalance => StatusCode::CONFLICT,
        sel_storage::sqlite::LocalCreditNoteError::InvalidAmount
        | sel_storage::sqlite::LocalCreditNoteError::ReasonRequired => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        sel_storage::sqlite::LocalCreditNoteError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.to_string())
}

fn local_payment_allocation_error(
    error: sel_storage::sqlite::LocalPaymentAllocationError,
) -> (StatusCode, String) {
    let status = match error {
        sel_storage::sqlite::LocalPaymentAllocationError::PaymentNotFound => StatusCode::NOT_FOUND,
        sel_storage::sqlite::LocalPaymentAllocationError::IdempotencyPayloadMismatch
        | sel_storage::sqlite::LocalPaymentAllocationError::Overallocated
        | sel_storage::sqlite::LocalPaymentAllocationError::PaymentNotAllocatable => {
            StatusCode::CONFLICT
        }
        sel_storage::sqlite::LocalPaymentAllocationError::InvalidAllocation
        | sel_storage::sqlite::LocalPaymentAllocationError::AllocationExceedsPayment
        | sel_storage::sqlite::LocalPaymentAllocationError::ContextInvalid => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        sel_storage::sqlite::LocalPaymentAllocationError::Database(_)
        | sel_storage::sqlite::LocalPaymentAllocationError::StoredResponse(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, error.to_string())
}

pub async fn run(addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router()).await
}

pub async fn run_with_store(addr: SocketAddr, store: LocalStore) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router_with_store(store)).await
}

pub async fn run_with_store_and_auth(
    addr: SocketAddr,
    store: LocalStore,
    auth_token: impl Into<String>,
) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router_with_store_and_auth(store, auth_token)).await
}

pub async fn run_with_store_and_auth_and_sync(
    addr: SocketAddr,
    store: LocalStore,
    auth_token: impl Into<String>,
    sync_endpoint: impl Into<String>,
    sync_bearer_token: impl Into<String>,
) -> Result<(), std::io::Error> {
    let _worker = spawn_sync_worker(
        store.clone(),
        sync_endpoint,
        sync_bearer_token,
        std::time::Duration::from_secs(30),
    );
    run_with_store_and_auth(addr, store, auth_token).await
}
