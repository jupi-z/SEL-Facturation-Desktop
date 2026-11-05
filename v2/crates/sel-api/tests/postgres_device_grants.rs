use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_contracts::offline_grant_canonical_bytes;
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn admin_can_enroll_revoke_device_and_issue_signed_offline_grant() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    std::env::remove_var("SEL_ALLOW_LEGACY_TOKEN");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL device and grant fixture")
        .await
        .expect("create fixture organization");

    let admin_id = Uuid::new_v4();
    let target_user_id = Uuid::new_v4();
    let password_hash = CentralStore::hash_user_password("correct horse battery staple")
        .expect("hash fixture password");
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
         VALUES ($1, $2, 'ADMIN', 'ACTIVE', 'device-admin', $3),
                ($4, $2, 'CASHIER', 'ACTIVE', 'device-cashier', NULL)",
    )
    .bind(admin_id)
    .bind(organization_id)
    .bind(password_hash)
    .bind(target_user_id)
    .execute(&store.pool)
    .await
    .expect("insert fixture users");

    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let device_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::days(1);
    let scopes = json!(["CUSTOMER_CREATED", "PAYMENT_RECORDED"]);
    let limits = json!({"max_events": 5, "max_payment_amount": "100.00"});
    let canonical = offline_grant_canonical_bytes(
        grant_id,
        target_user_id,
        device_id,
        &scopes,
        &limits,
        issued_at,
        expires_at,
    )
    .expect("canonical grant");
    let signature = BASE64.encode(signing_key.sign(&canonical).to_bytes());

    let store = store.with_offline_grant_verifier(signing_key.verifying_key());
    let app = router(AppState::with_production_central_store(store.clone()));
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "identity_ref": "device-admin",
                        "password": "correct horse battery staple"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let access_token = json_body(login).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let enroll = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/devices/enroll")
                .header("authorization", format!("Bearer {access_token}"))
                .header("Idempotency-Key", "device-enroll-fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "device_id": device_id,
                        "public_key_id": "fixture-device-key"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enroll.status(), StatusCode::CREATED);
    assert_eq!(json_body(enroll).await["state"], "ACTIVE");

    let grant = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/offline-grants")
                .header("authorization", format!("Bearer {access_token}"))
                .header("Idempotency-Key", "grant-create-fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "grant_id": grant_id,
                        "user_id": target_user_id,
                        "device_id": device_id,
                        "scopes": scopes,
                        "limits": limits,
                        "issued_at": issued_at,
                        "expires_at": expires_at,
                        "signature": signature
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(grant.status(), StatusCode::CREATED);
    assert_eq!(json_body(grant).await["grant_id"], grant_id.to_string());

    let listed_devices = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/devices")
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed_devices.status(), StatusCode::OK);
    assert_eq!(json_body(listed_devices).await.as_array().unwrap().len(), 1);
    let listed_grants = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/offline-grants")
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed_grants.status(), StatusCode::OK);
    let grants = json_body(listed_grants).await;
    assert_eq!(grants.as_array().unwrap().len(), 1);
    assert!(grants[0].get("signature").is_none());

    let revoke_grant = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/offline-grants/{grant_id}/revoke"))
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_grant.status(), StatusCode::NO_CONTENT);
    let repeat_revoke = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/offline-grants/{grant_id}/revoke"))
                .header("authorization", format!("Bearer {access_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeat_revoke.status(), StatusCode::NO_CONTENT);
    let revoked_at = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
        "SELECT revoked_at FROM offline_grants WHERE id = $1",
    )
    .bind(grant_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert!(revoked_at.is_some());

    let revoke = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/devices/{device_id}/revoke"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("Idempotency-Key", "device-revoke-fixture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke.status(), StatusCode::NO_CONTENT);
}
