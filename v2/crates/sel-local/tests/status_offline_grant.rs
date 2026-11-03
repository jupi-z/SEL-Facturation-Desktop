use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_local::router_with_store_and_auth;
use sel_storage::LocalStore;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn sync_status_reports_configured_offline_identity() {
    let path = std::env::temp_dir().join(format!("sel-status-{}.sqlite", Uuid::new_v4()));
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let mut store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        organization_id,
        device_id,
    )
    .await
    .unwrap();
    store
        .configure_offline_identity(actor_id, grant_id)
        .await
        .unwrap();

    let response = router_with_store_and_auth(store, "test-token")
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["offline_grant"], "CONFIGURED");

    drop(store);
    let _ = std::fs::remove_file(path);
}
