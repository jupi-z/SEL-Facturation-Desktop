use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use sel_contracts::{offline_grant_canonical_bytes, SyncEnvelope, SyncEventResult};
use sel_storage::{migrate_postgres, CentralStore};
use sel_sync::payload_digest;
use serde_json::json;
use uuid::Uuid;

fn event(
    organization_id: Uuid,
    device_id: Uuid,
    actor_id: Uuid,
    grant_id: Uuid,
    aggregate_id: Uuid,
) -> SyncEnvelope {
    let payload = json!({
        "customer_code": format!("SIGNED-{}", &aggregate_id.to_string()[..8]),
        "legal_name": "Signed offline grant fixture"
    });
    SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "CUSTOMER_CREATED".into(),
        aggregate_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    }
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn signed_offline_grant_is_verified_and_limits_are_enforced() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let signing = SigningKey::from_bytes(&[23u8; 32]);
    let scopes = json!({"commands": ["CUSTOMER_CREATED"]});
    let limits = json!({"max_events": 1});
    let issued_at = Utc::now() - Duration::minutes(1);
    let expires_at = Utc::now() + Duration::hours(1);
    let canonical = offline_grant_canonical_bytes(
        grant_id, actor_id, device_id, &scopes, &limits, issued_at, expires_at,
    )
    .unwrap();
    let signature = BASE64.encode(signing.sign(&canonical).to_bytes());

    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap()
        .with_offline_grant_verifier(signing.verifying_key());
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("signed offline fixture")
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'BILLING_AGENT', 'ACTIVE', 'signed-grant-user')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'signed-grant-device', 'ACTIVE')")
        .bind(device_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .bind(&scopes)
        .bind(&limits)
        .bind(issued_at)
        .bind(expires_at)
        .bind(&signature)
        .execute(&store.pool)
        .await
        .unwrap();

    let accepted = store
        .apply_sync_event(&event(
            organization_id,
            device_id,
            actor_id,
            grant_id,
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert!(
        matches!(accepted, SyncEventResult::Accepted { .. }),
        "accepted result: {accepted:?}"
    );

    let limited = store
        .apply_sync_event(&event(
            organization_id,
            device_id,
            actor_id,
            grant_id,
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        limited,
        SyncEventResult::Rejected {
            code,
            ..
        } if code == "DEVICE_OR_GRANT_NOT_AUTHORIZED"
    ));

    let forged_grant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, $6, $7)")
        .bind(forged_grant_id)
        .bind(actor_id)
        .bind(device_id)
        .bind(&scopes)
        .bind(issued_at)
        .bind(expires_at)
        .bind(BASE64.encode([0u8; 64]))
        .execute(&store.pool)
        .await
        .unwrap();
    let forged = store
        .apply_sync_event(&event(
            organization_id,
            device_id,
            actor_id,
            forged_grant_id,
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        forged,
        SyncEventResult::Rejected {
            code,
            ..
        } if code == "DEVICE_OR_GRANT_NOT_AUTHORIZED"
    ));
}
