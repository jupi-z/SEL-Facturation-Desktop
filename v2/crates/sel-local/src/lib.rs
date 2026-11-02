use axum::{
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;

pub fn router() -> Router {
    Router::new()
        .route("/local/v1/sync/status", get(status))
        .route("/local/v1/sync/trigger", post(trigger_sync))
        .route("/local/v1/updates/status", get(update_status))
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

pub async fn run(addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router()).await
}
