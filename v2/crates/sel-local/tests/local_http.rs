use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_contracts::SyncEnvelope;
use sel_local::{router_with_store, router_with_store_and_auth};
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

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

#[tokio::test]
async fn local_production_router_requires_the_configured_bearer_token() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-auth-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let app = router_with_store_and_auth(store, "local-test-token-0123456789");

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), axum::http::StatusCode::UNAUTHORIZED);

    let invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), axum::http::StatusCode::UNAUTHORIZED);

    let accepted = app
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .header("authorization", "Bearer local-test-token-0123456789")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), axum::http::StatusCode::OK);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn utility_preview_returns_both_services_without_persisting_financial_data() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-preview-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let store = LocalStore::connect(&database_url, organization_id, Uuid::new_v4())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.095', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', '1.30', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    let (customer, _) = store
        .create_customer(&sel_contracts::CreateCustomer {
            customer_code: "PREVIEW-001".into(),
            legal_name: "Preview customer".into(),
            address: None,
            phone: None,
        })
        .await
        .unwrap();
    let before = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT
           (SELECT count(*) FROM invoices),
           (SELECT count(*) FROM receivables),
           (SELECT count(*) FROM outbox)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();

    let response = router_with_store(store.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/billing/utility/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer.id,
                        "billing_period": "2026-11",
                        "electricity": {"quantity": "100", "unit": "KWH"},
                        "water": {"quantity": "0", "unit": "M3"},
                        "water_zero_confirmed": true,
                        "currency": "USD",
                        "tariff_revision": "local-approved-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "LOCAL_PREVIEW");
    assert_eq!(body["receivable_id"], serde_json::Value::Null);
    assert_eq!(body["lines"].as_array().unwrap().len(), 2);
    assert!(body["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| { line["service_code"] == "WATER" && line["quantity"] == "0" }));

    let after = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT
           (SELECT count(*) FROM invoices),
           (SELECT count(*) FROM receivables),
           (SELECT count(*) FROM outbox)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let _ = std::fs::remove_file(path);
}
