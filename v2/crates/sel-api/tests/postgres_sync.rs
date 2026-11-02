use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use sel_sync::payload_digest;
use serde_json::json;
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn sync_ack_is_durable_and_only_follows_business_commit() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
    migrate_postgres(&store.pool).await.unwrap();
    store.ensure_organization("sync fixture").await.unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'BILLING_AGENT', 'ACTIVE', 'sync-fixture')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'fixture-key', 'ACTIVE')")
        .bind(device_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now(), now() + interval '1 day', 'fixture')")
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let aggregate_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let payload = json!({"customer_code":"SYNC-PG-1","legal_name":"Sync customer"});
    let event = json!({
        "protocol_version": 1,
        "event_id": event_id,
        "device_id": device_id,
        "organization_id": organization_id,
        "actor_id": actor_id,
        "command_type": "CUSTOMER_CREATED",
        "aggregate_id": aggregate_id,
        "aggregate_expected_version": 0,
        "occurred_at": "2026-11-02T10:00:00Z",
        "offline_grant_id": grant_id,
        "payload": payload,
        "payload_sha256": payload_digest(&payload),
        "client_schema_version": 1
    });
    let body = json!({"device_id": device_id, "events": [event]});
    let push = || {
        Request::builder()
            .method("POST")
            .uri("/v1/sync/push")
            .header("authorization", "Bearer test")
            .header("Idempotency-Key", "pg-sync-key-1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let app = router(AppState::with_central_store(store.clone()));
    let first = app.clone().oneshot(push()).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first = json_body(first).await;
    assert_eq!(first["results"][0]["status"], "ACCEPTED");
    assert!(first["results"][0]["server_sequence"].as_i64().unwrap() > 0);
    let retry = app.clone().oneshot(push()).await.unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(json_body(retry).await["results"][0]["status"], "DUPLICATE");
    let mut conflicting_event = event.clone();
    conflicting_event["payload"] = json!({
        "customer_code": "SYNC-PG-1",
        "legal_name": "tampered payload"
    });
    conflicting_event["payload_sha256"] = json!("tampered");
    let conflict = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-sync-key-conflict")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(
                        &json!({"device_id": device_id, "events": [conflicting_event]}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::OK);
    assert_eq!(
        json_body(conflict).await["results"][0]["status"],
        "CONFLICT"
    );
    let conflict_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM sync_conflicts WHERE device_id = $1 AND event_id = $2",
    )
    .bind(device_id)
    .bind(event_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(conflict_count, 1);
    let count = sqlx::query("SELECT count(*) AS count FROM customers WHERE id = $1")
        .bind(aggregate_id)
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .get::<i64, _>("count");
    assert_eq!(count, 1);
    let pulled = app
        .clone()
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
    assert_eq!(
        json_body(pulled).await["changes"].as_array().unwrap().len(),
        1
    );
    let status = app
        .oneshot(
            Request::builder()
                .uri("/v1/sync/status")
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status = json_body(status).await;
    assert_eq!(status["conflicts"], 1);
    assert_eq!(status["rejected"], 0);
}
