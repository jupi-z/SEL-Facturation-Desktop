use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_sync::payload_digest;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn sync_push_is_idempotent_and_pull_exposes_a_change() {
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let payload = json!({"customer_code":"OFFLINE-1"});
    let event = json!({
        "protocol_version":1,
        "event_id":event_id,
        "device_id":device_id,
        "organization_id":organization_id,
        "actor_id":Uuid::new_v4(),
        "command_type":"CUSTOMER_CREATED",
        "aggregate_id":aggregate_id,
        "aggregate_expected_version":0,
        "occurred_at":"2026-11-02T10:00:00Z",
        "offline_grant_id":Uuid::new_v4(),
        "payload":payload,
        "payload_sha256":payload_digest(&json!({"customer_code":"OFFLINE-1"})),
        "client_schema_version":1
    });
    let app = router(AppState::new(organization_id));
    let request = json!({"device_id":device_id,"events":[event]});
    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "sync-test-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "sync-test-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let body = json_body(second).await;
    assert_eq!(body["results"][0]["status"], "DUPLICATE");
    let pulled = app
        .oneshot(
            Request::builder()
                .uri("/v1/sync/pull?cursor=0&limit=10")
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pulled.status(), StatusCode::OK);
    let body = json_body(pulled).await;
    assert_eq!(body["changes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn solar_sale_and_activation_use_one_receivable_and_exact_schedule() {
    let organization_id = Uuid::new_v4();
    let customer_id = Uuid::new_v4();
    let app = router(AppState::new(organization_id));
    let sale = app.clone().oneshot(Request::builder().method("POST").uri("/v1/solar/sales").header("authorization", "Bearer test").header("Idempotency-Key", "solar-sale-1").header("content-type", "application/json").body(Body::from(serde_json::to_vec(&json!({"customer_id":customer_id,"product_code":"KIT-TEST","principal":"100.00","currency":"USD","terms_revision":"fixture-v1"})).unwrap())).unwrap()).await.unwrap();
    assert_eq!(sale.status(), StatusCode::CREATED);
    let sale_body = json_body(sale).await;
    let activation = app.oneshot(Request::builder().method("POST").uri(format!("/v1/solar/contracts/{}/activate", Uuid::new_v4())).header("authorization", "Bearer test").header("Idempotency-Key", "solar-activate-1").header("content-type", "application/json").body(Body::from(serde_json::to_vec(&json!({"sale_id":sale_body["sale_id"],"receivable_id":sale_body["receivable_id"],"schedule":[{"sequence":1,"due_date":"2026-12-01","amount":"50.00"},{"sequence":2,"due_date":"2027-01-01","amount":"50.00"}],"currency":"USD","approved_terms_revision":"fixture-v1"})).unwrap())).unwrap()).await.unwrap();
    assert_eq!(activation.status(), StatusCode::OK);
}
