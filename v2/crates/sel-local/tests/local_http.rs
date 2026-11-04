use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_contracts::{
    catalog_snapshot_sha256, CatalogSnapshot, CatalogSnapshotPayload, SyncEnvelope,
};
use sel_local::{refresh_catalog_once, router_with_store, router_with_store_and_auth};
use sel_storage::LocalStore;
use sel_sync::payload_digest;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn local_solar_catalog_exposes_only_current_approved_products() {
    let path = std::env::temp_dir().join(format!("sel-v2-solar-catalog-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    for (sku, status, active, product_type, start, end) in [
        ("KIT-ACTIVE", "APPROVED", 1, "SOLAR_KIT", "2026-01-01", None),
        ("KIT-DRAFT", "DRAFT", 1, "SOLAR_KIT", "2026-01-01", None),
        ("KIT-REVOKED", "REVOKED", 0, "SOLAR_KIT", "2026-01-01", None),
        ("NOT-A-KIT", "APPROVED", 1, "OTHER", "2026-01-01", None),
        ("KIT-FUTURE", "APPROVED", 1, "SOLAR_KIT", "2099-01-01", None),
        (
            "KIT-EXPIRED",
            "APPROVED",
            1,
            "SOLAR_KIT",
            "2020-01-01",
            Some("2020-12-31"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO products
             (id, sku, product_type, label, price_reference, currency, active,
              effective_from, effective_to, status, approval_ref)
             VALUES ($1,$2,$3,$4,'120.00','USD',$5,$6,$7,$8,'SEL-APPROVED')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(sku)
        .bind(product_type)
        .bind(format!("Product {sku}"))
        .bind(active)
        .bind(start)
        .bind(end)
        .bind(status)
        .execute(&store.pool)
        .await
        .unwrap();
    }

    let app = router_with_store(store.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/solar/products")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let products = json_body(response).await;
    assert_eq!(products.as_array().unwrap().len(), 1);
    assert_eq!(products[0]["sku"], "KIT-ACTIVE");
    assert_eq!(products[0]["price_reference"], "120.00");
    assert_eq!(products[0]["approval_ref"], "SEL-APPROVED");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_status_reads_durable_outbox_counts() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-http-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let payload = json!({"customer_code":"HTTP-LOCAL"});
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id: store.device_id,
        organization_id: store.organization_id,
        actor_id: Uuid::new_v4(),
        command_type: "CUSTOMER_CREATED".into(),
        aggregate_id: Uuid::new_v4(),
        aggregate_expected_version: 0,
        occurred_at: chrono::Utc::now(),
        offline_grant_id: Uuid::new_v4(),
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    store.record_command(&event).await.unwrap();
    let app = router_with_store(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["status"], "LOCAL_PENDING");
    assert_eq!(body["pending"], 1);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_sync_issues_expose_conflicts_without_leaking_payloads() {
    let path = std::env::temp_dir().join(format!("sel-v2-sync-issues-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sync_conflicts (id,event_id,reason,central_revision,payload_json,created_at)
         VALUES ($1,$2,'REVISION_MISMATCH',7,'{\"sensitive\":\"must-not-be-returned\"}',datetime('now'))",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(event_id.to_string())
    .execute(&store.pool)
    .await
    .unwrap();

    let app = router_with_store(store.clone());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/issues")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let issues = json_body(response).await;
    assert_eq!(issues["conflicts"], 1);
    assert_eq!(issues["items"][0]["event_id"], event_id.to_string());
    assert_eq!(issues["items"][0]["reason"], "REVISION_MISMATCH");
    assert_eq!(issues["items"][0]["central_revision"], 7);
    assert!(issues["items"][0].get("payload").is_none());
    assert!(issues["items"][0].get("payload_json").is_none());

    let status = app
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(json_body(status).await["conflicts"], 1);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_production_router_requires_the_configured_bearer_token() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-auth-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let app = router_with_store_and_auth(store, "local-test-token-0123456789");

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), axum::http::StatusCode::UNAUTHORIZED);

    let invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), axum::http::StatusCode::UNAUTHORIZED);

    let accepted = app
        .oneshot(
            Request::builder()
                .uri("/local/v1/sync/status")
                .header("authorization", "Bearer local-test-token-0123456789")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), axum::http::StatusCode::OK);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_customer_search_filters_sqlite_by_name_and_code() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-local-customer-search-{}.sqlite",
        Uuid::new_v4()
    ));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let app = router_with_store(store.clone());
    for (code, name) in [
        ("SEL-C-101", "Clarisse Bomatsa"),
        ("SEL-C-102", "Amani Kizel"),
        ("SEL-C-103", "Élodie Mbuyi"),
    ] {
        let created = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/local/v1/customers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({"customer_code":code,"legal_name":name}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    }

    for (term, expected_code) in [
        ("clarisse", "SEL-C-101"),
        ("c-101", "SEL-C-101"),
        ("élodie", "SEL-C-103"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/local/v1/customers?q={term}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["customer_code"], expected_code);
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_meter_readings_are_durable_validated_and_corrected_append_only() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-local-meter-readings-{}.sqlite",
        Uuid::new_v4()
    ));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let store = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .unwrap();
    let app = router_with_store(store.clone());

    let created_customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/customers")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"customer_code":"METER-001","legal_name":"Client Compteur"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created_customer.status(), axum::http::StatusCode::CREATED);
    let customer = json_body(created_customer).await;
    let customer_id = customer["id"].as_str().unwrap();

    let created_account = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/customers/{customer_id}/service-accounts"
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "service_code":"WATER",
                        "contract_ref":"WATER-CONTRACT-001",
                        "connection_status":"CONNECTED",
                        "activated_at":"2026-01-01"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created_account.status(), axum::http::StatusCode::CREATED);
    let account = json_body(created_account).await;
    let account_id = account["id"].as_str().unwrap();

    let created_meter = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/local/v1/service-accounts/{account_id}/meters"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"serial":"W-M-001","unit":"M3"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created_meter.status(), axum::http::StatusCode::CREATED);
    let meter = json_body(created_meter).await;
    let meter_id = meter["id"].as_str().unwrap();

    let preview = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/local/v1/meters/{meter_id}/readings/preview"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "period":"2026-10",
                        "previous":"0",
                        "current":"1.25",
                        "source":"MANUAL"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), axum::http::StatusCode::OK);
    let preview = json_body(preview).await;
    assert_eq!(preview["measured_quantity"], "1.25");
    assert_eq!(preview["meter_id"], meter_id);
    let readings_before_save = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/local/v1/meters/{meter_id}/readings"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(json_body(readings_before_save)
        .await
        .as_array()
        .unwrap()
        .is_empty());

    let zero_reading = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/local/v1/meters/{meter_id}/readings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "period":"2026-10",
                        "previous":"0",
                        "current":"0",
                        "source":"MANUAL"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(zero_reading.status(), axum::http::StatusCode::CREATED);
    let zero = json_body(zero_reading).await;
    assert_eq!(zero["measured_quantity"], "0");
    let reading_id = zero["id"].as_str().unwrap();

    let correction = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/local/v1/meter-readings/{reading_id}/corrections"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "current":"1.25",
                        "source":"MANUAL",
                        "reason":"Index relu sur le compteur"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(correction.status(), axum::http::StatusCode::CREATED);
    let corrected = json_body(correction).await;
    assert_eq!(corrected["previous"], "0");
    assert_eq!(corrected["current"], "1.25");
    assert_eq!(corrected["measured_quantity"], "1.25");
    assert_eq!(corrected["corrected_from_id"], reading_id);

    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/local/v1/meters/{meter_id}/readings?period=2026-10"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), axum::http::StatusCode::OK);
    let history = json_body(history).await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["id"], reading_id);
    assert_eq!(history[1]["id"], corrected["id"]);

    let queued_meter_commands: Vec<String> = sqlx::query_scalar(
        "SELECT command_type FROM command_journal
         WHERE command_type IN ('SERVICE_ACCOUNT_CREATED', 'METER_CREATED',
                                'METER_READING_CREATED', 'METER_READING_CORRECTED')
         ORDER BY created_at, rowid",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        queued_meter_commands,
        [
            "SERVICE_ACCOUNT_CREATED",
            "METER_CREATED",
            "METER_READING_CREATED",
            "METER_READING_CORRECTED"
        ]
    );
    let queued_outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox o
         JOIN command_journal j ON j.event_id = o.event_id
         WHERE j.command_type IN ('SERVICE_ACCOUNT_CREATED', 'METER_CREATED',
                                  'METER_READING_CREATED', 'METER_READING_CORRECTED')",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(queued_outbox_count, 4);

    let duplicate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/local/v1/meters/{meter_id}/readings"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "period":"2026-10",
                        "previous":"0",
                        "current":"2",
                        "source":"MANUAL"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(duplicate.status(), axum::http::StatusCode::CONFLICT);

    drop(app);
    drop(store);
    let reopened = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .unwrap();
    let persisted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meter_readings")
        .fetch_one(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(persisted, 2);
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn catalog_refresh_fetches_and_persists_verified_snapshot_once() {
    let path =
        std::env::temp_dir().join(format!("sel-v2-catalog-refresh-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("open local store");
    let payload = CatalogSnapshotPayload {
        schema_version: 1,
        products: vec![],
        tariffs: vec![],
        tax_profiles: vec![],
        fx_rates: vec![],
    };
    let snapshot = CatalogSnapshot {
        sha256: catalog_snapshot_sha256(&payload),
        payload,
    };
    let app = axum::Router::new().route(
        "/v1/catalog/snapshot",
        axum::routing::get({
            let snapshot = snapshot.clone();
            move || {
                let snapshot = snapshot.clone();
                async move { axum::Json(snapshot) }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let endpoint = format!("http://{address}");
    assert!(refresh_catalog_once(&store, &endpoint, "test-token")
        .await
        .unwrap());
    assert!(!refresh_catalog_once(&store, &endpoint, "test-token")
        .await
        .unwrap());
    assert_eq!(
        store.catalog_snapshot_hash().await.unwrap(),
        Some(snapshot.sha256)
    );
    server.abort();
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn utility_preview_returns_both_services_without_persisting_financial_data() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-preview-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let store = LocalStore::connect(&database_url, organization_id, Uuid::new_v4())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.095', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', '1.30', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    let (customer, _) = store
        .create_customer(&sel_contracts::CreateCustomer {
            customer_code: "PREVIEW-001".into(),
            legal_name: "Preview customer".into(),
            address: None,
            phone: None,
        })
        .await
        .unwrap();
    let before = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT
           (SELECT count(*) FROM invoices),
           (SELECT count(*) FROM receivables),
           (SELECT count(*) FROM outbox)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();

    let response = router_with_store(store.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/billing/utility/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer.id,
                        "billing_period": "2026-11",
                        "electricity": {"quantity": "100", "unit": "KWH"},
                        "water": {"quantity": "0", "unit": "M3"},
                        "water_zero_confirmed": true,
                        "currency": "USD",
                        "tariff_revision": "local-approved-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "LOCAL_PREVIEW");
    assert_eq!(body["receivable_id"], serde_json::Value::Null);
    assert_eq!(body["lines"].as_array().unwrap().len(), 2);
    assert!(body["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| { line["service_code"] == "WATER" && line["quantity"] == "0" }));

    let after = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT
           (SELECT count(*) FROM invoices),
           (SELECT count(*) FROM receivables),
           (SELECT count(*) FROM outbox)",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(after, before);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn utility_invoice_uses_and_binds_the_selected_meter_readings() {
    let path = std::env::temp_dir().join(format!("sel-v2-meter-billing-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let app = router_with_store(store.clone());
    sqlx::query(
        "INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.095', 'USD', '2026-01-01', 'meter-billing-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', '1.30', 'USD', '2026-01-01', 'meter-billing-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();

    let customer = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/local/v1/customers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"customer_code":"READING-001","legal_name":"Client Relevé"})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    let customer_id = customer["id"].as_str().unwrap();

    async fn create_reading(
        app: &axum::Router,
        customer_id: &str,
        service_code: &str,
        serial: &str,
        unit: &str,
        previous: &str,
        current: &str,
    ) -> String {
        let account = json_body(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!(
                            "/local/v1/customers/{customer_id}/service-accounts"
                        ))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            json!({
                                "service_code":service_code,
                                "contract_ref":format!("{serial}-CONTRACT"),
                                "connection_status":"CONNECTED",
                                "activated_at":"2026-01-01"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        let meter = json_body(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!(
                            "/local/v1/service-accounts/{}/meters",
                            account["id"].as_str().unwrap()
                        ))
                        .header("content-type", "application/json")
                        .body(Body::from(json!({"serial":serial,"unit":unit}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        let reading = json_body(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!(
                            "/local/v1/meters/{}/readings",
                            meter["id"].as_str().unwrap()
                        ))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            json!({
                                "period":"2026-11",
                                "previous":previous,
                                "current":current,
                                "source":"MANUAL"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;
        reading["id"].as_str().unwrap().to_owned()
    }

    let electricity_reading = create_reading(
        &app,
        customer_id,
        "ELECTRICITY",
        "E-METER-001",
        "KWH",
        "100",
        "150",
    )
    .await;
    let water_reading = create_reading(
        &app,
        customer_id,
        "WATER",
        "W-METER-001",
        "M3",
        "20",
        "22.5",
    )
    .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/billing/utility/issue")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "customer_id":customer_id,
                        "billing_period":"2026-11",
                        "electricity":{"quantity":"999","unit":"KWH"},
                        "water":{"quantity":"999","unit":"M3"},
                        "meter_readings":{
                            "electricity_reading_id":electricity_reading,
                            "water_reading_id":water_reading
                        },
                        "water_zero_confirmed":false,
                        "currency":"USD",
                        "tariff_revision":"meter-billing-v1",
                        "due_date":"2026-12-15"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let invoice = json_body(response).await;
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 2);
    assert_eq!(invoice["billing_period"], "2026-11");
    assert_eq!(invoice["due_date"], "2026-12-15");
    assert!(invoice["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| { line["service_code"] == "ELECTRICITY" && line["quantity"] == "50" }));
    assert!(invoice["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|line| { line["service_code"] == "WATER" && line["quantity"] == "2.5" }));

    let invoice_id = invoice["invoice_id"].as_str().unwrap();
    let document = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/local/v1/invoices/{invoice_id}/document-data"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(document["document_schema_version"], 1);
    assert_eq!(document["template_version"], "v2-a4-1");
    assert_eq!(document["origin"], "LOCAL_PENDING");
    assert_eq!(document["invoice"]["billing_period"], "2026-11");
    assert_eq!(document["invoice"]["due_date"], "2026-12-15");
    assert_eq!(
        document["invoice"]["snapshot_sha256"],
        invoice["snapshot_sha256"]
    );
    assert_eq!(
        document["qr_internal_reference"],
        format!("SEL-V2-{invoice_id}")
    );

    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoice_meter_readings")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(bindings, 2);
    let correction = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/meter-readings/{water_reading}/corrections"
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "current":"23",
                        "source":"MANUAL",
                        "reason":"Tentative après émission"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(correction.status(), axum::http::StatusCode::CONFLICT);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn service_account_contract_can_be_updated_with_revision_control_offline() {
    let path = std::env::temp_dir().join(format!(
        "sel-v2-service-account-update-{}.sqlite",
        Uuid::new_v4()
    ));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let app = router_with_store(store.clone());

    let customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/customers")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"customer_code":"SERVICE-UPDATE-1","legal_name":"Test Client"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), axum::http::StatusCode::CREATED);
    let customer = json_body(customer).await;
    let account = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/customers/{}/service-accounts",
                    customer["id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "service_code":"ELECTRICITY",
                        "contract_ref":"CONTRACT-OLD",
                        "connection_status":"CONNECTED",
                        "activated_at":"2026-01-01"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(account.status(), axum::http::StatusCode::CREATED);
    let account = json_body(account).await;
    let account_id = account["id"].as_str().unwrap();

    let update = || {
        Request::builder()
            .method("PATCH")
            .uri(format!("/local/v1/service-accounts/{account_id}"))
            .header("if-match", "\"1\"")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "contract_ref":"CONTRACT-NEW",
                    "connection_status":"NOT_CONNECTED"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let updated = app.clone().oneshot(update()).await.unwrap();
    assert_eq!(updated.status(), axum::http::StatusCode::OK);
    let updated = json_body(updated).await;
    assert_eq!(updated["contract_ref"], "CONTRACT-NEW");
    assert_eq!(updated["connection_status"], "NOT_CONNECTED");
    assert_eq!(updated["revision"], 2);

    let stale = app.clone().oneshot(update()).await.unwrap();
    assert_eq!(stale.status(), axum::http::StatusCode::CONFLICT);
    let empty_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/local/v1/service-accounts/{account_id}"))
                .header("if-match", "\"2\"")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        empty_patch.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let immutable_field_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/local/v1/service-accounts/{account_id}"))
                .header("if-match", "\"2\"")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"customer_id":"f8f2c2ca-8cfe-4b11-bb3b-e044baec6b92"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        immutable_field_patch.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let listed = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/local/v1/customers/{}/service-accounts",
                    customer["id"].as_str().unwrap()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), axum::http::StatusCode::OK);
    let listed = json_body(listed).await;
    assert_eq!(listed[0]["contract_ref"], "CONTRACT-NEW");
    assert_eq!(listed[0]["revision"], 2);

    let pending_update_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outbox o JOIN command_journal j ON j.event_id=o.event_id
         WHERE j.command_type='SERVICE_ACCOUNT_UPDATED'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(pending_update_count, 1);
    let _ = std::fs::remove_file(path);
}
