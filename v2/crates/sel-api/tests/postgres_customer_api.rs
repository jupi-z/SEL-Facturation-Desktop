use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn customer_api_reads_from_postgres_after_new_app_state() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL API PostgreSQL fixture")
        .await
        .expect("create fixture organization");
    let app = router(AppState::with_central_store(store.clone()));
    let body = serde_json::to_vec(&json!({
        "customer_code": "PG-API-1",
        "legal_name": "Postgres API fixture",
        "address": "Central address",
        "phone": "+243000000001"
    }))
    .unwrap();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/customers")
        .header("authorization", "Bearer test")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let created = app.clone().oneshot(request).await.unwrap();
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let created: serde_json::Value =
        serde_json::from_slice(&created.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let customer_id = created["id"].as_str().expect("customer id");
    let clear_phone = Request::builder()
        .method("PATCH")
        .uri(format!("/v1/customers/{customer_id}"))
        .header("authorization", "Bearer test")
        .header("if-match", "1")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"phone": null})).unwrap(),
        ))
        .unwrap();
    let updated = app.clone().oneshot(clear_phone).await.unwrap();
    assert_eq!(updated.status(), axum::http::StatusCode::OK);
    let updated: serde_json::Value =
        serde_json::from_slice(&updated.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(updated["address"], "Central address");
    assert!(updated["phone"].is_null());
    drop(app);
    drop(store);

    let reopened = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("reconnect central store");
    let app = router(AppState::with_central_store(reopened));
    let request = Request::builder()
        .uri(format!("/v1/customers/{customer_id}"))
        .header("authorization", "Bearer test")
        .body(Body::empty())
        .unwrap();
    let loaded = app.oneshot(request).await.unwrap();
    assert_eq!(loaded.status(), axum::http::StatusCode::OK);
    let loaded: serde_json::Value =
        serde_json::from_slice(&loaded.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(loaded["address"], "Central address");
    assert!(loaded["phone"].is_null());

    let search = Request::builder()
        .uri("/v1/customers?q=FIXTURE")
        .header("authorization", "Bearer test")
        .body(Body::empty())
        .unwrap();
    let searched = router(AppState::with_central_store(
        CentralStore::connect(&database_url, organization_id)
            .await
            .expect("connect central store for search"),
    ))
    .oneshot(search)
    .await
    .unwrap();
    assert_eq!(searched.status(), axum::http::StatusCode::OK);
    let searched: serde_json::Value =
        serde_json::from_slice(&searched.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(searched.as_array().unwrap().len(), 1);
    assert_eq!(searched[0]["customer_code"], "PG-API-1");
}
