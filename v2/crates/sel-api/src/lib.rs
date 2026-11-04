use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use rust_decimal::Decimal;
use sel_contracts::{
    catalog_snapshot_sha256, utility_snapshot_sha256, CatalogSnapshot, CatalogSnapshotPayload,
    CorrectMeterReading, CreateCustomer, CreateMeter, CreateMeterReading, CreateServiceAccount,
    CreateSolarQuoteRequest, CreateSolarSaleRequest, CreditNoteRequest, Customer, DocumentData,
    InvoiceLineResponse, Meter, MeterReading, PaymentAllocationRequest, ProblemDetails,
    RecordPaymentRequest, ReversePaymentRequest, SolarQuoteResponse, SolarSaleResponse,
    SyncPullResponse, SyncPushRequest, SyncPushResponse, UpdateCustomer, UpdateServiceAccount,
    UtilityBillingRequest, UtilityBillingResponse, VoidInvoiceRequest,
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
pub struct CreateAdminUserRequest {
    pub identity_ref: String,
    pub role: String,
    pub password: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct UpdateAdminUserRequest {
    pub role: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateTariffDraftRequest {
    pub service_code: String,
    pub customer_category: String,
    pub unit: String,
    pub unit_price: Decimal,
    pub currency: String,
    pub effective_from: chrono::NaiveDate,
    pub effective_to: Option<chrono::NaiveDate>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ApproveTariffDraftRequest {
    pub approval_ref: String,
    #[serde(default)]
    pub supersedes_tariff_id: Option<Uuid>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RevokeTariffRequest {
    pub change_ref: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateSolarProductDraftRequest {
    pub sku: String,
    pub label: String,
    pub price_reference: Decimal,
    pub currency: String,
    pub effective_from: chrono::NaiveDate,
    pub effective_to: Option<chrono::NaiveDate>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ApproveSolarProductRequest {
    pub approval_ref: String,
    #[serde(default)]
    pub supersedes_product_id: Option<Uuid>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateTaxProfileDraftRequest {
    pub service_code: String,
    pub jurisdiction: String,
    pub base: String,
    pub rate: Decimal,
    pub source: String,
    pub effective_from: chrono::NaiveDate,
    pub effective_to: Option<chrono::NaiveDate>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CreateFxRateDraftRequest {
    pub from_currency: String,
    pub to_currency: String,
    pub value: Decimal,
    pub source: String,
    pub effective_from: chrono::NaiveDate,
    pub effective_to: Option<chrono::NaiveDate>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ApproveReferenceDraftRequest {
    pub approval_ref: String,
    #[serde(default)]
    pub supersedes_id: Option<Uuid>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct EnrollDeviceRequest {
    pub device_id: Uuid,
    pub public_key_id: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
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

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AllocatePaymentRequest {
    pub allocations: Vec<PaymentAllocationRequest>,
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
        .route(
            "/v1/admin/users",
            get(admin_list_users).post(admin_create_user),
        )
        .route("/v1/admin/users/:id", patch(admin_update_user))
        .route("/v1/admin/devices", get(admin_list_devices))
        .route("/v1/admin/offline-grants", get(admin_list_offline_grants))
        .route(
            "/v1/admin/offline-grants/:id/revoke",
            post(admin_revoke_offline_grant),
        )
        .route("/v1/devices/enroll", post(enroll_device))
        .route("/v1/devices/:id/revoke", post(revoke_device))
        .route("/v1/offline-grants", post(create_offline_grant))
        .route(
            "/v1/admin/catalog/tariffs",
            get(admin_catalog_tariffs).post(create_tariff_draft),
        )
        .route(
            "/v1/admin/catalog/tariffs/:id/approve",
            post(approve_tariff_draft),
        )
        .route("/v1/admin/catalog/tariffs/:id/revoke", post(revoke_tariff))
        .route(
            "/v1/admin/catalog/products",
            get(admin_catalog_products).post(create_solar_product_draft),
        )
        .route(
            "/v1/admin/catalog/products/:id/approve",
            post(approve_solar_product),
        )
        .route(
            "/v1/admin/catalog/products/:id/revoke",
            post(revoke_solar_product),
        )
        .route(
            "/v1/admin/catalog/tax-profiles",
            get(admin_tax_profiles).post(create_tax_profile_draft),
        )
        .route(
            "/v1/admin/catalog/tax-profiles/:id/approve",
            post(approve_tax_profile),
        )
        .route(
            "/v1/admin/catalog/tax-profiles/:id/revoke",
            post(revoke_tax_profile),
        )
        .route(
            "/v1/admin/catalog/fx-rates",
            get(admin_fx_rates).post(create_fx_rate_draft),
        )
        .route(
            "/v1/admin/catalog/fx-rates/:id/approve",
            post(approve_fx_rate),
        )
        .route(
            "/v1/admin/catalog/fx-rates/:id/revoke",
            post(revoke_fx_rate),
        )
        .route("/v1/meta/capabilities", get(capabilities))
        .route("/v1/catalog/products", get(catalog_products))
        .route("/v1/catalog/tariffs", get(catalog_tariffs))
        .route("/v1/catalog/tax-profiles", get(catalog_tax_profiles))
        .route("/v1/catalog/fx-rates", get(catalog_fx_rates))
        .route("/v1/catalog/snapshot", get(catalog_snapshot))
        .route("/v1/customers", get(list_customers).post(create_customer))
        .route(
            "/v1/customers/:id",
            get(get_customer).patch(update_customer),
        )
        .route(
            "/v1/customers/:id/service-accounts",
            get(list_service_accounts).post(create_service_account),
        )
        .route(
            "/v1/service-accounts/:id",
            get(get_service_account).patch(update_service_account),
        )
        .route(
            "/v1/service-accounts/:id/meters",
            get(list_meters).post(create_meter),
        )
        .route(
            "/v1/meters/:id/readings",
            get(list_meter_readings).post(create_meter_reading),
        )
        .route(
            "/v1/meter-readings/:id/corrections",
            post(correct_meter_reading),
        )
        .route("/v1/billing/utility/preview", post(preview_utility))
        .route("/v1/billing/utility/issue", post(issue_utility))
        .route("/v1/invoices", get(list_invoices))
        .route("/v1/invoices/:id", get(get_invoice))
        .route("/v1/invoices/:id/document-data", get(get_document_data))
        .route("/v1/invoices/:id/void", post(void_invoice))
        .route("/v1/payments", get(list_payments).post(record_payment))
        .route(
            "/v1/payments/:id/document-data",
            get(get_payment_document_data),
        )
        .route("/v1/payments/:id/allocate", post(allocate_payment))
        .route("/v1/payments/:id/reverse", post(reverse_payment))
        .route("/v1/receivables/:id", get(get_receivable))
        .route("/v1/installment-plans/:id", get(get_installment_plan))
        .route("/v1/receivables/:id/credit", post(create_credit_note))
        .route("/v1/customers/:id/statement", get(get_statement))
        .route("/v1/sync/push", post(sync_push))
        .route("/v1/sync/pull", get(sync_pull))
        .route("/v1/sync/status", get(sync_status))
        .route("/v1/config/versions", get(config_versions))
        .route("/v1/client-resources/manifest", get(resource_manifest))
        .route("/v1/solar/sales", post(create_solar_sale))
        .route("/v1/solar/quotes", post(create_solar_quote))
        .route("/v1/solar/contracts", post(create_solar_contract))
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
    use axum::http::Method;

    let is_admin = role == "ADMIN";
    let is_supervisor = role == "SUPERVISOR";
    let is_billing = role == "BILLING_AGENT";
    let is_cashier = role == "CASHIER";
    let is_reader = matches!(
        role,
        "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
    );

    if *method == Method::GET {
        if matches!(
            path,
            "/v1/admin/users" | "/v1/admin/devices" | "/v1/admin/offline-grants"
        ) || matches!(
            path,
            "/v1/admin/catalog/tariffs"
                | "/v1/admin/catalog/products"
                | "/v1/admin/catalog/tax-profiles"
                | "/v1/admin/catalog/fx-rates"
        ) {
            return is_admin;
        }
        return is_reader
            && (matches!(
                path,
                "/v1/meta/capabilities"
                    | "/v1/catalog/products"
                    | "/v1/catalog/tariffs"
                    | "/v1/catalog/tax-profiles"
                    | "/v1/catalog/fx-rates"
                    | "/v1/catalog/snapshot"
                    | "/v1/customers"
                    | "/v1/service-accounts/:id/meters"
                    | "/v1/meters/:id/readings"
                    | "/v1/invoices"
                    | "/v1/payments"
                    | "/v1/sync/pull"
                    | "/v1/sync/status"
                    | "/v1/config/versions"
                    | "/v1/client-resources/manifest"
            ) || path_matches(path, "/v1/customers/:id")
                || path_matches(path, "/v1/customers/:id/service-accounts")
                || path_matches(path, "/v1/service-accounts/:id")
                || path_matches(path, "/v1/service-accounts/:id/meters")
                || path_matches(path, "/v1/meters/:id/readings")
                || path_matches(path, "/v1/customers/:id/statement")
                || path_matches(path, "/v1/invoices/:id")
                || path_matches(path, "/v1/invoices/:id/document-data")
                || path_matches(path, "/v1/payments/:id/document-data")
                || path_matches(path, "/v1/receivables/:id")
                || path_matches(path, "/v1/installment-plans/:id"));
    }

    if path == "/v1/admin/users" && *method == Method::POST {
        return is_admin;
    }
    if path_matches(path, "/v1/admin/users/:id") && *method == Method::PATCH {
        return is_admin;
    }
    if path_matches(path, "/v1/admin/offline-grants/:id/revoke") && *method == Method::POST {
        return is_admin;
    }

    if *method == Method::POST {
        if path == "/v1/auth/logout" {
            return is_reader;
        }
        if path_matches(path, "/v1/users/:id/password") {
            return is_admin;
        }
        if matches!(path, "/v1/devices/enroll" | "/v1/offline-grants")
            || path_matches(path, "/v1/devices/:id/revoke")
        {
            return is_admin || is_supervisor;
        }
        if path == "/v1/admin/catalog/tariffs"
            || path_matches(path, "/v1/admin/catalog/tariffs/:id/approve")
            || path_matches(path, "/v1/admin/catalog/tariffs/:id/revoke")
            || path == "/v1/admin/catalog/products"
            || path_matches(path, "/v1/admin/catalog/products/:id/approve")
            || path_matches(path, "/v1/admin/catalog/products/:id/revoke")
            || path == "/v1/admin/catalog/tax-profiles"
            || path_matches(path, "/v1/admin/catalog/tax-profiles/:id/approve")
            || path_matches(path, "/v1/admin/catalog/tax-profiles/:id/revoke")
            || path == "/v1/admin/catalog/fx-rates"
            || path_matches(path, "/v1/admin/catalog/fx-rates/:id/approve")
            || path_matches(path, "/v1/admin/catalog/fx-rates/:id/revoke")
        {
            return is_admin;
        }
        if path == "/v1/sync/push" {
            return is_admin || is_supervisor || is_billing || is_cashier;
        }
        if matches!(
            path,
            "/v1/customers"
                | "/v1/billing/utility/preview"
                | "/v1/billing/utility/issue"
                | "/v1/solar/sales"
                | "/v1/solar/quotes"
                | "/v1/solar/contracts"
        ) || path_matches(path, "/v1/customers/:id/service-accounts")
            || path_matches(path, "/v1/service-accounts/:id/meters")
            || path_matches(path, "/v1/meters/:id/readings")
            || path_matches(path, "/v1/solar/contracts/:id/activate")
        {
            return is_admin || is_supervisor || is_billing;
        }
        if path_matches(path, "/v1/customers/:id") {
            return is_admin || is_supervisor || is_billing;
        }
        if path_matches(path, "/v1/meter-readings/:id/corrections") {
            return is_admin || is_supervisor;
        }
        if path == "/v1/payments"
            || path_matches(path, "/v1/payments/:id/allocate")
            || path_matches(path, "/v1/payments/:id/reverse")
        {
            return is_admin || is_supervisor || is_cashier;
        }
        if path_matches(path, "/v1/invoices/:id/void")
            || path_matches(path, "/v1/receivables/:id/credit")
        {
            return is_admin || is_supervisor;
        }
    }

    if *method == Method::PATCH && path_matches(path, "/v1/customers/:id") {
        return is_admin || is_supervisor || is_billing;
    }
    if *method == Method::PATCH && path_matches(path, "/v1/service-accounts/:id") {
        return is_admin || is_supervisor || is_billing;
    }

    false
}

fn path_matches(path: &str, pattern: &str) -> bool {
    let path_segments = path.trim_matches('/').split('/');
    let pattern_segments = pattern.trim_matches('/').split('/');
    let mut matched = true;
    let mut count = 0;
    for (actual, expected) in path_segments.zip(pattern_segments) {
        count += 1;
        if expected == ":id" {
            if actual.parse::<Uuid>().is_err() {
                matched = false;
                break;
            }
        } else if actual != expected {
            matched = false;
            break;
        }
    }
    matched
        && count == path.trim_matches('/').split('/').count()
        && count == pattern.trim_matches('/').split('/').count()
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

async fn catalog_products(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let products = match &state.central {
        Some(store) => store.catalog_products().await.map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(products)))
}

async fn admin_catalog_tariffs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let tariffs = match &state.central {
        Some(store) => store.admin_catalog_tariffs().await.map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(tariffs)))
}

async fn admin_catalog_products(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let products = match &state.central {
        Some(store) => store
            .admin_catalog_products()
            .await
            .map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(products)))
}

async fn admin_tax_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let versions = match &state.central {
        Some(store) => store
            .admin_reference_versions("TAX_PROFILE")
            .await
            .map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(versions)))
}

async fn admin_fx_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let versions = match &state.central {
        Some(store) => store
            .admin_reference_versions("FX_RATE")
            .await
            .map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(versions)))
}

async fn create_tax_profile_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateTaxProfileDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "reference administration requires PostgreSQL",
        )
    })?;
    let digest = digest_json(&input)?;
    reference_write_response(
        store
            .create_tax_profile_draft(
                &input.service_code,
                &input.jurisdiction,
                &input.base,
                input.rate,
                &input.source,
                input.effective_from,
                input.effective_to,
                principal.user_id,
                idempotency_key,
                &digest,
            )
            .await
            .map_err(reference_write_error)?,
    )
}

async fn create_fx_rate_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateFxRateDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "reference administration requires PostgreSQL",
        )
    })?;
    let digest = digest_json(&input)?;
    reference_write_response(
        store
            .create_fx_rate_draft(
                &input.from_currency,
                &input.to_currency,
                input.value,
                &input.source,
                input.effective_from,
                input.effective_to,
                principal.user_id,
                idempotency_key,
                &digest,
            )
            .await
            .map_err(reference_write_error)?,
    )
}

async fn approve_tax_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<ApproveReferenceDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    approve_reference(state, headers, id, principal, input, "TAX_PROFILE").await
}

async fn approve_fx_rate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<ApproveReferenceDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    approve_reference(state, headers, id, principal, input, "FX_RATE").await
}

async fn approve_reference(
    state: AppState,
    headers: HeaderMap,
    id: Uuid,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    input: ApproveReferenceDraftRequest,
    expected_type: &str,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let key = require_idempotency(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "reference administration requires PostgreSQL",
        )
    })?;
    let digest = digest_json(&(id, &input, expected_type))?;
    let outcome = store
        .approve_reference_draft(
            id,
            expected_type,
            &input.approval_ref,
            input.supersedes_id,
            principal.user_id,
            key,
            &digest,
        )
        .await
        .map_err(reference_write_error)?;
    reference_write_response(outcome)
}

async fn revoke_tax_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<RevokeTariffRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    revoke_reference(state, headers, id, principal, input, "TAX_PROFILE").await
}

async fn revoke_fx_rate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<RevokeTariffRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    revoke_reference(state, headers, id, principal, input, "FX_RATE").await
}

async fn revoke_reference(
    state: AppState,
    headers: HeaderMap,
    id: Uuid,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    input: RevokeTariffRequest,
    expected_type: &str,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let key = require_idempotency(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "reference administration requires PostgreSQL",
        )
    })?;
    let digest = digest_json(&(id, &input, expected_type))?;
    let outcome = store
        .revoke_reference_version(
            id,
            expected_type,
            &input.change_ref,
            principal.user_id,
            key,
            &digest,
        )
        .await
        .map_err(reference_write_error)?;
    reference_write_response(outcome)
}

fn reference_write_response(
    outcome: sel_storage::postgres::CatalogWriteOutcome,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    Ok(match outcome {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) if body["status"] == "DRAFT" => {
            (StatusCode::CREATED, Json(body))
        }
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => (StatusCode::OK, Json(body)),
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => (StatusCode::OK, Json(body)),
    })
}

async fn catalog_tariffs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let tariffs = match &state.central {
        Some(store) => store.catalog_tariffs().await.map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(tariffs)))
}

async fn catalog_tax_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let profiles = match &state.central {
        Some(store) => store.catalog_tax_profiles().await.map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(profiles)))
}

async fn catalog_fx_rates(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let rates = match &state.central {
        Some(store) => store.catalog_fx_rates().await.map_err(storage_error)?,
        None => Vec::new(),
    };
    Ok(Json(serde_json::Value::Array(rates)))
}

async fn catalog_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CatalogSnapshot>, ApiError> {
    require_auth(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "offline catalog snapshot requires the central PostgreSQL store",
        ));
    };
    let mut products = store
        .admin_catalog_products()
        .await
        .map_err(storage_error)?;
    let mut tariffs = store.admin_catalog_tariffs().await.map_err(storage_error)?;
    let mut tax_profiles = store
        .admin_reference_versions("TAX_PROFILE")
        .await
        .map_err(storage_error)?;
    let mut fx_rates = store
        .admin_reference_versions("FX_RATE")
        .await
        .map_err(storage_error)?;
    for rows in [
        &mut products,
        &mut tariffs,
        &mut tax_profiles,
        &mut fx_rates,
    ] {
        rows.retain(|row| matches!(row["status"].as_str(), Some("APPROVED" | "REVOKED")));
    }
    let payload = CatalogSnapshotPayload {
        schema_version: 1,
        products,
        tariffs,
        tax_profiles,
        fx_rates,
    };
    let sha256 = catalog_snapshot_sha256(&payload);
    Ok(Json(CatalogSnapshot { payload, sha256 }))
}

async fn create_tariff_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateTariffDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "tariff administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .create_tariff_draft(
            &input.service_code,
            &input.customer_category,
            &input.unit,
            input.unit_price,
            &input.currency,
            input.effective_from,
            input.effective_to,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(tariff_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn approve_tariff_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tariff_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<ApproveTariffDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "tariff administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&(tariff_id, &input))?;
    match store
        .approve_tariff_draft(
            tariff_id,
            &input.approval_ref,
            input.supersedes_tariff_id,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(tariff_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn revoke_tariff(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tariff_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<RevokeTariffRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "tariff administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&(tariff_id, &input))?;
    match store
        .revoke_tariff(
            tariff_id,
            &input.change_ref,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(tariff_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn create_solar_product_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateSolarProductDraftRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "product administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .create_solar_product_draft(
            &input.sku,
            &input.label,
            input.price_reference,
            &input.currency,
            input.effective_from,
            input.effective_to,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(product_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn approve_solar_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<ApproveSolarProductRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "product administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&(product_id, &input))?;
    match store
        .approve_solar_product(
            product_id,
            &input.approval_ref,
            input.supersedes_product_id,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(product_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
}

async fn revoke_solar_product(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(product_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<RevokeTariffRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_principal(principal)?;
    let idempotency_key = require_idempotency(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CATALOG_CENTRAL_REQUIRED",
            "product administration requires the central PostgreSQL store",
        ));
    };
    let digest = digest_json(&(product_id, &input))?;
    match store
        .revoke_solar_product(
            product_id,
            &input.change_ref,
            principal.user_id,
            idempotency_key,
            &digest,
        )
        .await
        .map_err(product_write_error)?
    {
        sel_storage::postgres::CatalogWriteOutcome::Created(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
        sel_storage::postgres::CatalogWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
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
    if input.password.chars().count() < sel_storage::postgres::MIN_PASSWORD_LENGTH {
        return Err(ApiError::unprocessable(
            "PASSWORD_TOO_SHORT",
            "password must contain at least 8 characters",
        ));
    }
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "AUTH_UNAVAILABLE",
            "password provisioning requires the central PostgreSQL store",
        ));
    };
    store
        .set_user_password(user_id, principal.user_id, &input.password)
        .await
        .map_err(auth_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_auth(&headers)?;
    require_admin(principal)?;
    let store = central_store(&state)?;
    store
        .admin_list_users()
        .await
        .map(Json)
        .map_err(storage_error)
}

async fn admin_create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateAdminUserRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let principal = require_admin(principal)?;
    if input.identity_ref.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "IDENTITY_REF_REQUIRED",
            "identity_ref must not be empty",
        ));
    }
    if !matches!(
        input.role.as_str(),
        "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
    ) {
        return Err(ApiError::unprocessable(
            "USER_ROLE_INVALID",
            "role is not a supported SEL role",
        ));
    }
    if input.password.chars().count() < sel_storage::postgres::MIN_PASSWORD_LENGTH {
        return Err(ApiError::unprocessable(
            "PASSWORD_TOO_SHORT",
            "password must contain at least 8 characters",
        ));
    }
    central_store(&state)?
        .admin_create_user(
            principal.user_id,
            &input.identity_ref,
            &input.role,
            &input.password,
        )
        .await
        .map(|body| (StatusCode::CREATED, Json(body)))
        .map_err(admin_directory_error)
}

async fn admin_update_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<UpdateAdminUserRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let principal = require_admin(principal)?;
    if input.role.as_deref().is_some_and(|role| {
        !matches!(
            role,
            "ADMIN" | "BILLING_AGENT" | "CASHIER" | "SUPERVISOR" | "AUDITOR_READONLY"
        )
    }) {
        return Err(ApiError::unprocessable(
            "USER_ROLE_INVALID",
            "role is not a supported SEL role",
        ));
    }
    if input
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "ACTIVE" | "REVOKED"))
    {
        return Err(ApiError::unprocessable(
            "USER_STATUS_INVALID",
            "status must be ACTIVE or REVOKED",
        ));
    }
    if input.role.is_none() && input.status.is_none() {
        return Err(ApiError::unprocessable(
            "USER_UPDATE_EMPTY",
            "at least one of role or status is required",
        ));
    }
    central_store(&state)?
        .admin_update_user(
            principal.user_id,
            user_id,
            input.role.as_deref(),
            input.status.as_deref(),
        )
        .await
        .map(Json)
        .map_err(admin_directory_error)
}

async fn admin_list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_auth(&headers)?;
    require_admin(principal)?;
    central_store(&state)?
        .admin_list_devices()
        .await
        .map(Json)
        .map_err(storage_error)
}

async fn admin_list_offline_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_auth(&headers)?;
    require_admin(principal)?;
    central_store(&state)?
        .admin_list_offline_grants()
        .await
        .map(Json)
        .map_err(storage_error)
}

async fn admin_revoke_offline_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(grant_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<StatusCode, ApiError> {
    require_auth(&headers)?;
    let principal = require_admin(principal)?;
    central_store(&state)?
        .admin_revoke_offline_grant(principal.user_id, grant_id)
        .await
        .map_err(admin_directory_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn require_admin(
    principal: Option<Extension<AuthenticatedPrincipal>>,
) -> Result<sel_storage::postgres::AuthSessionPrincipal, ApiError> {
    let Some(Extension(AuthenticatedPrincipal(principal))) = principal else {
        return Err(ApiError::unauthorized(
            "SESSION_REQUIRED",
            "individual session authentication is required",
        ));
    };
    if principal.role != "ADMIN" {
        return Err(ApiError::forbidden(
            "PERMISSION_DENIED",
            "administrator role is required",
        ));
    }
    Ok(principal)
}

fn central_store(state: &AppState) -> Result<&sel_storage::CentralStore, ApiError> {
    state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CENTRAL_STORE_REQUIRED",
            "administration requires the central PostgreSQL store",
        )
    })
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

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct CustomerListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    status: Option<String>,
    q: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct InvoiceListQuery {
    customer_id: Option<Uuid>,
    period: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct PaymentListQuery {
    customer_id: Option<Uuid>,
    status: Option<String>,
    limit: Option<usize>,
}

async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<InvoiceListQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_auth(&headers)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let period = query
        .period
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if period.is_some_and(|value| {
        chrono::NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").is_err()
    }) {
        return Err(ApiError::unprocessable(
            "BILLING_PERIOD_INVALID",
            "period must use YYYY-MM format",
        ));
    }
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "LOCAL_PENDING" | "CENTRAL_ACCEPTED" | "VOID" | "CONFLICT_REQUIRES_REVIEW"
        )
    }) {
        return Err(ApiError::unprocessable(
            "INVOICE_STATUS_INVALID",
            "status must be LOCAL_PENDING, CENTRAL_ACCEPTED, VOID or CONFLICT_REQUIRES_REVIEW",
        ));
    }
    if let Some(store) = &state.central {
        return store
            .list_invoice_responses(
                query.customer_id,
                period,
                query.status.as_deref(),
                limit as i64,
            )
            .await
            .map(Json)
            .map_err(storage_error);
    }
    let mut invoices: Vec<_> = state
        .invoices
        .read()
        .await
        .iter()
        .filter(|(_, invoice)| {
            query
                .customer_id
                .is_none_or(|customer_id| invoice.customer_id == customer_id)
                && period.is_none_or(|period| invoice.period == period)
                && query
                    .status
                    .as_deref()
                    .is_none_or(|status| invoice.response.status == status)
        })
        .map(|(id, invoice)| {
            let mut value = serde_json::to_value(&invoice.response).map_err(ApiError::internal)?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "period".into(),
                    serde_json::Value::String(invoice.period.clone()),
                );
                object.insert(
                    "invoice_id".into(),
                    serde_json::Value::String(id.to_string()),
                );
            }
            Ok((*id, value))
        })
        .collect::<Result<_, ApiError>>()?;
    invoices.sort_by_key(|(id, _)| *id);
    invoices.truncate(limit);
    Ok(Json(
        invoices.into_iter().map(|(_, invoice)| invoice).collect(),
    ))
}

async fn list_customers(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CustomerListQuery>,
) -> Result<Response, ApiError> {
    require_auth(&headers)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 100);
    if let Some(status) = query.status.as_deref() {
        if !matches!(status, "ACTIVE" | "ARCHIVED") {
            return Err(ApiError::unprocessable(
                "CUSTOMER_STATUS_INVALID",
                "status filter must be ACTIVE or ARCHIVED",
            ));
        }
    }
    let search = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if search.is_some_and(|value| value.chars().count() > 128) {
        return Err(ApiError::unprocessable(
            "CUSTOMER_SEARCH_TOO_LONG",
            "customer search is limited to 128 characters",
        ));
    }
    let (customers, next_cursor) = if let Some(store) = &state.central {
        store
            .list_customers_page(
                query.cursor.as_deref(),
                query.status.as_deref(),
                search,
                limit as i64,
            )
            .await
            .map_err(storage_error)?
    } else {
        let mut customers: Vec<_> = state
            .customers
            .read()
            .await
            .values()
            .filter(|item| {
                query
                    .status
                    .as_deref()
                    .is_none_or(|status| item.status == status)
            })
            .filter(|item| {
                query
                    .cursor
                    .as_deref()
                    .is_none_or(|cursor| item.customer_code.as_str() > cursor)
            })
            .filter(|item| {
                search.is_none_or(|term| {
                    let term = term.to_lowercase();
                    item.customer_code.to_lowercase().contains(&term)
                        || item.legal_name.to_lowercase().contains(&term)
                })
            })
            .cloned()
            .collect();
        customers.sort_by(|a, b| a.customer_code.cmp(&b.customer_code));
        let next_cursor = if customers.len() > limit {
            customers
                .get(limit - 1)
                .map(|customer| customer.customer_code.clone())
        } else {
            None
        };
        customers.truncate(limit);
        (customers, next_cursor)
    };
    let mut response = Json(customers).into_response();
    if let Some(next_cursor) = next_cursor {
        response.headers_mut().insert(
            "x-next-cursor",
            HeaderValue::from_str(&next_cursor).map_err(ApiError::internal)?,
        );
    }
    Ok(response)
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
    if input
        .legal_name
        .as_deref()
        .is_some_and(|legal_name| legal_name.trim().is_empty())
    {
        return Err(ApiError::unprocessable(
            "CUSTOMER_REQUIRED",
            "legal name cannot be empty",
        ));
    }
    if input
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "ACTIVE" | "ARCHIVED"))
    {
        return Err(ApiError::unprocessable(
            "CUSTOMER_STATUS_INVALID",
            "status must be ACTIVE or ARCHIVED",
        ));
    }
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
    customer.address = input.address.apply_to(customer.address.clone());
    customer.phone = input.phone.apply_to(customer.phone.clone());
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

async fn create_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateServiceAccount>,
) -> Result<(StatusCode, Json<sel_contracts::ServiceAccount>), ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .create_service_account(
            customer_id,
            &input,
            principal.map(|Extension(AuthenticatedPrincipal(principal))| principal.user_id),
        )
        .await
        .map(|account| (StatusCode::CREATED, Json(account)))
        .map_err(meter_write_error)
}

async fn list_service_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<Vec<sel_contracts::ServiceAccount>>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .list_service_accounts(customer_id)
        .await
        .map(Json)
        .map_err(meter_write_error)
}

async fn get_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<sel_contracts::ServiceAccount>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .get_service_account(id)
        .await
        .map_err(meter_write_error)?
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found("SERVICE_ACCOUNT_NOT_FOUND", "service account not found")
        })
}

async fn update_service_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<UpdateServiceAccount>,
) -> Result<Response, ApiError> {
    require_auth(&headers)?;
    let expected = headers
        .get("if-match")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim_matches('"').parse::<i64>().ok())
        .ok_or_else(|| {
            ApiError::unprocessable(
                "IF_MATCH_REQUIRED",
                "If-Match must contain the service account revision",
            )
        })?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    let account = store
        .update_service_account(
            id,
            expected,
            &input,
            principal.map(|Extension(AuthenticatedPrincipal(principal))| principal.user_id),
        )
        .await
        .map_err(meter_write_error)?
        .ok_or_else(|| {
            ApiError::not_found("SERVICE_ACCOUNT_NOT_FOUND", "service account not found")
        })?;
    let mut response = Json(account.clone()).into_response();
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&format!("\"{}\"", account.revision)).map_err(ApiError::internal)?,
    );
    Ok(response)
}

async fn create_meter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(service_account_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateMeter>,
) -> Result<(StatusCode, Json<Meter>), ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .create_meter(
            service_account_id,
            &input,
            principal.map(|Extension(AuthenticatedPrincipal(principal))| principal.user_id),
        )
        .await
        .map(|meter| (StatusCode::CREATED, Json(meter)))
        .map_err(meter_write_error)
}

async fn list_meters(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(service_account_id): Path<Uuid>,
) -> Result<Json<Vec<Meter>>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .list_meters(service_account_id)
        .await
        .map(Json)
        .map_err(meter_write_error)
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
struct MeterReadingQuery {
    period: Option<String>,
}

async fn list_meter_readings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meter_id): Path<Uuid>,
    Query(query): Query<MeterReadingQuery>,
) -> Result<Json<Vec<MeterReading>>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .list_meter_readings(meter_id, query.period.as_deref())
        .await
        .map(Json)
        .map_err(meter_write_error)
}

async fn create_meter_reading(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(meter_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreateMeterReading>,
) -> Result<(StatusCode, Json<MeterReading>), ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .create_meter_reading(
            meter_id,
            &input,
            principal.map(|Extension(AuthenticatedPrincipal(principal))| principal.user_id),
        )
        .await
        .map(|reading| (StatusCode::CREATED, Json(reading)))
        .map_err(meter_write_error)
}

async fn correct_meter_reading(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(reading_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CorrectMeterReading>,
) -> Result<(StatusCode, Json<MeterReading>), ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unavailable(
            "PERSISTENT_STORAGE_REQUIRED",
            "central PostgreSQL storage is required",
        )
    })?;
    store
        .correct_meter_reading(
            reading_id,
            &input,
            principal.map(|Extension(AuthenticatedPrincipal(principal))| principal.user_id),
        )
        .await
        .map(|reading| (StatusCode::CREATED, Json(reading)))
        .map_err(meter_write_error)
}

async fn preview_utility(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<UtilityBillingRequest>,
) -> Result<Json<UtilityBillingResponse>, ApiError> {
    require_auth(&headers)?;
    let input = resolve_utility_request(&state, input).await?;
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
    let resolved_input = resolve_utility_request(&state, input).await?;
    let response = calculate_utility(&state, &resolved_input).await?;
    let mut committed = response.clone();
    committed.status = "CENTRAL_ACCEPTED".into();
    let receivable_id = Uuid::new_v4();
    committed.receivable_id = Some(receivable_id);
    if let Some(store) = &state.central {
        return match store
            .issue_utility_invoice(&resolved_input, &committed, key, &digest)
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
        invoice.customer_id == resolved_input.customer_id
            && invoice.period == resolved_input.billing_period
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
            customer_id: resolved_input.customer_id,
            period: resolved_input.billing_period.clone(),
            response: committed.clone(),
        },
    );
    drop(invoices);
    state.receivables.write().await.insert(
        receivable_id,
        ReceivableRecord {
            id: receivable_id,
            customer_id: resolved_input.customer_id,
            currency: resolved_input.currency.clone(),
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

async fn resolve_utility_request(
    state: &AppState,
    mut input: UtilityBillingRequest,
) -> Result<UtilityBillingRequest, ApiError> {
    if let Some(refs) = &input.meter_readings {
        let store = state.central.as_ref().ok_or_else(|| {
            ApiError::unprocessable(
                "METER_READINGS_REQUIRE_CENTRAL_STORE",
                "meter-based billing requires the configured central store",
            )
        })?;
        let (electricity, water) = store
            .utility_meter_reading_quantities(input.customer_id, &input.billing_period, refs)
            .await
            .map_err(|error| ApiError::conflict("METER_READING_NOT_BILLABLE", error.to_string()))?;
        input.electricity.quantity = electricity;
        input.water.quantity = water;
    }
    Ok(input)
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
    if input.billing_period.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "BILLING_PERIOD_INVALID",
            "billing period is required",
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
        billing_period: input.billing_period.clone(),
        due_date: input.due_date,
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

async fn get_installment_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let Some(store) = &state.central else {
        return Err(ApiError::not_found(
            "INSTALLMENT_PLAN_NOT_FOUND",
            "installment plan not found",
        ));
    };
    store
        .get_installment_plan(id)
        .await
        .map_err(storage_error)?
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found("INSTALLMENT_PLAN_NOT_FOUND", "installment plan not found")
        })
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

async fn list_payments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PaymentListQuery>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    require_auth(&headers)?;
    if query.status.as_deref().is_some_and(|status| {
        !matches!(
            status,
            "CENTRAL_ACCEPTED" | "CONFLICT_REQUIRES_REVIEW" | "REJECTED"
        )
    }) {
        return Err(ApiError::unprocessable(
            "PAYMENT_STATUS_INVALID",
            "status must be CENTRAL_ACCEPTED, CONFLICT_REQUIRES_REVIEW or REJECTED",
        ));
    }
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "PAYMENTS_CENTRAL_REQUIRED",
            "the payment journal requires the central PostgreSQL store",
        )
    })?;
    store
        .list_payment_responses(
            query.customer_id,
            query.status.as_deref(),
            query.limit.unwrap_or(50).clamp(1, 100) as i64,
        )
        .await
        .map(Json)
        .map_err(storage_error)
}

async fn get_payment_document_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(payment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "PAYMENT_DOCUMENT_CENTRAL_REQUIRED",
            "payment receipt data requires the central PostgreSQL store",
        )
    })?;
    store
        .payment_document_data(payment_id)
        .await
        .map_err(storage_error)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("PAYMENT_NOT_FOUND", "payment not found"))
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

async fn allocate_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(payment_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<AllocatePaymentRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    if input.allocations.is_empty()
        || input
            .allocations
            .iter()
            .any(|allocation| allocation.amount <= Decimal::ZERO)
    {
        return Err(ApiError::unprocessable(
            "PAYMENT_ALLOCATION_INVALID",
            "at least one positive allocation is required",
        ));
    }
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
                "an authenticated actor is required for payment allocation",
            )
        })?
    };
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "PAYMENT_ALLOCATION_CENTRAL_REQUIRED",
            "audited payment allocations require a central PostgreSQL store",
        ));
    };
    let digest = digest_json(&input)?;
    match store
        .allocate_payment(payment_id, &input.allocations, actor_id, key, &digest)
        .await
        .map_err(payment_allocation_error)?
    {
        sel_storage::postgres::PaymentAllocationWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::PaymentAllocationWriteOutcome::Replay(body) => {
            Ok((StatusCode::OK, Json(body)))
        }
    }
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

async fn create_credit_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(receivable_id): Path<Uuid>,
    principal: Option<Extension<AuthenticatedPrincipal>>,
    Json(input): Json<CreditNoteRequest>,
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
                "X-Actor-Id is required for an audited credit note",
            )
        })?
    };
    if input.amount <= Decimal::ZERO || input.reason.trim().is_empty() {
        return Err(ApiError::unprocessable(
            "CREDIT_NOTE_INVALID",
            "a positive amount and a non-empty reason are required",
        ));
    }
    let digest = digest_json(&input)?;
    let Some(store) = &state.central else {
        return Err(ApiError::unprocessable(
            "CREDIT_NOTE_CENTRAL_REQUIRED",
            "audited credit notes require a central PostgreSQL store",
        ));
    };
    match store
        .create_credit_note(receivable_id, &input, actor_id, key, &digest)
        .await
        .map_err(credit_note_error)?
    {
        sel_storage::postgres::CreditNoteWriteOutcome::Created(body) => {
            Ok((StatusCode::CREATED, Json(body)))
        }
        sel_storage::postgres::CreditNoteWriteOutcome::Replay(body) => {
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

async fn create_solar_quote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateSolarQuoteRequest>,
) -> Result<Json<SolarQuoteResponse>, ApiError> {
    require_auth(&headers)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CENTRAL_STORE_REQUIRED",
            "solar quotes require the central PostgreSQL store",
        )
    })?;
    store
        .create_solar_quote(&input)
        .await
        .map_err(storage_error)?
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found(
                "SOLAR_PRODUCT_NOT_FOUND",
                "active solar product was not found",
            )
        })
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

async fn create_solar_contract(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<sel_contracts::ActivateSolarContractRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_auth(&headers)?;
    let key = require_idempotency(&headers)?;
    let digest = digest_json(&input)?;
    let store = state.central.as_ref().ok_or_else(|| {
        ApiError::unprocessable(
            "CENTRAL_STORE_REQUIRED",
            "solar contract creation requires the central PostgreSQL store",
        )
    })?;
    let contract_id = Uuid::new_v4();
    match store
        .activate_solar_contract(contract_id, &input, key, &digest)
        .await
        .map_err(solar_contract_write_error)?
    {
        sel_storage::postgres::SolarContractWriteOutcome::Created(body)
        | sel_storage::postgres::SolarContractWriteOutcome::Replay(body) => Ok(Json(body)),
    }
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
        sel_storage::postgres::AuthError::IdentityRefRequired => {
            ApiError::unprocessable("IDENTITY_REF_REQUIRED", "identity_ref must not be empty")
        }
        sel_storage::postgres::AuthError::InitialAdminAlreadyProvisioned => ApiError::conflict(
            "INITIAL_ADMIN_ALREADY_PROVISIONED",
            "the organization already has a user account",
        ),
        sel_storage::postgres::AuthError::OrganizationNotFound => {
            ApiError::internal("organization must be initialized before administrator bootstrap")
        }
        sel_storage::postgres::AuthError::ActorNotAuthorized => ApiError::forbidden(
            "PERMISSION_DENIED",
            "only an active administrator can provision user passwords",
        ),
    }
}

fn admin_directory_error(error: sel_storage::postgres::AdminDirectoryError) -> ApiError {
    use sel_storage::postgres::AdminDirectoryError as E;
    match error {
        E::Database(error) => storage_error(error),
        E::ActorNotAuthorized => {
            ApiError::forbidden("PERMISSION_DENIED", "an active administrator is required")
        }
        E::UserNotFound => {
            ApiError::not_found("USER_NOT_FOUND", "user not found in this organization")
        }
        E::ResourceNotFound => ApiError::not_found(
            "ADMIN_RESOURCE_NOT_FOUND",
            "device or grant not found in this organization",
        ),
        E::LastAdministrator => ApiError::conflict(
            "LAST_ADMINISTRATOR",
            "the last active administrator cannot be disabled or demoted",
        ),
        E::SelfModification => ApiError::conflict(
            "SELF_MODIFICATION_DENIED",
            "an administrator cannot change their own role or status",
        ),
        E::IdentityConflict => ApiError::conflict(
            "USER_IDENTITY_EXISTS",
            "identity_ref is empty or already exists in this organization",
        ),
        E::InvalidFields => ApiError::unprocessable(
            "USER_FIELDS_INVALID",
            "identity_ref, role, status, or password is invalid",
        ),
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

fn tariff_write_error(error: sel_storage::postgres::TariffWriteError) -> ApiError {
    use sel_storage::postgres::TariffWriteError;
    match error {
        TariffWriteError::InvalidFields => ApiError::unprocessable(
            "TARIFF_FIELDS_INVALID",
            "service, category, unit, price, currency, or effective period is invalid",
        ),
        TariffWriteError::ApprovalRefRequired => ApiError::unprocessable(
            "TARIFF_APPROVAL_REF_REQUIRED",
            "an approval reference is required",
        ),
        TariffWriteError::NotFound => {
            ApiError::not_found("TARIFF_DRAFT_NOT_FOUND", "tariff draft was not found")
        }
        TariffWriteError::NotDraft => {
            ApiError::conflict("TARIFF_NOT_DRAFT", "only a draft tariff can be approved")
        }
        TariffWriteError::PeriodOverlap => ApiError::conflict(
            "TARIFF_PERIOD_OVERLAP",
            "an approved tariff already covers part of this service category period",
        ),
        TariffWriteError::InvalidSupersededTariff => ApiError::unprocessable(
            "TARIFF_SUPERSESSION_INVALID",
            "the replaced tariff must be approved, compatible, and cover the new tariff start date",
        ),
        TariffWriteError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        TariffWriteError::Database(error) => storage_error(error),
    }
}

fn product_write_error(error: sel_storage::postgres::ProductWriteError) -> ApiError {
    use sel_storage::postgres::ProductWriteError;
    match error {
        ProductWriteError::InvalidFields => ApiError::unprocessable(
            "SOLAR_PRODUCT_FIELDS_INVALID",
            "SKU, label, price, currency, or effective period is invalid",
        ),
        ProductWriteError::ApprovalRefRequired => ApiError::unprocessable(
            "SOLAR_PRODUCT_APPROVAL_REF_REQUIRED",
            "an explicit SEL approval reference is required",
        ),
        ProductWriteError::NotFound => {
            ApiError::not_found("SOLAR_PRODUCT_NOT_FOUND", "solar product version was not found")
        }
        ProductWriteError::NotDraft => ApiError::conflict(
            "SOLAR_PRODUCT_NOT_DRAFT",
            "only a draft solar product can be approved",
        ),
        ProductWriteError::PeriodOverlap => ApiError::conflict(
            "SOLAR_PRODUCT_PERIOD_OVERLAP",
            "an approved product version already covers part of this SKU period",
        ),
        ProductWriteError::InvalidSupersededProduct => ApiError::unprocessable(
            "SOLAR_PRODUCT_SUPERSESSION_INVALID",
            "the replaced version must be approved, match the SKU/currency, and cover the new start date",
        ),
        ProductWriteError::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        ProductWriteError::Database(error) => storage_error(error),
    }
}

fn reference_write_error(error: sel_storage::postgres::ReferenceCatalogWriteError) -> ApiError {
    use sel_storage::postgres::ReferenceCatalogWriteError as Error;
    match error {
        Error::InvalidFields => ApiError::unprocessable(
            "REFERENCE_FIELDS_INVALID",
            "reference values, dates, idempotency key, or route type are invalid",
        ),
        Error::ApprovalRefRequired => ApiError::unprocessable(
            "REFERENCE_APPROVAL_REF_REQUIRED",
            "an explicit SEL approval reference is required",
        ),
        Error::NotFound => ApiError::not_found(
            "REFERENCE_VERSION_NOT_FOUND",
            "reference version was not found",
        ),
        Error::NotDraft => ApiError::conflict(
            "REFERENCE_NOT_DRAFT",
            "only a draft reference can be approved",
        ),
        Error::PeriodOverlap => ApiError::conflict(
            "REFERENCE_PERIOD_OVERLAP",
            "an approved reference already covers part of this period",
        ),
        Error::InvalidSupersededReference => ApiError::unprocessable(
            "REFERENCE_SUPERSESSION_INVALID",
            "the replaced version must be approved, compatible, and cover the new start date",
        ),
        Error::IdempotencyPayloadMismatch => ApiError::conflict(
            "IDEMPOTENCY_PAYLOAD_MISMATCH",
            "idempotency key was reused with another payload",
        ),
        Error::Database(error) => storage_error(error),
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

fn meter_write_error(error: sel_storage::postgres::MeterWriteError) -> ApiError {
    use sel_storage::postgres::MeterWriteError;
    match error {
        MeterWriteError::CustomerNotFound => {
            ApiError::not_found("CUSTOMER_NOT_FOUND", "customer not found")
        }
        MeterWriteError::ServiceAccountNotFound => {
            ApiError::not_found("SERVICE_ACCOUNT_NOT_FOUND", "service account not found")
        }
        MeterWriteError::ServiceAccountRevisionConflict => ApiError::conflict(
            "SERVICE_ACCOUNT_REVISION_CONFLICT",
            "service account was changed by another actor",
        ),
        MeterWriteError::MeterNotFound => ApiError::not_found("METER_NOT_FOUND", "meter not found"),
        MeterWriteError::DuplicateReading => ApiError::conflict(
            "METER_READING_EXISTS",
            "a reading already exists for this period",
        ),
        MeterWriteError::PreviousIndexMismatch => ApiError::conflict(
            "METER_PREVIOUS_INDEX_MISMATCH",
            "previous index does not match the latest accepted reading",
        ),
        MeterWriteError::ReadingNotLatest => ApiError::conflict(
            "METER_READING_NOT_LATEST",
            "only the latest reading can be corrected",
        ),
        MeterWriteError::ReadingAlreadyInvoiced => ApiError::conflict(
            "METER_READING_ALREADY_INVOICED",
            "an invoiced meter reading cannot be corrected; issue a financial correction",
        ),
        MeterWriteError::InvalidConfiguration => ApiError::unprocessable(
            "METER_CONFIGURATION_INVALID",
            "service account, connection status, or meter unit is invalid",
        ),
        MeterWriteError::InvalidInput => {
            ApiError::unprocessable("METER_INPUT_INVALID", "meter reading input is invalid")
        }
        MeterWriteError::InvalidIndex => ApiError::unprocessable(
            "METER_INDEX_INVALID",
            "current meter index must be greater than or equal to previous index",
        ),
        MeterWriteError::CorrectionReasonRequired => ApiError::unprocessable(
            "METER_CORRECTION_REASON_REQUIRED",
            "a reason is required to correct a reading",
        ),
        MeterWriteError::Database(error) => storage_error(error),
    }
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
        sel_storage::postgres::UtilityIssueError::MeterReadingInvalid => ApiError::conflict(
            "METER_READING_NOT_BILLABLE",
            "selected meter readings are stale, already invoiced, or do not match the invoice",
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

fn payment_allocation_error(error: sel_storage::postgres::PaymentAllocationWriteError) -> ApiError {
    match error {
        sel_storage::postgres::PaymentAllocationWriteError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::PaymentAllocationWriteError::PaymentNotFound => {
            ApiError::not_found("PAYMENT_NOT_FOUND", "payment not found")
        }
        sel_storage::postgres::PaymentAllocationWriteError::PaymentNotAccepted => {
            ApiError::conflict(
                "PAYMENT_NOT_ACCEPTED",
                "only a centrally accepted payment can be allocated",
            )
        }
        sel_storage::postgres::PaymentAllocationWriteError::InvalidAllocation => {
            ApiError::unprocessable(
                "PAYMENT_ALLOCATION_INVALID",
                "allocation amounts must be strictly positive",
            )
        }
        sel_storage::postgres::PaymentAllocationWriteError::AllocationExceedsPayment => {
            ApiError::unprocessable(
                "PAYMENT_ALLOCATION_EXCEEDS_RECEIPT",
                "allocations exceed the payment amount",
            )
        }
        sel_storage::postgres::PaymentAllocationWriteError::Overallocated => ApiError::conflict(
            "RECEIVABLE_OVERALLOCATED",
            "payment would exceed the receivable balance",
        ),
        sel_storage::postgres::PaymentAllocationWriteError::ContextInvalid => {
            ApiError::unprocessable(
                "PAYMENT_CONTEXT_INVALID",
                "payment customer or currency does not match the receivable",
            )
        }
        sel_storage::postgres::PaymentAllocationWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::PaymentAllocationWriteError::StoredResponse(error) => {
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

fn credit_note_error(error: sel_storage::postgres::CreditNoteWriteError) -> ApiError {
    match error {
        sel_storage::postgres::CreditNoteWriteError::IdempotencyPayloadMismatch => {
            ApiError::conflict(
                "IDEMPOTENCY_PAYLOAD_MISMATCH",
                "idempotency key was reused with another payload",
            )
        }
        sel_storage::postgres::CreditNoteWriteError::ReceivableNotFound => {
            ApiError::not_found("RECEIVABLE_NOT_FOUND", "receivable not found")
        }
        sel_storage::postgres::CreditNoteWriteError::ReceivableNotCreditable => ApiError::conflict(
            "RECEIVABLE_NOT_CREDITABLE",
            "the receivable cannot receive a credit in its current status",
        ),
        sel_storage::postgres::CreditNoteWriteError::ActorNotActive => {
            ApiError::unauthorized("ACTOR_NOT_ACTIVE", "the credit note actor is not active")
        }
        sel_storage::postgres::CreditNoteWriteError::ActorNotAuthorized => ApiError::forbidden(
            "ACTOR_NOT_AUTHORIZED",
            "the actor role cannot record a credit note",
        ),
        sel_storage::postgres::CreditNoteWriteError::InvalidAmount => ApiError::unprocessable(
            "CREDIT_NOTE_INVALID",
            "credit amount must be strictly positive",
        ),
        sel_storage::postgres::CreditNoteWriteError::AmountExceedsBalance => ApiError::conflict(
            "CREDIT_NOTE_EXCEEDS_BALANCE",
            "credit amount exceeds the outstanding receivable balance",
        ),
        sel_storage::postgres::CreditNoteWriteError::ReasonRequired => ApiError::unprocessable(
            "CREDIT_NOTE_REASON_REQUIRED",
            "a reason is required for an audited credit note",
        ),
        sel_storage::postgres::CreditNoteWriteError::Database(error) => storage_error(error),
        sel_storage::postgres::CreditNoteWriteError::StoredResponse(error) => {
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
        sel_storage::postgres::SolarSaleWriteError::PriceMismatch => ApiError::unprocessable(
            "SOLAR_PRICE_MISMATCH",
            "the sale principal must match the approved product price",
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
    fn unavailable(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code,
            detail: detail.into(),
        }
    }
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
    use uuid::Uuid;

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
        assert!(role_allows_request(&Method::GET, "/v1/payments", "CASHIER"));
        assert!(role_allows_request(
            &Method::GET,
            "/v1/payments/00000000-0000-0000-0000-000000000001/document-data",
            "AUDITOR_READONLY"
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

    #[test]
    fn production_role_matrix_denies_unclassified_routes_and_methods() {
        assert!(!role_allows_request(
            &Method::GET,
            "/v1/unregistered-resource",
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::DELETE,
            "/v1/customers/123",
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/unregistered-resource",
            "SUPERVISOR"
        ));
    }

    #[test]
    fn cashier_can_submit_grant_checked_offline_sync_events() {
        assert!(role_allows_request(
            &Method::POST,
            "/v1/sync/push",
            "CASHIER"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/sync/push",
            "AUDITOR_READONLY"
        ));
    }

    #[test]
    fn only_admin_can_provision_user_passwords() {
        let user_id = "3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0";
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/users/{user_id}/password"),
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            &format!("/v1/users/{user_id}/password"),
            "SUPERVISOR"
        ));
    }

    #[test]
    fn only_admin_can_create_and_approve_tariff_versions() {
        let tariff_id = Uuid::new_v4();
        assert!(role_allows_request(
            &Method::POST,
            "/v1/admin/catalog/tariffs",
            "ADMIN"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/tariffs/{tariff_id}/approve"),
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/admin/catalog/tariffs",
            "SUPERVISOR"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/tariffs/{tariff_id}/approve"),
            "BILLING_AGENT"
        ));
    }

    #[test]
    fn role_matrix_matches_only_declared_resource_routes() {
        let customer_id = "3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0";
        let service_account_id = Uuid::new_v4();
        let meter_id = Uuid::new_v4();
        let reading_id = Uuid::new_v4();
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/customers/{customer_id}/service-accounts"),
            "BILLING_AGENT"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/service-accounts/{service_account_id}/meters"),
            "BILLING_AGENT"
        ));
        assert!(role_allows_request(
            &Method::GET,
            &format!("/v1/service-accounts/{service_account_id}"),
            "AUDITOR_READONLY"
        ));
        assert!(role_allows_request(
            &Method::PATCH,
            &format!("/v1/service-accounts/{service_account_id}"),
            "BILLING_AGENT"
        ));
        assert!(!role_allows_request(
            &Method::PATCH,
            &format!("/v1/service-accounts/{service_account_id}"),
            "AUDITOR_READONLY"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/meters/{meter_id}/readings"),
            "BILLING_AGENT"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            &format!("/v1/meter-readings/{reading_id}/corrections"),
            "BILLING_AGENT"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/meter-readings/{reading_id}/corrections"),
            "SUPERVISOR"
        ));
        assert!(role_allows_request(
            &Method::PATCH,
            &format!("/v1/customers/{customer_id}"),
            "BILLING_AGENT"
        ));
        assert!(role_allows_request(
            &Method::GET,
            &format!("/v1/invoices/{customer_id}/document-data"),
            "AUDITOR_READONLY"
        ));
        assert!(!role_allows_request(
            &Method::GET,
            "/v1/invoices/not-a-uuid",
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            &format!("/v1/receivables/{customer_id}/credit"),
            "CASHIER"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/receivables/{customer_id}/credit"),
            "SUPERVISOR"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/auth/login",
            "ADMIN"
        ));
    }

    #[test]
    fn user_device_and_grant_administration_is_admin_only() {
        for path in [
            "/v1/admin/users",
            "/v1/admin/devices",
            "/v1/admin/offline-grants",
        ] {
            assert!(role_allows_request(&Method::GET, path, "ADMIN"), "{path}");
            for role in ["SUPERVISOR", "BILLING_AGENT", "CASHIER", "AUDITOR_READONLY"] {
                assert!(
                    !role_allows_request(&Method::GET, path, role),
                    "{role} {path}"
                );
            }
        }
        let user_path = "/v1/admin/users/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0";
        let grant_path = "/v1/admin/offline-grants/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/revoke";
        assert!(role_allows_request(
            &Method::POST,
            "/v1/admin/users",
            "ADMIN"
        ));
        assert!(role_allows_request(&Method::PATCH, user_path, "ADMIN"));
        assert!(role_allows_request(&Method::POST, grant_path, "ADMIN"));
        assert!(!role_allows_request(
            &Method::POST,
            "/v1/admin/users",
            "SUPERVISOR"
        ));
        assert!(!role_allows_request(
            &Method::PATCH,
            user_path,
            "SUPERVISOR"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            grant_path,
            "SUPERVISOR"
        ));
    }

    #[test]
    fn catalog_administration_is_admin_only_and_reads_drafts() {
        let tariff_id = "3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0";
        assert!(role_allows_request(
            &Method::GET,
            "/v1/admin/catalog/tariffs",
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::GET,
            "/v1/admin/catalog/tariffs",
            "BILLING_AGENT"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/tariffs/{tariff_id}/revoke"),
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/tariffs/{tariff_id}/revoke"),
            "SUPERVISOR"
        ));
    }

    #[test]
    fn product_catalog_mutations_require_admin_and_public_reads_remain_available() {
        let product_id = "3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0";
        assert!(role_allows_request(
            &Method::GET,
            "/v1/admin/catalog/products",
            "ADMIN"
        ));
        assert!(!role_allows_request(
            &Method::GET,
            "/v1/admin/catalog/products",
            "SUPERVISOR"
        ));
        assert!(role_allows_request(
            &Method::POST,
            "/v1/admin/catalog/products",
            "ADMIN"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/products/{product_id}/approve"),
            "ADMIN"
        ));
        assert!(role_allows_request(
            &Method::POST,
            &format!("/v1/admin/catalog/products/{product_id}/revoke"),
            "ADMIN"
        ));
        for role in ["BILLING_AGENT", "CASHIER", "SUPERVISOR", "AUDITOR_READONLY"] {
            assert!(!role_allows_request(
                &Method::POST,
                "/v1/admin/catalog/products",
                role
            ));
        }
        assert!(role_allows_request(
            &Method::GET,
            "/v1/catalog/products",
            "BILLING_AGENT"
        ));
    }

    #[test]
    fn tax_and_exchange_rate_administration_is_admin_only() {
        for catalog in ["tax-profiles", "fx-rates"] {
            let collection = format!("/v1/admin/catalog/{catalog}");
            let version = format!("{collection}/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0");
            assert!(role_allows_request(&Method::GET, &collection, "ADMIN"));
            assert!(role_allows_request(&Method::POST, &collection, "ADMIN"));
            assert!(role_allows_request(
                &Method::POST,
                &format!("{version}/approve"),
                "ADMIN"
            ));
            assert!(role_allows_request(
                &Method::POST,
                &format!("{version}/revoke"),
                "ADMIN"
            ));
            for role in ["BILLING_AGENT", "CASHIER", "SUPERVISOR", "AUDITOR_READONLY"] {
                assert!(!role_allows_request(&Method::POST, &collection, role));
                assert!(!role_allows_request(
                    &Method::POST,
                    &format!("{version}/approve"),
                    role
                ));
                assert!(!role_allows_request(
                    &Method::POST,
                    &format!("{version}/revoke"),
                    role
                ));
            }
        }
    }
}
