use axum::{routing::post, Json, Router};
use chrono::Utc;
use sel_contracts::{
    SyncEnvelope, SyncEventResult, SyncPullResponse, SyncPushRequest, SyncPushResponse,
};
use sel_local::sync_once;
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use uuid::Uuid;

async fn accept_sync(Json(request): Json<SyncPushRequest>) -> Json<SyncPushResponse> {
    Json(SyncPushResponse {
        results: request
            .events
            .iter()
            .map(|event| SyncEventResult::Accepted {
                server_sequence: 1,
                server_entity_id: event.aggregate_id,
            })
            .collect(),
    })
}

async fn empty_pull() -> Json<SyncPullResponse> {
    Json(SyncPullResponse {
        changes: Vec::new(),
        next_cursor: 0,
    })
}

#[tokio::test]
async fn sync_worker_transmits_outbox_and_persists_ack() {
    let path = std::env::temp_dir().join(format!("sel-v2-worker-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let store = LocalStore::connect(&database_url, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("connect local store");
    let payload =
        serde_json::json!({"customer_code":"SYNC-WORKER-1","legal_name":"Worker fixture"});
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id: store.device_id,
        organization_id: store.organization_id,
        actor_id: Uuid::nil(),
        command_type: "CUSTOMER_CREATED".into(),
        aggregate_id: Uuid::new_v4(),
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: Uuid::nil(),
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    store.record_command(&event).await.expect("queue event");

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind fake sync server");
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/v1/sync/push", post(accept_sync))
                .route("/v1/sync/pull", axum::routing::get(empty_pull)),
        )
        .await
        .unwrap();
    });
    let summary = sync_once(&store, &format!("http://{address}"), "local-test-token")
        .await
        .expect("sync event");
    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.accepted, 1);
    assert_eq!(store.pending_count().await.unwrap(), 0);
    server.abort();
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pulled_customer_changes_are_applied_with_a_durable_cursor() {
    let path = std::env::temp_dir().join(format!("sel-v2-pull-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let store = LocalStore::connect(&database_url, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("connect local store");
    let customer_id = Uuid::new_v4();
    store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![serde_json::json!({
                "sequence": 5,
                "entity": "customers",
                "entity_id": customer_id,
                "revision": 1,
                "change": {"customer_code":"PULLED-1","legal_name":"Pulled customer","status":"ACTIVE"}
            })],
            next_cursor: 5,
        })
        .await
        .expect("apply pulled change");
    assert_eq!(store.central_cursor().await.unwrap(), 5);
    assert_eq!(
        store
            .get_customer(customer_id)
            .await
            .unwrap()
            .unwrap()
            .customer_code,
        "PULLED-1"
    );
    let _ = std::fs::remove_file(path);
}
