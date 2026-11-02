use chrono::Utc;
use sel_contracts::{SyncEnvelope, SyncEventResult};
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_command_journal_and_outbox_survive_reopen() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-{}.sqlite", Uuid::new_v4()));
    let url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let event_id = Uuid::new_v4();
    let aggregate_id = Uuid::new_v4();
    let payload = json!({"customer_code":"LOCAL-1","legal_name":"Offline fixture"});
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id,
        device_id,
        organization_id,
        actor_id: Uuid::new_v4(),
        command_type: "CUSTOMER_CREATED".into(),
        aggregate_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: Uuid::new_v4(),
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    let first = LocalStore::connect(&url, organization_id, device_id)
        .await
        .expect("open local store");
    first
        .record_command(&event)
        .await
        .expect("record offline command");
    assert_eq!(first.pending_count().await.unwrap(), 1);
    drop(first);

    let second = LocalStore::connect(&url, organization_id, device_id)
        .await
        .expect("reopen local store");
    assert_eq!(second.pending_count().await.unwrap(), 1);
    second
        .acknowledge(
            event_id,
            &SyncEventResult::Accepted {
                server_sequence: 1,
                server_entity_id: aggregate_id,
            },
        )
        .await
        .expect("acknowledge after reconnect");
    assert_eq!(second.pending_count().await.unwrap(), 0);
    assert_eq!(second.journal_count(event_id).await.unwrap(), 1);
    drop(second);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}
