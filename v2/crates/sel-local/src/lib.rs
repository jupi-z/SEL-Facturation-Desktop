use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use sel_storage::LocalStore;
use serde_json::json;
use std::net::SocketAddr;

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

pub async fn run(addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router()).await
}

pub async fn run_with_store(addr: SocketAddr, store: LocalStore) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router_with_store(store)).await
}
