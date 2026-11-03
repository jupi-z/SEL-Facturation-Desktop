use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_decimal::Decimal;
use sel_contracts::{
    utility_snapshot_sha256, CreateCustomer, CreateSolarSaleRequest, Customer, DocumentData,
    InvoiceLineResponse, PaymentAllocationRequest, ProblemDetails, RecordPaymentRequest,
    ReversePaymentRequest, SolarSaleResponse, SyncPullResponse, SyncPushRequest, SyncPushResponse,
    UpdateCustomer, UtilityBillingRequest, UtilityBillingResponse, VoidInvoiceRequest,
};
use sel_domain::billing;
use sel_domain::solar;
use sel_sync::SyncEngine;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub organization_id: Uuid,
    pub central: Option<sel_storage::CentralStore>,
    strict_auth: bool,
    pub tariffs: Arc<RwLock<TariffBook>>,
    pub customers: Arc<RwLock<HashMap<Uuid, Customer>>>,
    pub invoices: Arc<RwLock<HashMap<Uuid, InvoiceRecord>>>,
    pub receivables: Arc<RwLock<HashMap<Uuid, ReceivableRecord>>>,
    pub payments: Arc<RwLock<HashMap<Uuid, PaymentRecord>>>,
    pub idempotency: Arc<RwLock<HashMap<String, IdempotencyRecord>>>,
    pub sync: Arc<RwLock<SyncEngine>>,
    pub solar_sales: Arc<RwLock<HashMap<Uuid, SolarSaleResponse>>>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct LoginRequest {
    pub identity_ref: String,
    pub password: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct SetUserPasswordRequest {
    pub password: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct EnrollDeviceRequest {
    pub device_id: Uuid,
    pub public_key_id: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct CreateOfflineGrantRequest {
    pub grant_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub scopes: serde_json::Value,
    pub limits: serde_json::Value,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub signature: String,
}

#[derive(Clone, Debug)]
struct AuthenticatedPrincipal(sel_storage::postgres::AuthSessionPrincipal);

#[derive(Clone, Default)]
pub struct TariffBook {
    pub electricity_unit_price: Option<Decimal>,
    pub electricity_tax_rate: Option<Decimal>,
    pub water_unit_price: Option<Decimal>,
    pub water_tax_rate: Option<Decimal>,
    pub revision: Option<String>,
}

#[derive(Clone)]
pub struct InvoiceRecord {
    pub customer_id: Uuid,
    pub period: String,
    pub response: UtilityBillingResponse,
}

#[derive(Clone)]
pub struct ReceivableRecord {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub currency: String,
    pub principal: Decimal,
}

#[derive(Clone)]
pub struct PaymentRecord {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub amount: Decimal,
    pub allocations: Vec<PaymentAllocationRequest>,
}

#[derive(Clone)]
pub struct IdempotencyRecord {
    pub digest: String,
    pub response_status: StatusCode,
    pub response: serde_json::Value,
}

impl AppState {
    pub fn new(organization_id: Uuid) -> Self {
        Self {
            organization_id,
            central: None,
            strict_auth: false,
            tariffs: Arc::new(RwLock::new(TariffBook::default())),
            customers: Arc::new(RwLock::new(HashMap::new())),
            invoices: Arc::new(RwLock::new(HashMap::new())),
            receivables: Arc::new(RwLock::new(HashMap::new())),
            payments: Arc::new(RwLock::new(HashMap::new())),
            idempotency: Arc::new(RwLock::new(HashMap::new())),
            sync: Arc::new(RwLock::new(SyncEngine::default())),
            solar_sales: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_central_store(store: sel_storage::CentralStore) -> Self {
        let mut state = Self::new(store.organization_id);
        state.central = Some(store);
        state
    }

    pub fn with_production_central_store(store: sel_storage::CentralStore) -> Self {
        let mut state = Self::with_central_store(store);
        state.strict_auth = true;
        state
    }

    pub async fn configure_tariffs(&self, tariffs: TariffBook) {
        *self.tariffs.write().await = tariffs;
    }
}

pub fn router(state: AppState) -> Router {
    let auth_state = state.clone();
    Router::new()
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/v1/auth/login", post(auth_login))
        .route("/v1/auth/refresh", post(auth_refresh))
        .route("/v1/auth/logout", post(auth_logout))
        .route("/v1/users/:id/password", post(set_user_password))
        .route("/v1/devices/enroll", post(enroll_device))
        .route("/v1/devices/:id/revoke", post(revoke_device))
        .route("/v1/offline-grants", post(create_offline_grant))
        .route("/v1/meta/capabilities", get(capabilities))
        .route("/v1/customers", get(list_customers).post(create_customer))
        .route(
            "/v1/customers/:id",
            get(get_customer).patch(update_customer),
        )
        .route("/v1/billing/utility/preview", post(preview_utility))
        .route("/v1/billing/utility/issue", post(issue_utility))
        .route("/v1/invoices/:id", get(get_invoice))
        .route("/v1/invoices/:id/document-data", get(get_document_data))
        .route("/v1/invoices/:id/void", post(void_invoice))
        .route("/v1/payments", post(record_payment))
        .route("/v1/payments/:id/reverse", post(reverse_payment))
        .route("/v1/receivables/:id", get(get_receivable))
        .route("/v1/customers/:id/statement", get(get_statement))
        .route("/v1/sync/push", post(sync_push))
        .route("/v1/sync/pull", get(sync_pull))
        .route("/v1/sync/status", get(sync_status))
        .route("/v1/config/versions", get(config_versions))
        .route("/v1/client-resources/manifest", get(resource_manifest))
        .route("/v1/solar/sales", post(create_solar_sale))
        .route(
            "/v1/solar/contracts/:id/activate",
            post(activate_solar_contract),
        )
        .layer(middleware::from_fn_with_state(
            auth_state,
            production_auth_guard,
        ))
        .with_state(state)
}

pub async fn run(addr: std::net::SocketAddr, state: AppState) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(state)).await
}

async fn health_live() -> Json<serde_json::Value> {
    Json(json!({"status":"live","service":"sel-api","protocol_version":1}))
}

async fn production_auth_guard(
    State(state): State<AppState>,
    mut request: axum::extract::Request,
    next: Next,
) -> Response {
    if !state.strict_auth || !request.uri().path().starts_with("/v1/") {
        return next.run(request).await;
    }
    if matches!(request.uri().path(), "/v1/auth/login" | "/v1/auth/refresh") {
        return next.run(request).await;
    }
    let configured = std::env::var("SEL_API_AUTH_TOKEN").unwrap_or_default();
    let presented = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if let Some(store) = &state.central {
        if let Ok(principal) = store.authenticate_access_token(presented).await {
            if !role_allows_request(request.method(), request.uri().path(), &principal.role) {
                return ApiError::forbidden(
                    "PERMISSION_DENIED",
                    "the authenticated role cannot perform this operation",
                )
                .into_response();
            }
            request
                .extensions_mut()
                .insert(AuthenticatedPrincipal(principal));
            return next.run(request).await;
        }
    }
    // The legacy shared token is retained only for explicit test fixtures. It
    // is never accepted by the production binary unless this opt-in is set.
    if std::env::var("SEL_ALLOW_LEGACY_TOKEN").ok().as_deref() == Some("1")
        && !configured.is_empty()
        && constant_time_equal(presented.as_bytes(), configured.as_bytes())
    {
        return next.run(request).await;
    }
    ApiError::unauthorized(
        "AUTH_NOT_CONFIGURED_OR_INVALID",
        "a valid individual session token is required",
    )
    .into_response()
}

fn role_allows_request(method: &axum::http::Method, path: &str, role: &str) -> bool {
    if *method == axum::http::Method::GET {
        return matches!(
            role,
            "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
        );
    }
    if path == "/v1/sync/pull" {
        return matches!(
            role,
            "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
        );
    }
    if path == "/v1/sync/push" || path.starts_with("/v1/billing/") || path.starts_with("/v1/solar/")
    {
        return matches!(role, "ADMIN" | "BILLING_AGENT" | "SUPERVISOR");
    }
    if path == "/v1/payments" || path.starts_with("/v1/payments/") {
        return matches!(role, "ADMIN" | "CASHIER" | "SUPERVISOR");
    }
    if path.starts_with("/v1/customers") {
        return matches!(role, "ADMIN" | "BILLING_AGENT" | "SUPERVISOR");
    }
    matches!(role, "ADMIN" | "SUPERVISOR")
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

async fn health_ready(State(state): State<AppState>) -> Json<serde_json::Value> {
    let configured = state.tariffs.read().await.revision.is_some();
    Json(json!({"status":"ready","service":"sel-api","configured":configured}))
}

async fn capabilities() -> Json<serde_json::Value> {
    Json(json!({
        "protocol_version": 1,
        "document_schema_version": 1,
        "services": ["ELECTRICITY", "WATER", "SOLAR_KIT"],
        "pdf_endpoint": false,
        "server_invoice_files": false
    }))
}

async fn auth_login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(store) = &state.central else {
        return Err(ApiError::unauthorized(
            "AUTH_UNAVAILABLE",
            "individual authentication requires the central PostgreSQL store",
        ));
    };
    if input.identity_ref.trim().is_empty() || input.password.is_empty() {
        return Err(ApiError::unauthorized(
            "INVALID_CREDENTIALS",
            "identity_ref and password are required",
        ));
    }
    store
        .login_user(&input.identity_ref, &input.password)
        .await
        .map(Json)
        .map_err(auth_error)
}

async fn auth_refresh(
    State(state): State<AppState>,
    Json(input): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(store) = &state.central else {
        return Err(ApiError::unauthorized(
            "AUTH_UNAVAILABLE",
            "individual authentication requires the central PostgreSQL store",
        ));
    };
    if input.refresh_token.trim().is_empty() {
        return Err(ApiError::unauthorized(
            "INVALID_SESSION",
            "refresh_token is required",
        ));
    }
    store
        .refresh_user_session(&input.refresh_token)
        .await
        .map(Json)
        .map_err(auth_error)
}

async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_auth(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unauthorized(
            "AUTH_UNAVAILABLE",
            "individual authentication requires the central PostgreSQL store",
        ));
    };
    let token = bearer_token(&headers).ok_or_else(|| {
        ApiError::unauthorized("AUTH_REQUIRED", "a bearer session token is required")
    })?;
    store.revoke_user_session(token).await.map_err(auth_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_user_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<SetUserPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    require_auth(&headers)?;
    let Some(Extension(AuthenticatedPrincipal(principal))) = principal else {
        return Err(ApiError::unauthorized(
            "SESSION_REQUIRED",
            "individual session authentication is required",
        ));
    };
    if principal.role != "ADMIN" {
        return Err(ApiError::forbidden(
            "PERMISSION_DENIED",
            "only an administrator can provision user passwords",
        ));
    }
    if input.password.chars().count() < 12 {
        return Err(ApiError::unprocessable(
            "PASSWORD_TOO_SHORT",
            "password must contain at least 12 characters",
        ));
    }
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "AUTH_UNAVAILABLE",
            "password provisioning requires the central PostgreSQL store",
        ));
    };
    store
        .set_user_password(user_id, &input.password)
        .await
        .map_err(auth_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enroll_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<EnrollDeviceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let key = require_idempotency(&headers)?;
    if input.public_key_id.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "DEVICE_KEY_REQUIRED",
            "public_key_id is required",
        ));
    }
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "DEVICE_ENROLLMENT_CENTRAL_REQUIRED",
            "device enrollment requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .enroll_device(
            input.device_id,
            input.public_key_id.trim(),
            principal.user_id,
            key,
            &digest,
        )
        .await
        .map_err(device_write_error)?
    {
        sel_storage::postgres::DeviceWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::DeviceWriteOutcome::Replay(body) => Ok((StatusCode::OK, Json(body))),
    }
}

async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Path(device_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "DEVICE_REVOKE_CENTRAL_REQUIRED",
            "device revocation requires the central PostgreSQL store",
        ));
    };
    let digest = digest_bytes(device_id.as_bytes());
    match store
        .revoke_device(device_id, principal.user_id, key, &digest)
        .await
        .map_err(device_write_error)?
    {
        sel_storage::postgres::DeviceWriteOutcome::Created(_)
        | sel_storage::postgres::DeviceWriteOutcome::Replay(_) => Ok(StatusCode::NO_CONTENT),
    }
}

async fn create_offline_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateOfflineGrantRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "OFFLINE_GRANT_CENTRAL_REQUIRED",
            "offline grant issuance requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .create_offline_grant(
            input.grant_id,
            input.user_id,
            input.device_id,
            &input.scopes,
            &input.limits,
            input.issued_at,
            input.expires_at,
            &input.signature,
            principal.user_id,
            key,
            &digest,
        )
        .await
        .map_err(offline_grant_write_error)?
    {
        sel_storage::postgres::OfflineGrantWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::OfflineGrantWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
}

async fn list_customers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Customer>>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store
            .list_customers()
            .await
            .map(Json)
            .map_err(storage_error);
    }
    let mut customers: Vec<_> = state.customers.read().await.values().cloned().collect();
    customers.sort_by(|a, b| a.customer_code.cmp(&b.customer_code));
    Ok(Json(customers))
}

async fn create_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateCustomer>,
) -> Result<(StatusCode, Json<Customer>), ApiError> {
    require_auth(&headers)?;
    if input.customer_code.trim().is_empty() || input.legal_name.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "CUSTOMER_REQUIRED",
            "customer code and legal name are required",
        ));
    }
    if let Some(store) = &state.central {
        return store
            .create_customer(&input)
            .await
            .map(|customer| (StatusCode::CREATED, Json(customer)))
            .map_err(storage_error);
    }
    let mut customers = state.customers.write().await;
    if customers
        .values()
        .any(|item| item.customer_code == input.customer_code)
    {
        return Err(ApiError::conflict(
            "CUSTOMER_CODE_EXISTS",
            "customer code already exists",
        ));
    }
    let customer = Customer {
        id: Uuid::new_v4(),
        organization_id: state.organization_id,
        customer_code: input.customer_code,
        legal_name: input.legal_name,
        address: input.address,
        phone: input.phone,
        status: "ACTIVE".into(),
        revision: 1,
    };
    customers.insert(customer.id, customer.clone());
    Ok((StatusCode::CREATED, Json(customer)))
}

async fn get_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Customer>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store
            .get_customer(id)
            .await
            .map_err(storage_error)?
            .map(Json)
            .ok_or_else(|| ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found"));
    }
    state
        .customers
        .read()
        .await
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found"))
}

async fn update_customer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCustomer>,
) -> Result<Response, ApiError> {
    require_auth(&headers)?;
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse::<i64>().ok())
        .ok_or_else(|| {
            ApiError::unprocessable(
                "IF_MATCH_REQUIRED",
                "If-Match must contain the customer revision",
            )
        })?;
    if let Some(store) = &state.central {
        let customer = store
            .update_customer(id, expected, &input)
            .await
            .map_err(storage_error)?
            .ok_or_else(|| {
                ApiError::conflict(
                    "CUSTOMER_REVISION_CONFLICT",
                    "customer was changed by another actor or does not exist",
                )
            })?;
        let body = Json(customer.clone()).into_response();
        let mut response = body;
        response.headers_mut().insert(
            "etag",
            HeaderValue::from_str(&format!("\"{}\"", customer.revision))
                .map_err(ApiError::internal)?,
        );
        return Ok(response);
    }
    let mut customers = state.customers.write().await;
    let customer = customers
        .get_mut(&id)
        .ok_or_else(|| ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found"))?;
    if customer.revision != expected {
        return Err(ApiError::conflict(
            "CUSTOMER_REVISION_CONFLICT",
            "customer was changed by another actor",
        ));
    }
    if let Some(legal_name) = input.legal_name {
        if legal_name.trim().is_empty() {
            return Err(ApiError::unprocessable(
                "CUSTOMER_REQUIRED",
                "legal name cannot be empty",
            ));
        }
        customer.legal_name = legal_name;
    }
    if input.address.is_some() {
        customer.address = input.address;
    }
    if input.phone.is_some() {
        customer.phone = input.phone;
    }
    if let Some(status) = input.status {
        if !matches!(status.as_str(), "ACTIVE" | "ARCHIVED") {
            return Err(ApiError::unprocessable(
                "CUSTOMER_STATUS_INVALID",
                "status must be ACTIVE or ARCHIVED",
            ));
        }
        customer.status = status;
    }
    customer.revision += 1;
    let body = Json(customer.clone()).into_response();
    let mut response = body;
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&format!("\"{}\"", customer.revision)).map_err(ApiError::internal)?,
    );
    Ok(response)
}

async fn preview_utility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<Json<UtilityBillingResponse>, ApiError> {
    require_auth(&headers)?;
    let response = calculate_utility(&state, &input).await?;
    Ok(Json(response))
}

async fn issue_utility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<Response, ApiError> {
    require_auth(&headers)?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unprocessable("IDEMPOTENCY_REQUIRED", "Idempotency-Key is required")
        })?;
    let digest = digest_json(&input)?;
    {
        let idempotency = state.idempotency.read().await;
        if let Some(original) = idempotency.get(key) {
            if original.digest != digest {
                return Err(ApiError::conflict(
                    "IDEMPOTENCY_PAYLOAD_MISMATCH",
                    "idempotency key was reused with another payload",
                ));
            }
            return Ok((original.response_status, Json(original.response.clone())).into_response());
        }
    }
    let response = calculate_utility(&state, &input).await?;
    let mut committed = response.clone();
    committed.status = "CENTRAL_ACCEPTED".into();
    let receivable_id = Uuid::new_v4();
    committed.receivable_id = Some(receivable_id);
    if let Some(store) = &state.central {
        return match store
            .issue_utility_invoice(&input, &committed, key, &digest)
            .await
            .map_err(utility_issue_error)?
        {
            sel_storage::postgres::UtilityIssueOutcome::Created(response) => {
                Ok((StatusCode::CREATED, Json(response)).into_response())
            }
            sel_storage::postgres::UtilityIssueOutcome::Replay(response) => {
                Ok((StatusCode::OK, Json(response)).into_response())
            }
        };
    }
    let mut invoices = state.invoices.write().await;
    if invoices.values().any(|invoice| {
        invoice.customer_id == input.customer_id && invoice.period == input.billing_period
    }) {
        return Err(ApiError::conflict(
            "INVOICE_PERIOD_EXISTS",
            "an active invoice already exists for this customer and period",
        ));
    }
    let invoice_id = response.invoice_id;
    invoices.insert(
        invoice_id,
        InvoiceRecord {
            customer_id: input.customer_id,
            period: input.billing_period.clone(),
            response: committed.clone(),
        },
    );
    drop(invoices);
    state.receivables.write().await.insert(
        receivable_id,
        ReceivableRecord {
            id: receivable_id,
            customer_id: input.customer_id,
            currency: input.currency.clone(),
            principal: committed.total,
        },
    );
    state.idempotency.write().await.insert(
        key.to_owned(),
        IdempotencyRecord {
            digest,
            response_status: StatusCode::CREATED,
            response: serde_json::to_value(&committed).map_err(ApiError::internal)?,
        },
    );
    Ok((StatusCode::CREATED, Json(committed)).into_response())
}

async fn calculate_utility(
    state: &AppState,
    input: &UtilityBillingRequest,
) -> Result<UtilityBillingResponse, ApiError> {
    if input.electricity.unit != "KWH" || input.water.unit != "M3" {
        return Err(ApiError::unprocessable(
            "UNIT_INVALID",
            "utility units must be KWH and M3",
        ));
    }
    let tariffs = if let Some(store) = &state.central {
        let snapshot = store
            .load_tariffs(
                &input.tariff_revision,
                &input.currency,
                &input.billing_period,
            )
            .await
            .map_err(storage_error)?;
        TariffBook {
            electricity_unit_price: snapshot.electricity_unit_price,
            electricity_tax_rate: snapshot.electricity_tax_rate,
            water_unit_price: snapshot.water_unit_price,
            water_tax_rate: snapshot.water_tax_rate,
            revision: Some(input.tariff_revision.clone()),
        }
    } else {
        state.tariffs.read().await.clone()
    };
    if tariffs.revision.as_deref() != Some(input.tariff_revision.as_str()) {
        return Err(ApiError::unprocessable(
            "TARIFF_REVISION_INVALID",
            "the tariff revision is not approved or is unavailable",
        ));
    }
    if input.water.quantity.is_zero() && !input.water_zero_confirmed {
        return Err(ApiError::unprocessable(
            "WATER_ZERO_CONFIRMATION_REQUIRED",
            "zero water consumption must be confirmed",
        ));
    }
    if input.water.quantity.is_sign_negative() || input.electricity.quantity.is_sign_negative() {
        return Err(ApiError::unprocessable(
            "QUANTITY_INVALID",
            "quantities cannot be negative",
        ));
    }
    let water = if input.water.quantity.is_zero() {
        billing::WaterInput::ZeroConfirmed
    } else {
        billing::WaterInput::Positive(input.water.quantity)
    };
    let result = billing::calculate_utility(billing::UtilityInput {
        electricity_kwh: input.electricity.quantity,
        water_m3: water,
        electricity_unit_price: tariffs.electricity_unit_price.ok_or_else(|| {
            ApiError::unprocessable(
                "ELECTRICITY_TARIFF_MISSING",
                "electricity tariff is not configured",
            )
        })?,
        electricity_tax_rate: tariffs.electricity_tax_rate.ok_or_else(|| {
            ApiError::unprocessable(
                "ELECTRICITY_TAX_MISSING",
                "electricity tax profile is not configured",
            )
        })?,
        water_tariff: tariffs.water_unit_price,
        water_tax_rate: tariffs.water_tax_rate,
        currency: input.currency.clone(),
    })
    .map_err(|error| ApiError::unprocessable("BILLING_RULE_INVALID", error.to_string()))?;
    let lines = result
        .lines
        .into_iter()
        .map(|line| InvoiceLineResponse {
            service_code: format!("{:?}", line.service).to_uppercase(),
            quantity: line.quantity,
            unit: line.unit.to_owned(),
            unit_price: line.unit_price,
            subtotal: line.subtotal,
            tax: line.tax,
            total: line.total,
        })
        .collect();
    let mut response = UtilityBillingResponse {
        invoice_id: Uuid::new_v4(),
        receivable_id: None,
        invoice_number: None,
        status: "LOCAL_PREVIEW".into(),
        currency: result.currency,
        lines,
        subtotal: result.subtotal,
        tax: result.tax,
        total: result.total,
        snapshot_sha256: String::new(),
    };
    response.snapshot_sha256 = utility_snapshot_sha256(&response).map_err(ApiError::internal)?;
    Ok(response)
}

async fn get_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<UtilityBillingResponse>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store
            .get_invoice_response(id)
            .await
            .map_err(storage_error)?
            .map(Json)
            .ok_or_else(|| ApiError::not_found("INVOICE_NOT_FOUND", "invoice not found"));
    }
    state
        .invoices
        .read()
        .await
        .get(&id)
        .map(|item| Json(item.response.clone()))
        .ok_or_else(|| ApiError::not_found("INVOICE_NOT_FOUND", "invoice not found"))
}

async fn get_document_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentData>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        let invoice = store
            .get_invoice_response(id)
            .await
            .map_err(storage_error)?
            .ok_or_else(|| ApiError::not_found("INVOICE_NOT_FOUND", "invoice not found"))?;
        let reference = format!("SEL-V2-{id}");
        return Ok(Json(DocumentData {
            document_schema_version: 1,
            template_version: "v2-a4-1".into(),
            document_reference: reference.clone(),
            invoice,
            qr_internal_reference: reference,
            origin: "CENTRAL_ACCEPTED".into(),
        }));
    }
    let invoice = state
        .invoices
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("INVOICE_NOT_FOUND", "invoice not found"))?;
    let reference = format!("SEL-V2-{id}");
    Ok(Json(DocumentData {
        document_schema_version: 1,
        template_version: "v2-a4-1".into(),
        document_reference: reference.clone(),
        invoice: invoice.response,
        qr_internal_reference: reference,
        origin: "CENTRAL_ACCEPTED".into(),
    }))
}

async fn void_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invoice_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<VoidInvoiceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    let header_actor_id = headers
        .get("X-Actor-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .parse::<Uuid>()
                .map_err(|_| ApiError::unauthorized("ACTOR_INVALID", "X-Actor-Id must be a UUID"))
        })
        .transpose()?;
    let actor_id = if let Some(Extension(AuthenticatedPrincipal(principal))) = principal {
        if let Some(header_actor_id) = header_actor_id {
            if header_actor_id != principal.user_id {
                return Err(ApiError::forbidden(
                    "ACTOR_MISMATCH",
                    "X-Actor-Id does not match the authenticated session",
                ));
            }
        }
        principal.user_id
    } else {
        header_actor_id.ok_or_else(|| {
            ApiError::unauthorized(
                "ACTOR_REQUIRED",
                "X-Actor-Id is required for an audited invoice void",
            )
        })?
    };
    if input.reason.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "INVOICE_VOID_INVALID",
            "a non-empty reason is required",
        ));
    }
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "INVOICE_VOID_CENTRAL_REQUIRED",
            "audited invoice voids require a central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .void_invoice(invoice_id, actor_id, &input.reason, key, &digest)
        .await
        .map_err(invoice_void_error)?
    {
        sel_storage::postgres::InvoiceVoidOutcome::Created(body)
        | sel_storage::postgres::InvoiceVoidOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn record_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RecordPaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::unprocessable("IDEMPOTENCY_REQUIRED", "Idempotency-Key is required")
        })?;
    let digest = digest_json(&input)?;
    if let Some(original) = state.idempotency.read().await.get(key) {
        if original.digest != digest {
            return Err(ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            ));
        }
        return Ok((original.response_status, Json(original.response.clone())));
    }
    if input.received_amount <= Decimal::ZERO || input.allocations.is_empty() {
        return Err(ApiError::unprocessable(
            "PAYMENT_INVALID",
            "payment amount and allocations are required",
        ));
    }
    if input
        .allocations
        .iter()
        .any(|item| item.amount <= Decimal::ZERO)
    {
        return Err(ApiError::unprocessable(
            "PAYMENT_ALLOCATION_INVALID",
            "allocation amounts must be positive",
        ));
    }
    if let Some(store) = &state.central {
        return match store
            .record_payment(&input, key, &digest)
            .await
            .map_err(payment_write_error)?
        {
            sel_storage::postgres::PaymentWriteOutcome::Created(body) => {
                Ok((StatusCode::CREATED, Json(body)))
            }
            sel_storage::postgres::PaymentWriteOutcome::Replay(body) => {
                Ok((StatusCode::OK, Json(body)))
            }
        };
    }
    let receivables = state.receivables.read().await;
    let payments = state.payments.read().await;
    let mut allocated: Decimal = Decimal::ZERO;
    for allocation in &input.allocations {
        let receivable = receivables
            .get(&allocation.receivable_id)
            .ok_or_else(|| ApiError::not_found("RECEIVABLE_NOT_FOUND", "receivable not found"))?;
        if receivable.customer_id != input.customer_id || receivable.currency != input.currency {
            return Err(ApiError::unprocessable(
                "PAYMENT_CONTEXT_INVALID",
                "payment customer or currency does not match the receivable",
            ));
        }
        let prior: Decimal = payments
            .values()
            .filter(|payment| {
                payment
                    .allocations
                    .iter()
                    .any(|item| item.receivable_id == allocation.receivable_id)
            })
            .flat_map(|payment| {
                payment
                    .allocations
                    .iter()
                    .filter(|item| item.receivable_id == allocation.receivable_id)
                    .map(|item| item.amount)
            })
            .sum();
        if prior + allocation.amount > receivable.principal {
            return Err(ApiError::conflict(
                "RECEIVABLE_OVERALLOCATED",
                "payment would exceed the receivable balance",
            ));
        }
        allocated += allocation.amount;
    }
    drop(payments);
    drop(receivables);
    if allocated > input.received_amount {
        return Err(ApiError::unprocessable(
            "PAYMENT_ALLOCATION_EXCEEDS_RECEIPT",
            "allocations exceed received amount",
        ));
    }
    let payment_id = Uuid::new_v4();
    let body = json!({"payment_id":payment_id,"status":"CENTRAL_ACCEPTED","amount":input.received_amount,"currency":input.currency});
    state.payments.write().await.insert(
        payment_id,
        PaymentRecord {
            id: payment_id,
            customer_id: input.customer_id,
            amount: input.received_amount,
            allocations: input.allocations,
        },
    );
    state.idempotency.write().await.insert(
        key.to_owned(),
        IdempotencyRecord {
            digest,
            response_status: StatusCode::CREATED,
            response: body.clone(),
        },
    );
    Ok((StatusCode::CREATED, Json(body)))
}

async fn reverse_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(payment_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<ReversePaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    let header_actor_id = headers
        .get("X-Actor-Id")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .parse::<Uuid>()
                .map_err(|_| ApiError::unauthorized("ACTOR_INVALID", "X-Actor-Id must be a UUID"))
        })
        .transpose()?;
    let actor_id = if let Some(Extension(AuthenticatedPrincipal(principal))) = principal {
        if let Some(header_actor_id) = header_actor_id {
            if header_actor_id != principal.user_id {
                return Err(ApiError::forbidden(
                    "ACTOR_MISMATCH",
                    "X-Actor-Id does not match the authenticated session",
                ));
            }
        }
        principal.user_id
    } else {
        header_actor_id.ok_or_else(|| {
            ApiError::unauthorized(
                "ACTOR_REQUIRED",
                "X-Actor-Id is required for an audited financial reversal",
            )
        })?
    };
    if input.amount <= Decimal::ZERO || input.reason.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "PAYMENT_REVERSAL_INVALID",
            "a positive amount and a non-empty reason are required",
        ));
    }
    let digest = digest_json(&input)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "PAYMENT_REVERSAL_CENTRAL_REQUIRED",
            "audited payment reversals require a central PostgreSQL store",
        ));
    };
    match store
        .reverse_payment(payment_id, &input, actor_id, key, &digest)
        .await
        .map_err(payment_reverse_error)?
    {
        sel_storage::postgres::PaymentReverseOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::PaymentReverseOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn get_receivable(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store
            .get_receivable_summary(id)
            .await
            .map_err(storage_error)?
            .map(Json)
            .ok_or_else(|| ApiError::not_found("RECEIVABLE_NOT_FOUND", "receivable not found"));
    }
    let receivable = state
        .receivables
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("RECEIVABLE_NOT_FOUND", "receivable not found"))?;
    let paid: Decimal = state
        .payments
        .read()
        .await
        .values()
        .flat_map(|payment| payment.allocations.iter())
        .filter(|allocation| allocation.receivable_id == id)
        .map(|allocation| allocation.amount)
        .sum();
    Ok(Json(
        json!({"id":receivable.id,"customer_id":receivable.customer_id,"currency":receivable.currency,"principal":receivable.principal,"paid":paid,"balance":receivable.principal - paid,"status":if paid >= receivable.principal { "SETTLED" } else { "OPEN" }}),
    ))
}

async fn get_statement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store
            .customer_statement(customer_id)
            .await
            .map_err(storage_error)?
            .map(Json)
            .ok_or_else(|| ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found"));
    }
    let receivables = state
        .receivables
        .read()
        .await
        .values()
        .filter(|item| item.customer_id == customer_id)
        .cloned()
        .collect::<Vec<_>>();
    let payments = state.payments.read().await;
    let entries = receivables.into_iter().map(|receivable| {
        let paid: Decimal = payments.values().flat_map(|payment| payment.allocations.iter()).filter(|allocation| allocation.receivable_id == receivable.id).map(|allocation| allocation.amount).sum();
        json!({"receivable_id":receivable.id,"currency":receivable.currency,"principal":receivable.principal,"paid":paid,"balance":receivable.principal-paid})
    }).collect::<Vec<_>>();
    Ok(Json(
        json!({"customer_id":customer_id,"receivables":entries}),
    ))
}

async fn sync_push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SyncPushRequest>,
) -> Result<Json<SyncPushResponse>, ApiError> {
    require_auth(&headers)?;
    require_idempotency(&headers)?;
    if input.events.len() > 100 {
        return Err(ApiError::unprocessable(
            "SYNC_BATCH_TOO_LARGE",
            "sync batch is limited to 100 events",
        ));
    }
    if input.events.iter().any(|event| {
        event.device_id != input.device_id || event.organization_id != state.organization_id
    }) {
        return Err(ApiError::unauthorized(
            "SYNC_CONTEXT_INVALID",
            "sync device or organization context is invalid",
        ));
    }
    if let Some(store) = &state.central {
        let mut results = Vec::with_capacity(input.events.len());
        for event in &input.events {
            results.push(
                store
                    .apply_sync_event(event)
                    .await
                    .map_err(sync_apply_error)?,
            );
        }
        return Ok(Json(SyncPushResponse { results }));
    }
    let results = state
        .sync
        .write()
        .await
        .push(&input.events)
        .map_err(|error| ApiError::unprocessable("SYNC_EVENT_INVALID", error.to_string()))?;
    Ok(Json(SyncPushResponse { results }))
}

#[derive(serde::Deserialize)]
struct SyncQuery {
    cursor: Option<i64>,
    limit: Option<usize>,
}

async fn sync_pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SyncQuery>,
) -> Result<Json<SyncPullResponse>, ApiError> {
    require_auth(&headers)?;
    let cursor = query.cursor.unwrap_or(0);
    if cursor < 0 {
        return Err(ApiError::unprocessable(
            "SYNC_CURSOR_INVALID",
            "cursor cannot be negative",
        ));
    }
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);
    if let Some(store) = &state.central {
        let (changes, next_cursor) = store
            .pull_changes(cursor, limit as i64)
            .await
            .map_err(storage_error)?;
        return Ok(Json(SyncPullResponse {
            changes,
            next_cursor,
        }));
    }
    let (changes, next_cursor) = state.sync.read().await.pull(cursor, limit);
    let changes = changes
        .into_iter()
        .map(|change| serde_json::to_value(change).map_err(ApiError::internal))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(SyncPullResponse {
        changes,
        next_cursor,
    }))
}

async fn sync_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    if let Some(store) = &state.central {
        return store.sync_status().await.map(Json).map_err(storage_error);
    }
    Ok(Json(
        json!({"protocol_version":1,"state":"READY","pending":state.sync.read().await.pending(),"conflicts":0,"rejected":0,"last_successful_sync":null}),
    ))
}

async fn config_versions(headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    Ok(Json(
        json!({"protocol_version":1,"document_schema_version":1,"configuration_revision":null,"safe_update_required":false,"source":"central"}),
    ))
}

async fn resource_manifest(headers: HeaderMap) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    Ok(Json(
        json!({"channel":"stable","sequence":0,"resources":[],"status":"NO_TRUSTED_BUNDLE_PUBLISHED"}),
    ))
}

async fn create_solar_sale(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateSolarSaleRequest>,
) -> Result<(StatusCode, Json<SolarSaleResponse>), ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    if input.principal <= Decimal::ZERO
        || input.product_code.trim().is_empty()
        || input.currency.trim().is_empty()
    {
        return Err(ApiError::unprocessable(
            "SOLAR_SALE_INVALID",
            "solar product, currency and a positive principal are required",
        ));
    }
    let digest = digest_json(&input)?;
    if let Some(store) = &state.central {
        return match store
            .create_solar_sale(&input, key, &digest)
            .await
            .map_err(solar_sale_write_error)?
        {
            sel_storage::postgres::SolarSaleWriteOutcome::Created(response) => {
                Ok((StatusCode::CREATED, Json(response)))
            }
            sel_storage::postgres::SolarSaleWriteOutcome::Replay(response) => {
                Ok((StatusCode::OK, Json(response)))
            }
        };
    }
    let sale_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let response = SolarSaleResponse {
        sale_id,
        receivable_id,
        customer_id: input.customer_id,
        principal: input.principal,
        currency: input.currency.clone(),
        status: "CENTRAL_ACCEPTED".into(),
        product_code: Some(input.product_code.clone()),
    };
    state
        .solar_sales
        .write()
        .await
        .insert(sale_id, response.clone());
    state.receivables.write().await.insert(
        receivable_id,
        ReceivableRecord {
            id: receivable_id,
            customer_id: input.customer_id,
            currency: input.currency,
            principal: input.principal,
        },
    );
    Ok((StatusCode::CREATED, Json(response)))
}

async fn activate_solar_contract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<sel_contracts::ActivateSolarContractRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    let digest = digest_json(&input)?;
    if let Some(store) = &state.central {
        return match store
            .activate_solar_contract(id, &input, key, &digest)
            .await
            .map_err(solar_contract_write_error)?
        {
            sel_storage::postgres::SolarContractWriteOutcome::Created(body)
            | sel_storage::postgres::SolarContractWriteOutcome::Replay(body) => Ok(Json(body)),
        };
    }
    let receivable = state
        .receivables
        .read()
        .await
        .get(&input.receivable_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("RECEIVABLE_NOT_FOUND", "receivable not found"))?;
    if receivable.currency != input.currency {
        return Err(ApiError::unprocessable(
            "CURRENCY_MISMATCH",
            "schedule currency does not match the receivable",
        ));
    }
    let schedule = input
        .schedule
        .iter()
        .map(|item| solar::Installment {
            sequence: item.sequence,
            due_date: item.due_date.to_string(),
            amount: item.amount,
        })
        .collect::<Vec<_>>();
    solar::validate_schedule(&solar::SolarSchedule {
        principal: receivable.principal,
        currency: receivable.currency.clone(),
        installments: schedule,
    })
    .map_err(|error| ApiError::unprocessable("SOLAR_SCHEDULE_INVALID", error.to_string()))?;
    Ok(Json(
        json!({"status":"CENTRAL_ACCEPTED","sale_id":input.sale_id,"receivable_id":input.receivable_id,"installment_plan_id":Uuid::new_v4(),"principal":receivable.principal,"currency":receivable.currency,"schedule":input.schedule}),
    ))
}

fn require_auth(headers: &HeaderMap) -> Result<(), ApiError> {
    match headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
    {
        Some(value) if value.starts_with("Bearer ") && value.len() > 7 => Ok(()),
        _ => Err(ApiError::unauthorized(
            "AUTH_REQUIRED",
            "a bearer token is required",
        )),
    }
}

fn require_principal(
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<sel_storage::postgres::AuthSessionPrincipal, ApiError> {
    principal
        .map(|Extension(AuthenticatedPrincipal(principal))| principal)
        .ok_or_else(|| {
            ApiError::unauthorized(
                "SESSION_REQUIRED",
                "individual session authentication is required",
            )
        })
}

fn require_idempotency(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() >= 8)
        .ok_or_else(|| {
            ApiError::unprocessable(
                "IDEMPOTENCY_REQUIRED",
                "Idempotency-Key is required for this write",
            )
        })
}

fn digest_json<T: serde::Serialize>(value: &T) -> Result<String, ApiError> {
    Ok(digest_bytes(
        &serde_json::to_vec(value).map_err(ApiError::internal)?,
    ))
}

fn digest_bytes(value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(value);
    hex::encode(digest.finalize())
}

fn auth_error(error: sel_storage::postgres::AuthError) -> ApiError {
    match error {
        sel_storage::postgres::AuthError::InvalidCredentials => {
            ApiError::unauthorized("INVALID_CREDENTIALS", "identity_ref or password is invalid")
        }
        sel_storage::postgres::AuthError::InvalidSession => {
            ApiError::unauthorized("INVALID_SESSION", "session is invalid, revoked or expired")
        }
        sel_storage::postgres::AuthError::PasswordHash(_) => {
            ApiError::internal("password verification failed")
        }
        sel_storage::postgres::AuthError::Database(error) => storage_error(error),
        sel_storage::postgres::AuthError::InvalidResponse(error) => ApiError::internal(error),
        sel_storage::postgres::AuthError::UserNotFound => {
            ApiError::not_found("USER_NOT_FOUND", "active user not found")
        }
    }
}

fn device_write_error(error: sel_storage::postgres::DeviceWriteError) -> ApiError {
    match error {
        sel_storage::postgres::DeviceWriteError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        sel_storage::postgres::DeviceWriteError::IdentityMismatch => ApiError::conflict(
            "DEVICE_IDENTITY_CONFLICT",
            "device identity is already bound to another key or device",
        ),
        sel_storage::postgres::DeviceWriteError::AlreadyEnrolled => {
            ApiError::conflict("DEVICE_ALREADY_ENROLLED", "device is already enrolled")
        }
        sel_storage::postgres::DeviceWriteError::NotFound => {
            ApiError::not_found("DEVICE_NOT_FOUND", "device not found")
        }
        sel_storage::postgres::DeviceWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::DeviceWriteError::StoredResponse(error) => ApiError::internal(error),
    }
}

fn offline_grant_write_error(error: sel_storage::postgres::OfflineGrantWriteError) -> ApiError {
    match error {
        sel_storage::postgres::OfflineGrantWriteError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::OfflineGrantWriteError::ContextInvalid => ApiError::unprocessable(
            "OFFLINE_GRANT_CONTEXT_INVALID",
            "user and device must be active in the same organization",
        ),
        sel_storage::postgres::OfflineGrantWriteError::InvalidDates => ApiError::unprocessable(
            "OFFLINE_GRANT_DATES_INVALID",
            "expires_at must be later than issued_at",
        ),
        sel_storage::postgres::OfflineGrantWriteError::InvalidSignature => {
            ApiError::unprocessable("OFFLINE_GRANT_SIGNATURE_INVALID", "signature is invalid")
        }
        sel_storage::postgres::OfflineGrantWriteError::UntrustedSignature => ApiError::forbidden(
            "OFFLINE_GRANT_SIGNATURE_UNTRUSTED",
            "signature is not trusted by the server",
        ),
        sel_storage::postgres::OfflineGrantWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::OfflineGrantWriteError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn storage_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(database) = &error {
        if database.constraint() == Some("customers_organization_id_customer_code_key") {
            return ApiError::conflict("CUSTOMER_CODE_EXISTS", "customer code already exists");
        }
    }
    ApiError::internal(error)
}

fn utility_issue_error(error: sel_storage::postgres::UtilityIssueError) -> ApiError {
    match error {
        sel_storage::postgres::UtilityIssueError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        sel_storage::postgres::UtilityIssueError::InvoicePeriodExists => ApiError::conflict(
            "INVOICE_PERIOD_EXISTS",
            "an active invoice already exists for this customer and period",
        ),
        sel_storage::postgres::UtilityIssueError::Database(sqlx::Error::RowNotFound) => {
            ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found")
        }
        sel_storage::postgres::UtilityIssueError::Database(error) => storage_error(error),
        sel_storage::postgres::UtilityIssueError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn payment_write_error(error: sel_storage::postgres::PaymentWriteError) -> ApiError {
    match error {
        sel_storage::postgres::PaymentWriteError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        sel_storage::postgres::PaymentWriteError::Overallocated => ApiError::conflict(
            "RECEIVABLE_OVERALLOCATED",
            "payment would exceed the receivable balance",
        ),
        sel_storage::postgres::PaymentWriteError::ContextInvalid => ApiError::unprocessable(
            "PAYMENT_CONTEXT_INVALID",
            "payment customer or currency does not match the receivable",
        ),
        sel_storage::postgres::PaymentWriteError::AllocationExceedsReceipt => {
            ApiError::unprocessable(
                "PAYMENT_ALLOCATION_EXCEEDS_RECEIPT",
                "allocations exceed received amount",
            )
        }
        sel_storage::postgres::PaymentWriteError::InvalidAllocation => ApiError::unprocessable(
            "PAYMENT_ALLOCATION_INVALID",
            "payment amount and allocations must be strictly positive",
        ),
        sel_storage::postgres::PaymentWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::PaymentWriteError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn payment_reverse_error(error: sel_storage::postgres::PaymentReverseError) -> ApiError {
    match error {
        sel_storage::postgres::PaymentReverseError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::PaymentReverseError::PaymentNotFound => {
            ApiError::not_found("PAYMENT_NOT_FOUND", "payment not found")
        }
        sel_storage::postgres::PaymentReverseError::PaymentNotAccepted => ApiError::conflict(
            "PAYMENT_NOT_REVERSIBLE",
            "only a centrally accepted payment can be reversed",
        ),
        sel_storage::postgres::PaymentReverseError::ActorNotActive => ApiError::unauthorized(
            "ACTOR_NOT_ACTIVE",
            "the reversal actor is not active in this organization",
        ),
        sel_storage::postgres::PaymentReverseError::ActorNotAuthorized => ApiError::forbidden(
            "ACTOR_NOT_AUTHORIZED",
            "the actor role cannot perform a financial reversal",
        ),
        sel_storage::postgres::PaymentReverseError::InvalidAmount => ApiError::unprocessable(
            "PAYMENT_REVERSAL_INVALID",
            "reversal amount must be strictly positive",
        ),
        sel_storage::postgres::PaymentReverseError::AmountExceedsRemaining => ApiError::conflict(
            "PAYMENT_REVERSAL_OVERALLOCATED",
            "reversal exceeds the payment amount still reversible",
        ),
        sel_storage::postgres::PaymentReverseError::ReasonRequired => ApiError::unprocessable(
            "PAYMENT_REVERSAL_REASON_REQUIRED",
            "a reason is required for an audited reversal",
        ),
        sel_storage::postgres::PaymentReverseError::Database(error) => storage_error(error),
        sel_storage::postgres::PaymentReverseError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn invoice_void_error(error: sel_storage::postgres::InvoiceVoidError) -> ApiError {
    match error {
        sel_storage::postgres::InvoiceVoidError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        sel_storage::postgres::InvoiceVoidError::InvoiceNotFound => {
            ApiError::not_found("INVOICE_NOT_FOUND", "invoice not found")
        }
        sel_storage::postgres::InvoiceVoidError::AlreadyVoided => {
            ApiError::conflict("INVOICE_ALREADY_VOID", "invoice is already void")
        }
        sel_storage::postgres::InvoiceVoidError::HasPayments => ApiError::conflict(
            "INVOICE_HAS_PAYMENTS",
            "an invoice with accepted payments requires a separate audited credit/refund workflow",
        ),
        sel_storage::postgres::InvoiceVoidError::ActorNotActive => {
            ApiError::unauthorized("ACTOR_NOT_ACTIVE", "the actor is not active")
        }
        sel_storage::postgres::InvoiceVoidError::ActorNotAuthorized => ApiError::forbidden(
            "ACTOR_NOT_AUTHORIZED",
            "the actor role cannot void an invoice",
        ),
        sel_storage::postgres::InvoiceVoidError::ReasonRequired => ApiError::unprocessable(
            "INVOICE_VOID_REASON_REQUIRED",
            "a reason is required for an audited void",
        ),
        sel_storage::postgres::InvoiceVoidError::Database(error) => storage_error(error),
        sel_storage::postgres::InvoiceVoidError::StoredResponse(error) => ApiError::internal(error),
    }
}

fn solar_sale_write_error(error: sel_storage::postgres::SolarSaleWriteError) -> ApiError {
    match error {
        sel_storage::postgres::SolarSaleWriteError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::SolarSaleWriteError::CustomerNotFound => {
            ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found")
        }
        sel_storage::postgres::SolarSaleWriteError::ProductNotFound => ApiError::unprocessable(
            "SOLAR_PRODUCT_NOT_FOUND",
            "an active solar kit product is required",
        ),
        sel_storage::postgres::SolarSaleWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::SolarSaleWriteError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn solar_contract_write_error(error: sel_storage::postgres::SolarContractWriteError) -> ApiError {
    match error {
        sel_storage::postgres::SolarContractWriteError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::SolarContractWriteError::SaleNotFound => {
            ApiError::not_found("SOLAR_SALE_NOT_FOUND", "solar sale or receivable not found")
        }
        sel_storage::postgres::SolarContractWriteError::ContractAlreadyExists => {
            ApiError::conflict(
                "SOLAR_CONTRACT_ALREADY_EXISTS",
                "the solar sale already has an installment contract",
            )
        }
        sel_storage::postgres::SolarContractWriteError::CurrencyMismatch => {
            ApiError::unprocessable(
                "CURRENCY_MISMATCH",
                "schedule currency does not match the receivable",
            )
        }
        sel_storage::postgres::SolarContractWriteError::InvalidSchedule(detail) => {
            ApiError::unprocessable("SOLAR_SCHEDULE_INVALID", detail)
        }
        sel_storage::postgres::SolarContractWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::SolarContractWriteError::StoredResponse(error) => {
            ApiError::internal(error)
        }
    }
}

fn sync_apply_error(error: sel_storage::postgres::SyncApplyError) -> ApiError {
    match error {
        sel_storage::postgres::SyncApplyError::Database(error) => storage_error(error),
        sel_storage::postgres::SyncApplyError::StoredResult(error) => ApiError::internal(error),
    }
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    detail: String,
}

impl ApiError {
    fn unauthorized(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code,
            detail: detail.into(),
        }
    }
    fn not_found(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            detail: detail.into(),
        }
    }
    fn forbidden(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            detail: detail.into(),
        }
    }
    fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            detail: detail.into(),
        }
    }
    fn unprocessable(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code,
            detail: detail.into(),
        }
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL_ERROR",
            detail: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = Uuid::new_v4().to_string();
        let body = Json(ProblemDetails {
            problem_type: format!("https://sel.local/problems/{}", self.code.to_lowercase()),
            title: self.code.to_owned(),
            status: self.status.as_u16(),
            code: self.code.to_owned(),
            detail: self.detail,
            request_id,
        });
        let mut response = (self.status, body).into_response();
        response.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::role_allows_request;
    use axum::http::Method;

    #[test]
    fn production_role_matrix_keeps_read_access_separate_from_writes() {
        assert!(role_allows_request(
            &Method::GET,
            "/v1/customers",
            "AUDITOR_READONLY"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/customers",
            "AUDITOR_READONLY"
        ));
        assert!(role_allows_request(
            &Method::POST,
            "/v1/payments",
            "CASHIER"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/payments",
            "BILLING_AGENT"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/billing/utility/issue",
            "CASHIER"
        ));
    }
}
