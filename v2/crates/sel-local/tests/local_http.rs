use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_contracts::SyncEnvelope;
use sel_local::router_with_store;
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn local_status_reads_durable_outbox_counts() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-http-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let payload = json!({"customer_code":"HTTP-LOCAL"});
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id: store.device_id,
        organization_id: store.organization_id,
        actor_id: Uuid::new_v4(),
        command_type: "CUSTOMER_CREATED".into(),
        aggregate_id: Uuid::new_v4(),
        aggregate_expected_version: 0,
        occurred_at: chrono::Utc::now(),
        offline_grant_id: Uuid::new_v4(),
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    store.record_command(&event).await.unwrap();
    let app = router_with_store(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "LOCAL_PENDING");
    assert_eq!(body["pending"], 1);
    let _ = std::fs::remove_file(path);
}
