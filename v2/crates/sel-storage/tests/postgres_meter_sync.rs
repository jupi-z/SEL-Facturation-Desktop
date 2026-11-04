use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    Meter, MeterReading, PatchValue, ServiceAccount, ServiceAccountUpdateSyncPayload, SyncEnvelope,
    SyncEventResult, UpdateServiceAccount,
};
use sel_storage::{migrate_postgres, CentralStore};
use sel_sync::payload_digest;
use sqlx::{postgres::PgPoolOptions, Row};
use std::str::FromStr;
use uuid::Uuid;

fn envelope(
    organization_id: Uuid,
    device_id: Uuid,
    actor_id: Uuid,
    grant_id: Uuid,
    aggregate_id: Uuid,
    command_type: &str,
    payload: serde_json::Value,
) -> SyncEnvelope {
    SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: command_type.into(),
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
async fn offline_meter_account_and_reading_events_apply_atomically_and_idempotently() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let customer_id = Uuid::new_v4();
    let schema = format!("meter_sync_{}", Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .unwrap();
    let options = sqlx::postgres::PgConnectOptions::from_str(&database_url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .unwrap();
    admin_pool.close().await;
    let store = CentralStore::from_pool(pool, organization_id);
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("meter event fixture")
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'SUPERVISOR', 'ACTIVE', 'meter-sync')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'meter-sync-device', 'ACTIVE')")
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
    sqlx::query("INSERT INTO customers (id, organization_id, customer_code, legal_name, status) VALUES ($1, $2, 'METER-SYNC-1', 'Meter sync customer', 'ACTIVE')")
        .bind(customer_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();

    let account = ServiceAccount {
        id: Uuid::new_v4(),
        customer_id,
        service_code: "WATER".into(),
        contract_ref: Some("WATER-SYNC-1".into()),
        connection_status: "CONNECTED".into(),
        activated_at: Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        revision: 1,
    };
    let meter = Meter {
        id: Uuid::new_v4(),
        service_account_id: account.id,
        serial: "WATER-METER-SYNC-1".into(),
        unit: "M3".into(),
        active: true,
    };
    let initial = MeterReading {
        id: Uuid::new_v4(),
        meter_id: meter.id,
        period: "2026-10".into(),
        previous: Decimal::ZERO,
        current: Decimal::ZERO,
        measured_quantity: Decimal::ZERO,
        source: "MANUAL".into(),
        corrected_from_id: None,
        correction_reason: None,
    };
    let correction = MeterReading {
        id: Uuid::new_v4(),
        meter_id: meter.id,
        period: initial.period.clone(),
        previous: Decimal::ZERO,
        current: Decimal::new(125, 2),
        measured_quantity: Decimal::new(125, 2),
        source: "MANUAL".into(),
        corrected_from_id: Some(initial.id),
        correction_reason: Some("Index relu".into()),
    };

    for (id, command, value) in [
        (
            account.id,
            "SERVICE_ACCOUNT_CREATED",
            serde_json::to_value(&account).unwrap(),
        ),
        (
            meter.id,
            "METER_CREATED",
            serde_json::to_value(&meter).unwrap(),
        ),
        (
            initial.id,
            "METER_READING_CREATED",
            serde_json::to_value(&initial).unwrap(),
        ),
        (
            correction.id,
            "METER_READING_CORRECTED",
            serde_json::to_value(&correction).unwrap(),
        ),
    ] {
        let event = envelope(
            organization_id,
            device_id,
            actor_id,
            grant_id,
            id,
            command,
            value,
        );
        let result = store.apply_sync_event(&event).await.unwrap();
        assert!(
            matches!(result, SyncEventResult::Accepted { .. }),
            "{command}: {result:?}"
        );
        assert!(matches!(
            store.apply_sync_event(&event).await.unwrap(),
            SyncEventResult::Duplicate { .. }
        ));
    }

    let updated_account = ServiceAccount {
        contract_ref: Some("WATER-SYNC-UPDATED".into()),
        connection_status: "NOT_CONNECTED".into(),
        revision: 2,
        ..account.clone()
    };
    let update_event = envelope(
        organization_id,
        device_id,
        actor_id,
        grant_id,
        account.id,
        "SERVICE_ACCOUNT_UPDATED",
        serde_json::to_value(ServiceAccountUpdateSyncPayload {
            expected_revision: 1,
            account: updated_account.clone(),
        })
        .unwrap(),
    );
    assert!(matches!(
        store.apply_sync_event(&update_event).await.unwrap(),
        SyncEventResult::Accepted { .. }
    ));
    assert!(matches!(
        store.apply_sync_event(&update_event).await.unwrap(),
        SyncEventResult::Duplicate { .. }
    ));
    let stored_account: (Option<String>, String, i64) = sqlx::query_as(
        "SELECT contract_ref, connection_status, revision FROM service_accounts WHERE id = $1",
    )
    .bind(account.id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        stored_account,
        (Some("WATER-SYNC-UPDATED".into()), "NOT_CONNECTED".into(), 2)
    );
    let central_updated = store
        .update_service_account(
            account.id,
            2,
            &UpdateServiceAccount {
                contract_ref: PatchValue::Missing,
                connection_status: Some("CONNECTED".into()),
                activated_at: PatchValue::Missing,
            },
            Some(actor_id),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(central_updated.revision, 3);
    assert_eq!(central_updated.connection_status, "CONNECTED");
    assert!(matches!(
        store
            .update_service_account(
                account.id,
                2,
                &UpdateServiceAccount {
                    contract_ref: PatchValue::Missing,
                    connection_status: Some("UNKNOWN".into()),
                    activated_at: PatchValue::Missing,
                },
                Some(actor_id),
            )
            .await,
        Err(sel_storage::postgres::MeterWriteError::ServiceAccountRevisionConflict)
    ));

    let reading_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM meter_readings WHERE meter_id = $1")
            .bind(meter.id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(reading_count, 2);
    let stored_correction: (Decimal, Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT current, corrected_from_id, correction_reason FROM meter_readings WHERE id = $1",
    )
    .bind(correction.id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        stored_correction,
        (
            Decimal::new(125, 2),
            Some(initial.id),
            Some("Index relu".into())
        )
    );
    let change_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM central_changes
         WHERE entity IN ('service_accounts', 'meters', 'meter_readings')",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(change_count, 6);

    let row = sqlx::query("SELECT current_schema() AS schema")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    let schema: String = row.try_get("schema").unwrap();
    store.pool.close().await;
    let cleanup_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .unwrap();
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&cleanup_pool)
        .await
        .unwrap();
    cleanup_pool.close().await;
}
