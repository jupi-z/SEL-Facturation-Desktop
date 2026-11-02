use chrono::Utc;
use sel_contracts::{SyncEnvelope, SyncEventResult};
use sel_sync::{payload_digest, SyncEngine};
use serde_json::json;
use uuid::Uuid;

fn event(id: Uuid) -> SyncEnvelope {
    let payload = json!({"payment_id":id,"amount":"15.00","currency":"USD"});
    SyncEnvelope {
        protocol_version: 1,
        event_id: id,
        device_id: Uuid::new_v4(),
        organization_id: Uuid::new_v4(),
        actor_id: Uuid::new_v4(),
        command_type: "PAYMENT_RECORDED".into(),
        aggregate_id: Uuid::new_v4(),
        aggregate_expected_version: 1,
        occurred_at: Utc::now(),
        offline_grant_id: Uuid::new_v4(),
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    }
}

#[test]
fn retry_after_commit_is_duplicate_and_keeps_one_change() {
    let mut engine = SyncEngine::default();
    let item = event(Uuid::new_v4());
    let first = engine.push(std::slice::from_ref(&item)).unwrap();
    let second = engine.push(std::slice::from_ref(&item)).unwrap();
    assert!(matches!(first[0], SyncEventResult::Accepted { .. }));
    assert!(matches!(second[0], SyncEventResult::Duplicate { .. }));
    assert_eq!(engine.pull(0, 100).0.len(), 1);
}
