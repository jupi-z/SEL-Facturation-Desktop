use axum::{routing::post, Json, Router};
use chrono::Utc;
use sel_contracts::{
    SyncEnvelope, SyncEventResult, SyncPullResponse, SyncPushRequest, SyncPushResponse,
};
use sel_local::{spawn_sync_worker, sync_once};
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
async fn automatic_sync_worker_retries_until_the_outbox_is_empty() {
    let path = std::env::temp_dir().join(format!("sel-v2-auto-worker-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let payload = serde_json::json!({"customer_code":"AUTO-SYNC-1"});
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
    let worker = spawn_sync_worker(
        store.clone(),
        format!("http://{address}"),
        "local-test-token",
        std::time::Duration::from_millis(50),
    );
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(store.pending_count().await.unwrap(), 0);
    worker.abort();
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

#[tokio::test]
async fn pulled_utility_invoice_and_payment_changes_project_locally() {
    let path =
        std::env::temp_dir().join(format!("sel-v2-financial-pull-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let store = LocalStore::connect(&database_url, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let payment_id = Uuid::new_v4();
    let customer_change = serde_json::json!({
        "sequence": 1,
        "entity": "customers",
        "entity_id": customer_id,
        "revision": 1,
        "change": {"customer_code":"PULL-FIN-1","legal_name":"Pull finance customer","status":"ACTIVE"}
    });
    let invoice_change = serde_json::json!({
        "sequence": 2,
        "entity": "invoices",
        "entity_id": invoice_id,
        "revision": 1,
        "change": {
            "request": {
                "customer_id": customer_id,
                "billing_period": "2026-11",
                "electricity": {"quantity":"100","unit":"KWH"},
                "water": {"quantity":"0","unit":"M3"},
                "water_zero_confirmed": true,
                "currency":"USD",
                "tariff_revision":"fixture",
                "due_date":"2026-11-30"
            },
            "response": {
                "invoice_id": invoice_id,
                "receivable_id": receivable_id,
                "invoice_number": null,
                "status":"CENTRAL_ACCEPTED",
                "currency":"USD",
                "lines":[
                    {"service_code":"ELECTRICITY","quantity":"100","unit":"KWH","unit_price":"0.10","subtotal":"10.00","tax":"1.60","total":"11.60"},
                    {"service_code":"WATER","quantity":"0","unit":"M3","unit_price":null,"subtotal":"0.00","tax":"0.00","total":"0.00"}
                ],
                "subtotal":"10.00","tax":"1.60","total":"11.60","snapshot_sha256":"fixture"
            }
        }
    });
    let payment_change = serde_json::json!({
        "sequence": 3,
        "entity": "payments",
        "entity_id": payment_id,
        "revision": 1,
        "change": {
            "customer_id": customer_id,
            "payment": {"payment_id": payment_id,"status":"CENTRAL_ACCEPTED","amount":"3.00","currency":"USD"},
            "allocations":[{"receivable_id":receivable_id,"amount":"3.00"}]
        }
    });
    store
        .apply_pulled_changes(&sel_contracts::SyncPullResponse {
            changes: vec![customer_change, invoice_change, payment_change],
            next_cursor: 3,
        })
        .await
        .expect("apply financial changes");
    assert!(store
        .get_invoice_response(invoice_id)
        .await
        .unwrap()
        .is_some());
    let receivable = store
        .get_receivable_summary(receivable_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receivable["paid"], "3.00");
    assert_eq!(receivable["balance"], "8.60");
    assert_eq!(store.central_cursor().await.unwrap(), 3);
    let _ = std::fs::remove_file(path);
}
