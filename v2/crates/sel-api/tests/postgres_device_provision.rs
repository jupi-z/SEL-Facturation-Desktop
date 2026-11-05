use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_contracts::offline_grant_canonical_bytes;
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn device_provisioning_is_atomic_signed_scoped_and_sync_token_is_revocable() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    std::env::remove_var("SEL_ALLOW_LEGACY_TOKEN");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL device provisioning fixture")
        .await
        .unwrap();

    let admin_id = Uuid::new_v4();
    let password_hash = CentralStore::hash_user_password("provisioning fixture pass")
        .expect("hash administrator password");
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
         VALUES ($1,$2,'ADMIN','ACTIVE','provision-admin',$3)",
    )
    .bind(admin_id)
    .bind(organization_id)
    .bind(password_hash)
    .execute(&store.pool)
    .await
    .unwrap();

    let operator_id = Uuid::new_v4();
    let operator_password = CentralStore::hash_user_password("provisioning operator pass")
        .expect("hash operator password");
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref, password_hash)
         VALUES ($1,$2,'BILLING_AGENT','ACTIVE','provision-operator',$3)",
    )
    .bind(operator_id)
    .bind(organization_id)
    .bind(operator_password)
    .execute(&store.pool)
    .await
    .unwrap();

    let signing_key = SigningKey::from_bytes(&[11; 32]);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let store = store.with_offline_grant_verifier(verifying_key);
    let app = router(
        AppState::with_production_central_store(store.clone())
            .with_offline_grant_signer(signing_key),
    );
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "identity_ref":"provision-admin",
                        "password":"provisioning fixture pass"
                    })
                    .to_string(),
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

    let device_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let sync_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let sync_token_hash = format!("{:x}", Sha256::digest(sync_token.as_bytes()));
    let issued_at = Utc::now();
    let expires_at = issued_at + Duration::days(7);
    let request_body = json!({
        "device_id":device_id,
        "public_key_id":format!("test-{}", Uuid::new_v4()),
        "sync_token_sha256":sync_token_hash,
        "user_id":operator_id,
        "grant_id":grant_id,
        "scopes":["*"],
        "limits":{},
        "issued_at":issued_at,
        "expires_at":expires_at
    });
    let provision = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/devices/{device_id}/provision"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("idempotency-key", "device-provision-once")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(provision.status(), StatusCode::CREATED);
    let provisioned = json_body(provision).await;
    let signed_grant = &provisioned["offline_grant"];
    assert_eq!(signed_grant["grant_id"], grant_id.to_string());
    let canonical = offline_grant_canonical_bytes(
        grant_id,
        operator_id,
        device_id,
        &request_body["scopes"],
        &request_body["limits"],
        issued_at,
        expires_at,
    )
    .unwrap();
    let signature = Signature::from_slice(
        &BASE64
            .decode(signed_grant["signature"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    verifying_key
        .verify(&canonical, &signature)
        .expect("production issuer signature verifies");

    let replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/devices/{device_id}/provision"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("idempotency-key", "device-provision-once")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(json_body(replay).await, provisioned);

    let persisted_hash: String =
        sqlx::query_scalar("SELECT sync_token_hash FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(persisted_hash, sync_token_hash);
    let grant_count: i64 = sqlx::query_scalar("SELECT count(*) FROM offline_grants WHERE id = $1")
        .bind(grant_id)
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(grant_count, 1);
    let persisted_actor: Uuid =
        sqlx::query_scalar("SELECT user_id FROM offline_grants WHERE id = $1")
            .bind(grant_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(persisted_actor, operator_id);

    let sync = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", format!("Bearer {sync_token}"))
                .header("idempotency-key", "empty-sync-valid")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"device_id":device_id,"events":[]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sync.status(), StatusCode::OK);

    let mismatched = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", format!("Bearer {sync_token}"))
                .header("idempotency-key", "empty-sync-wrong-device")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"device_id":Uuid::new_v4(),"events":[]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatched.status(), StatusCode::UNAUTHORIZED);

    let customer_access = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/customers")
                .header("authorization", format!("Bearer {sync_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer_access.status(), StatusCode::UNAUTHORIZED);

    let revoke = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/devices/{device_id}/revoke"))
                .header("authorization", format!("Bearer {access_token}"))
                .header("idempotency-key", "device-revoke-once")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke.status(), StatusCode::NO_CONTENT);

    let revoked_sync = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/sync/push")
                .header("authorization", format!("Bearer {sync_token}"))
                .header("idempotency-key", "after-device-revoke")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"device_id":device_id,"events":[]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_sync.status(), StatusCode::UNAUTHORIZED);
}
