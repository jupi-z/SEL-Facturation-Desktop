use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use chrono::Utc;
use sel_contracts::{
    utility_snapshot_sha256, SyncEnvelope, SyncEventResult, SyncPullResponse, SyncPushRequest,
    SyncPushResponse, UtilityBillingResponse,
};
use sel_local::{spawn_sync_worker, sync_once};
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
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
async fn pulled_meter_accounts_meters_and_corrections_project_idempotently() {
    let path =
        std::env::temp_dir().join(format!("sel-v2-meter-projection-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("open local database");
    let (customer, _) = store
        .create_customer(&sel_contracts::CreateCustomer {
            customer_code: "METER-PROJECTION-1".into(),
            legal_name: "Meter projection fixture".into(),
            customer_category: Some("RESIDENTIAL".into()),
            address: None,
            phone: None,
        })
        .await
        .expect("create dependency customer");
    let account_id = Uuid::new_v4();
    let meter_id = Uuid::new_v4();
    let reading_id = Uuid::new_v4();
    let correction_id = Uuid::new_v4();
    let correction_reason = "Index relu";
    let response = SyncPullResponse {
        changes: vec![
            json!({
                "sequence": 1,
                "entity": "service_accounts",
                "entity_id": account_id,
                "revision": 1,
                "change": {
                    "id": account_id,
                    "customer_id": customer.id,
                    "service_code": "WATER",
                    "contract_ref": "PROJECTION-CONTRACT",
                    "connection_status": "CONNECTED",
                    "activated_at": "2026-01-01"
                }
            }),
            json!({
                "sequence": 2,
                "entity": "meters",
                "entity_id": meter_id,
                "revision": 1,
                "change": {
                    "id": meter_id,
                    "service_account_id": account_id,
                    "serial": "PROJECTION-W-1",
                    "unit": "M3",
                    "active": true
                }
            }),
            json!({
                "sequence": 3,
                "entity": "meter_readings",
                "entity_id": reading_id,
                "revision": 1,
                "change": {
                    "id": reading_id,
                    "meter_id": meter_id,
                    "period": "2026-10",
                    "previous": "0",
                    "current": "0",
                    "measured_quantity": "0",
                    "source": "MANUAL",
                    "corrected_from_id": null,
                    "correction_reason": null
                }
            }),
            json!({
                "sequence": 4,
                "entity": "meter_readings",
                "entity_id": correction_id,
                "revision": 1,
                "change": {
                    "id": correction_id,
                    "meter_id": meter_id,
                    "period": "2026-10",
                    "previous": "0",
                    "current": "1.25",
                    "measured_quantity": "1.25",
                    "source": "MANUAL",
                    "corrected_from_id": reading_id,
                    "correction_reason": correction_reason
                }
            }),
        ],
        next_cursor: 4,
    };
    store
        .apply_pulled_changes(&response)
        .await
        .expect("apply pulled meter history");
    store
        .apply_pulled_changes(&response)
        .await
        .expect("replay pulled meter history");
    let readings = store
        .list_meter_readings(meter_id, Some("2026-10"))
        .await
        .expect("read projected history");
    assert_eq!(readings.len(), 2);
    assert_eq!(readings[0].id, reading_id);
    assert_eq!(readings[1].id, correction_id);
    assert_eq!(readings[1].corrected_from_id, Some(reading_id));
    let meters = store
        .list_meters(account_id)
        .await
        .expect("read projected meter");
    assert_eq!(meters.len(), 1);
    assert_eq!(meters[0].id, meter_id);
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pulled_service_account_revision_updates_existing_local_projection_idempotently() {
    let path =
        std::env::temp_dir().join(format!("sel-v2-account-revision-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("open local database");
    let (customer, _) = store
        .create_customer(&sel_contracts::CreateCustomer {
            customer_code: "ACCOUNT-REV-1".into(),
            legal_name: "Account revision fixture".into(),
            customer_category: Some("RESIDENTIAL".into()),
            address: None,
            phone: None,
        })
        .await
        .expect("create customer");
    let account = store
        .create_service_account(
            customer.id,
            &sel_contracts::CreateServiceAccount {
                service_code: "WATER".into(),
                contract_ref: Some("CONTRACT-OLD".into()),
                connection_status: "CONNECTED".into(),
                activated_at: None,
            },
        )
        .await
        .expect("create service account");
    let changes = SyncPullResponse {
        changes: vec![json!({
            "sequence": 1,
            "entity": "service_accounts",
            "entity_id": account.id,
            "revision": 2,
            "change": {
                "id": account.id,
                "customer_id": customer.id,
                "service_code": "WATER",
                "contract_ref": "CONTRACT-NEW",
                "connection_status": "NOT_CONNECTED",
                "activated_at": null,
                "revision": 2
            }
        })],
        next_cursor: 1,
    };
    store
        .apply_pulled_changes(&changes)
        .await
        .expect("apply revision update");
    store
        .apply_pulled_changes(&changes)
        .await
        .expect("replay revision update");
    let projected = store
        .list_service_accounts(customer.id)
        .await
        .expect("read account");
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].id, account.id);
    assert_eq!(projected[0].contract_ref.as_deref(), Some("CONTRACT-NEW"));
    assert_eq!(projected[0].connection_status, "NOT_CONNECTED");
    assert_eq!(projected[0].revision, 2);
    drop(store);
    let _ = std::fs::remove_file(path);
}

async fn malformed_then_accept(
    State(attempts): State<Arc<AtomicUsize>>,
    Json(request): Json<SyncPushRequest>,
) -> axum::response::Response {
    if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
        return (
            axum::http::StatusCode::OK,
            "response lost before acknowledgement",
        )
            .into_response();
    }
    Json(SyncPushResponse {
        results: request
            .events
            .iter()
            .map(|event| SyncEventResult::Duplicate {
                original: Box::new(SyncEventResult::Accepted {
                    server_sequence: 1,
                    server_entity_id: event.aggregate_id,
                }),
            })
            .collect(),
    })
    .into_response()
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
async fn sync_worker_retries_after_response_loss_before_ack() {
    let path = std::env::temp_dir().join(format!("sel-v2-ack-loss-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let store = LocalStore::connect(&database_url, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("connect local store");
    let payload = serde_json::json!({
        "customer_code": "ACK-LOSS-1",
        "legal_name": "Acknowledgement loss fixture"
    });
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

    let attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind fake sync server");
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn({
        let attempts = Arc::clone(&attempts);
        async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/v1/sync/push", post(malformed_then_accept))
                    .route("/v1/sync/pull", axum::routing::get(empty_pull))
                    .with_state(attempts),
            )
            .await
            .unwrap();
        }
    });

    let first = sync_once(&store, &format!("http://{address}"), "local-test-token").await;
    assert!(first.is_err(), "a lost acknowledgement must be retried");
    assert_eq!(store.pending_count().await.unwrap(), 1);

    let second = sync_once(&store, &format!("http://{address}"), "local-test-token")
        .await
        .expect("replay after lost acknowledgement");
    assert_eq!(second.duplicates, 1);
    assert_eq!(store.pending_count().await.unwrap(), 0);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

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
async fn local_restart_recovers_in_flight_outbox_without_changing_identity() {
    let path = std::env::temp_dir().join(format!("sel-v2-restart-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let store = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("connect local store");
    let payload = serde_json::json!({
        "customer_code": "RESTART-1",
        "legal_name": "Restart fixture"
    });
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
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

    let claimed = store.claim_pending(1).await.expect("claim event");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].event_id, event.event_id);
    drop(store);

    let reopened = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("reopen local store");
    let recovered = reopened
        .claim_pending(1)
        .await
        .expect("recover event after restart");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].event_id, event.event_id);
    assert_eq!(recovered[0].aggregate_id, event.aggregate_id);
    assert_eq!(reopened.organization_id, organization_id);
    assert_eq!(reopened.device_id, device_id);

    drop(reopened);
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
            changes: vec![
                serde_json::json!({
                    "sequence": 5,
                    "entity": "customers",
                    "entity_id": customer_id,
                    "revision": 1,
                    "change": {
                        "customer_code":"PULLED-1",
                        "legal_name":"Pulled customer",
                        "customer_category":"RESIDENTIAL",
                        "address":"Initial address",
                        "phone":"+243000000001",
                        "status":"ACTIVE"
                    }
                }),
                serde_json::json!({
                    "sequence": 6,
                    "entity": "customers",
                    "entity_id": customer_id,
                    "revision": 2,
                    "change": {"legal_name":"Legacy update","customer_category":"COMMERCIAL","phone":null}
                }),
                serde_json::json!({
                    "sequence": 7,
                    "entity": "customers",
                    "entity_id": customer_id,
                    "revision": 3,
                    "change": {
                        "legal_name":"Updated customer",
                        "customer_category":null,
                        "phone":null,
                        "patch_format_version":2
                    }
                }),
            ],
            next_cursor: 7,
        })
        .await
        .expect("apply pulled change");
    assert_eq!(store.central_cursor().await.unwrap(), 7);
    let projected = store.get_customer(customer_id).await.unwrap().unwrap();
    assert_eq!(projected.customer_code, "PULLED-1");
    assert_eq!(projected.legal_name, "Updated customer");
    assert_eq!(projected.customer_category, None);
    assert_eq!(projected.address.as_deref(), Some("Initial address"));
    assert!(projected.phone.is_none());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pulled_changes_roll_back_projection_and_cursor_after_mid_batch_failure() {
    let path = std::env::temp_dir().join(format!("sel-v2-pull-rollback-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let valid_change = serde_json::json!({
        "sequence": 11,
        "entity": "customers",
        "entity_id": customer_id,
        "revision": 1,
        "change": {"customer_code":"ROLLBACK-1","legal_name":"Rollback fixture","status":"ACTIVE"}
    });
    let malformed_change = serde_json::json!({
        "sequence": 12,
        "entity": "customers",
        "entity_id": Uuid::new_v4(),
        "revision": 1
    });
    let error = store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![valid_change.clone(), malformed_change],
            next_cursor: 12,
        })
        .await
        .expect_err("malformed change must abort the whole pull transaction");
    assert!(error.to_string().contains("sync change payload is missing"));
    assert_eq!(store.central_cursor().await.unwrap(), 0);
    assert!(store.get_customer(customer_id).await.unwrap().is_none());

    store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![valid_change],
            next_cursor: 11,
        })
        .await
        .expect("retry valid change after rollback");
    assert_eq!(store.central_cursor().await.unwrap(), 11);
    assert_eq!(
        store
            .get_customer(customer_id)
            .await
            .unwrap()
            .unwrap()
            .customer_code,
        "ROLLBACK-1"
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pulled_sequence_collision_rolls_back_without_advancing_cursor() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-pull-sequence-collision-{}.sqlite",
        Uuid::new_v4()
    ));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let original = serde_json::json!({
        "sequence": 21,
        "entity": "customers",
        "entity_id": customer_id,
        "revision": 1,
        "change": {"customer_code":"SEQ-ORIGINAL","legal_name":"Original","status":"ACTIVE"}
    });
    store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![original],
            next_cursor: 21,
        })
        .await
        .expect("apply original change");

    let conflicting = serde_json::json!({
        "sequence": 21,
        "entity": "customers",
        "entity_id": customer_id,
        "revision": 2,
        "change": {"customer_code":"SEQ-CONFLICT","legal_name":"Conflicting","status":"ACTIVE"}
    });
    let error = store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![conflicting],
            next_cursor: 22,
        })
        .await
        .expect_err("different content for an existing sequence must fail closed");
    assert!(error
        .to_string()
        .contains("sync pull sequence collision has different content"));
    assert_eq!(store.central_cursor().await.unwrap(), 21);
    assert_eq!(
        store
            .get_customer(customer_id)
            .await
            .unwrap()
            .unwrap()
            .customer_code,
        "SEQ-ORIGINAL"
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
    let mut invoice_change = serde_json::json!({
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
    let invoice_response: UtilityBillingResponse =
        serde_json::from_value(invoice_change["change"]["response"].clone())
            .expect("invoice fixture response");
    invoice_change["change"]["response"]["snapshot_sha256"] = serde_json::json!(
        utility_snapshot_sha256(&invoice_response).expect("invoice fixture hash")
    );
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

#[tokio::test]
async fn pulled_payment_allocations_keep_distinct_central_ids_even_when_amounts_match() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-pull-payment-allocation-ids-{}.sqlite",
        Uuid::new_v4()
    ));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let payment_id = Uuid::new_v4();
    let allocation_one_id = Uuid::new_v4();
    let allocation_two_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers
         (id, organization_id, customer_code, legal_name, contact_fields_json, status, revision, created_at)
         VALUES ($1, $2, 'PULL-ALLOC-IDS', 'Payment allocation ids', '{}', 'ACTIVE', 1, datetime('now'))",
    )
    .bind(customer_id.to_string())
    .bind(store.organization_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed customer");
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'INVOICE', $2, $3, 'USD', '10.00', 'OPEN')",
    )
    .bind(receivable_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind(customer_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed receivable");
    store
        .apply_pulled_changes(&sel_contracts::SyncPullResponse {
            changes: vec![serde_json::json!({
                "sequence": 1,
                "entity": "payments",
                "entity_id": payment_id,
                "revision": 2,
                "change": {
                    "customer_id": customer_id,
                    "payment": {
                        "payment_id": payment_id,
                        "status": "CENTRAL_ACCEPTED",
                        "amount": "4.00",
                        "currency": "USD"
                    },
                    "allocations": [
                        {
                            "allocation_id": allocation_one_id,
                            "receivable_id": receivable_id,
                            "amount": "2.00",
                            "currency": "USD"
                        },
                        {
                            "allocation_id": allocation_two_id,
                            "receivable_id": receivable_id,
                            "amount": "2.00",
                            "currency": "USD"
                        }
                    ]
                }
            })],
            next_cursor: 1,
        })
        .await
        .expect("project payment allocations");

    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payment_allocations WHERE payment_id = $1")
            .bind(payment_id.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("count allocations");
    assert_eq!(count, 2);
    let total: String = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(CAST(allocated_amount AS REAL)), 0) AS TEXT)
         FROM payment_allocations WHERE payment_id = $1",
    )
    .bind(payment_id.to_string())
    .fetch_one(&store.pool)
    .await
    .expect("sum allocations");
    assert_eq!(total.parse::<f64>().unwrap(), 4.0);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn central_acceptance_reconciles_pending_invoice_and_payment_without_duplicates() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-reconcile-pending-{}.sqlite",
        Uuid::new_v4()
    ));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let payment_id = Uuid::new_v4();
    let allocation_id = Uuid::new_v4();
    let central_allocation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers
         (id, organization_id, customer_code, legal_name, contact_fields_json, status, revision, created_at)
         VALUES ($1, $2, 'RECON-1', 'Reconciliation customer', '{}', 'ACTIVE', 1, datetime('now'))",
    )
    .bind(customer_id.to_string())
    .bind(store.organization_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed customer");
    let mut response = serde_json::json!({
        "invoice_id": invoice_id,
        "receivable_id": receivable_id,
        "invoice_number": null,
        "status": "CENTRAL_ACCEPTED",
        "currency": "USD",
        "lines": [
            {"service_code":"ELECTRICITY","quantity":"100","unit":"KWH","unit_price":"0.10","subtotal":"10.00","tax":"1.60","total":"11.60"},
            {"service_code":"WATER","quantity":"0","unit":"M3","unit_price":null,"subtotal":"0.00","tax":"0.00","total":"0.00"}
        ],
        "subtotal":"10.00","tax":"1.60","total":"11.60","snapshot_sha256":"fixture"
    });
    let response_object: UtilityBillingResponse =
        serde_json::from_value(response.clone()).expect("pending invoice fixture response");
    response["snapshot_sha256"] = serde_json::json!(
        utility_snapshot_sha256(&response_object).expect("pending invoice fixture hash")
    );
    sqlx::query(
        "INSERT INTO invoices
         (id, customer_id, period, issued_at, currency, total, template_version, immutable_snapshot_json, status, version)
         VALUES ($1, $2, '2026-11', datetime('now'), 'USD', '11.60', 'v2-a4-1', $3, 'LOCAL_PENDING', 1)",
    )
    .bind(invoice_id.to_string())
    .bind(customer_id.to_string())
    .bind(response.to_string())
    .execute(&store.pool)
    .await
    .expect("seed pending invoice");
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'INVOICE', $2, $3, 'USD', '11.60', 'OPEN')",
    )
    .bind(receivable_id.to_string())
    .bind(invoice_id.to_string())
    .bind(customer_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed receivable");
    sqlx::query(
        "INSERT INTO payments
         (id, customer_id, amount_received, currency, method, paid_at, status)
         VALUES ($1, $2, '3.00', 'USD', 'CASH', datetime('now'), 'LOCAL_PENDING')",
    )
    .bind(payment_id.to_string())
    .bind(customer_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed pending payment");
    sqlx::query(
        "INSERT INTO payment_allocations
         (id, payment_id, receivable_id, allocated_amount, currency)
         VALUES ($1, $2, $3, '3.00', 'USD')",
    )
    .bind(allocation_id.to_string())
    .bind(payment_id.to_string())
    .bind(receivable_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed pending allocation");

    store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![
                serde_json::json!({
                    "sequence": 10,
                    "entity": "invoices",
                    "entity_id": invoice_id,
                    "revision": 2,
                    "change": {
                        "request": {
                            "customer_id": customer_id,
                            "billing_period": "2026-11",
                            "electricity": {"quantity":"100","unit":"KWH"},
                            "water": {"quantity":"0","unit":"M3"},
                            "water_zero_confirmed": true,
                            "currency":"USD",
                            "tariff_revision":"fixture"
                        },
                        "response": response
                    }
                }),
                serde_json::json!({
                    "sequence": 11,
                    "entity": "payments",
                    "entity_id": payment_id,
                    "revision": 1,
                    "change": {
                        "customer_id": customer_id,
                        "payment": {"payment_id":payment_id,"status":"CENTRAL_ACCEPTED","amount":"3.00","currency":"USD"},
                        "allocations":[{"allocation_id":central_allocation_id,"receivable_id":receivable_id,"amount":"3.00","currency":"USD"}]
                    }
                })
            ],
            next_cursor: 11,
        })
        .await
        .expect("reconcile central acceptance");

    let local_invoice = store
        .get_invoice_response(invoice_id)
        .await
        .expect("read reconciled invoice")
        .expect("invoice exists");
    assert_eq!(local_invoice.status, "CENTRAL_ACCEPTED");
    let payment_status: String = sqlx::query_scalar("SELECT status FROM payments WHERE id = $1")
        .bind(payment_id.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("read payment status");
    assert_eq!(payment_status, "CENTRAL_ACCEPTED");
    let allocation_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payment_allocations WHERE payment_id = $1")
            .bind(payment_id.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("count allocations");
    assert_eq!(allocation_count, 1);
    let projected_allocation_id: String =
        sqlx::query_scalar("SELECT id FROM payment_allocations WHERE payment_id = $1")
            .bind(payment_id.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("read reconciled allocation identity");
    assert_eq!(projected_allocation_id, central_allocation_id.to_string());
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn pulled_credit_note_is_idempotent_and_updates_local_balance() {
    let path = std::env::temp_dir().join(format!("sel-v2-credit-pull-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let credit_note_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers
         (id, organization_id, customer_code, legal_name, contact_fields_json, status, revision, created_at)
         VALUES ($1, $2, 'CREDIT-1', 'Credit customer', '{}', 'ACTIVE', 1, datetime('now'))",
    )
    .bind(customer_id.to_string())
    .bind(store.organization_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed credit customer");
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'INVOICE', $2, $3, 'USD', '10.00', 'OPEN')",
    )
    .bind(receivable_id.to_string())
    .bind(Uuid::new_v4().to_string())
    .bind(customer_id.to_string())
    .execute(&store.pool)
    .await
    .expect("seed credit receivable");
    let pull = SyncPullResponse {
        changes: vec![serde_json::json!({
            "sequence": 20,
            "entity": "credit_notes",
            "entity_id": credit_note_id,
            "revision": 1,
            "change": {
                "credit_note_id": credit_note_id,
                "receivable_id": receivable_id,
                "amount": "2.50",
                "currency": "USD",
                "reason": "central correction",
                "actor_id": Uuid::new_v4(),
                "receivable_status": "CREDITED"
            }
        })],
        next_cursor: 20,
    };
    store
        .apply_pulled_changes(&pull)
        .await
        .expect("apply credit projection");
    store
        .apply_pulled_changes(&pull)
        .await
        .expect("replay credit projection");
    let credit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM credit_notes WHERE id = $1")
        .bind(credit_note_id.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("count projected credit note");
    assert_eq!(credit_count, 1);
    let summary = store
        .get_receivable_summary(receivable_id)
        .await
        .expect("read projected credit summary")
        .expect("projected receivable exists");
    assert_eq!(summary["credited"], "2.50");
    assert_eq!(summary["balance"], "7.50");
    assert_eq!(summary["status"], "CREDITED");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn unsupported_central_projection_is_audited_instead_of_dropped() {
    let path =
        std::env::temp_dir().join(format!("sel-v2-unsupported-pull-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");

    store
        .apply_pulled_changes(&SyncPullResponse {
            changes: vec![serde_json::json!({
                "sequence": 42,
                "entity": "future_financial_entity",
                "entity_id": Uuid::new_v4(),
                "revision": 3,
                "change": {"amount":"9.00","currency":"USD"}
            })],
            next_cursor: 42,
        })
        .await
        .expect("audit unsupported projection");

    let conflicts: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_conflicts")
        .fetch_one(&store.pool)
        .await
        .expect("count projection conflicts");
    assert_eq!(conflicts, 1);
    assert_eq!(store.central_cursor().await.unwrap(), 42);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn solar_sale_and_contract_projections_converge_idempotently() {
    let path = std::env::temp_dir().join(format!("sel-v2-solar-pull-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    let customer_id = Uuid::new_v4();
    let sale_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let contract_id = Uuid::new_v4();
    let plan_id = Uuid::new_v4();
    let sale_change = serde_json::json!({
        "sequence": 2,
        "entity": "sales",
        "entity_id": sale_id,
        "revision": 1,
        "change": {
            "sale_id": sale_id,
            "receivable_id": receivable_id,
            "customer_id": customer_id,
            "principal": "120.00",
            "currency": "USD",
            "status": "CENTRAL_ACCEPTED",
            "product_code": "KIT-100"
        }
    });
    let contract_change = serde_json::json!({
        "sequence": 3,
        "entity": "installment_plans",
        "entity_id": plan_id,
        "revision": 1,
        "change": {
            "status": "CENTRAL_ACCEPTED",
            "sale_id": sale_id,
            "receivable_id": receivable_id,
            "installment_plan_id": plan_id,
            "contract_id": contract_id,
            "principal": "120.00",
            "currency": "USD",
            "terms_revision": "solar-v1",
            "schedule": [
                {"sequence":1,"due_date":"2026-12-01","amount":"60.00"},
                {"sequence":2,"due_date":"2027-01-01","amount":"60.00"}
            ]
        }
    });
    let changes = vec![
        serde_json::json!({
            "sequence": 1,
            "entity": "customers",
            "entity_id": customer_id,
            "revision": 1,
            "change": {"customer_code":"SOLAR-PULL-1","legal_name":"Solar pull customer","status":"ACTIVE"}
        }),
        sale_change,
        contract_change,
    ];
    let pull = SyncPullResponse {
        changes: changes.clone(),
        next_cursor: 3,
    };
    store
        .apply_pulled_changes(&pull)
        .await
        .expect("project solar changes");
    store
        .apply_pulled_changes(&pull)
        .await
        .expect("replay solar changes");

    let sale_count: i64 = sqlx::query_scalar("SELECT count(*) FROM sales WHERE id = $1")
        .bind(sale_id.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("count projected sale");
    let line_count: i64 = sqlx::query_scalar("SELECT count(*) FROM sale_lines WHERE sale_id = $1")
        .bind(sale_id.to_string())
        .fetch_one(&store.pool)
        .await
        .expect("count projected sale lines");
    let schedule_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installment_schedule WHERE plan_id = $1")
            .bind(plan_id.to_string())
            .fetch_one(&store.pool)
            .await
            .expect("count projected schedule");
    let schedule_id: String = sqlx::query_scalar(
        "SELECT id FROM installment_schedule WHERE plan_id = $1 AND sequence = 1",
    )
    .bind(plan_id.to_string())
    .fetch_one(&store.pool)
    .await
    .expect("read projected schedule id");
    assert_eq!(sale_count, 1);
    assert_eq!(line_count, 1);
    assert_eq!(schedule_count, 2);
    assert_eq!(
        schedule_id,
        sel_contracts::solar_schedule_id(plan_id, 1).to_string()
    );
    assert_eq!(store.central_cursor().await.unwrap(), 3);
    let _ = std::fs::remove_file(path);
}
