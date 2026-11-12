use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use sel_api::{router, AppState};
use sel_contracts::{catalog_snapshot_sha256, CatalogSnapshotPayload};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::{json, Value};
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn catalog_routes_expose_only_active_and_approved_reference_data() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL catalog API fixture")
        .await
        .expect("create fixture organization");

    let active_product = Uuid::new_v4();
    let inactive_product = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO products
         (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, $2, 'SOLAR_KIT', 'Kit fixture', 100.00, 'USD', TRUE),
                ($3, $4, 'SOLAR_KIT', 'Inactive fixture', 200.00, 'USD', FALSE)",
    )
    .bind(active_product)
    .bind(format!("CAT-A-{}", Uuid::new_v4()))
    .bind(inactive_product)
    .bind(format!("CAT-I-{}", Uuid::new_v4()))
    .execute(&store.pool)
    .await
    .expect("insert product fixtures");

    let approved_tariff = Uuid::new_v4();
    let draft_tariff = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency,
          effective_from, approval_ref, status)
         VALUES ($1, 'WATER', 'RESIDENTIAL', 'M3', 0.30, 'USD', '2026-01-01', $2, 'APPROVED'),
                ($3, 'WATER', 'RESIDENTIAL', 'M3', 9.99, 'USD', '2026-01-01', $4, 'DRAFT')",
    )
    .bind(approved_tariff)
    .bind(format!("APPROVED-{}", Uuid::new_v4()))
    .bind(draft_tariff)
    .bind(format!("DRAFT-{}", Uuid::new_v4()))
    .execute(&store.pool)
    .await
    .expect("insert tariff fixtures");

    let approved_tax = Uuid::new_v4();
    let revoked_tax = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tax_profile_versions
         (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'WATER', 'SEL', 'ENERGY', 0.16, 'fixture', '2026-01-01', 'APPROVED'),
                ($2, 'WATER', 'SEL', 'ENERGY', 0.99, 'fixture', '2026-01-01', 'REVOKED')",
    )
    .bind(approved_tax)
    .bind(revoked_tax)
    .execute(&store.pool)
    .await
    .expect("insert tax fixtures");

    let approved_fx = Uuid::new_v4();
    let unapproved_fx = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO fx_rate_versions
         (id, from_currency, to_currency, value, effective_from, source, approved, status)
         VALUES ($1, 'USD', 'CDF', 2360, '2026-01-01', 'fixture', TRUE, 'APPROVED'),
                ($2, 'USD', 'CDF', 9999, '2026-01-01', 'fixture', FALSE, 'DRAFT')",
    )
    .bind(approved_fx)
    .bind(unapproved_fx)
    .execute(&store.pool)
    .await
    .expect("insert FX fixtures");

    sqlx::query(
        "INSERT INTO catalog_reference_versions
         (id, reference_type, natural_key, payload, effective_from, status, approval_ref)
         VALUES
         ($1, 'TAX_PROFILE', 'WATER|SEL|ENERGY',
          jsonb_build_object('service_code','WATER','jurisdiction','SEL','base','ENERGY','rate','0.16','source','fixture'),
          '2026-01-01', 'APPROVED', 'SEL-TAX-FIXTURE'),
         ($2, 'TAX_PROFILE', 'WATER|SEL|ENERGY',
          jsonb_build_object('service_code','WATER','jurisdiction','SEL','base','ENERGY','rate','0.99','source','fixture'),
          '2026-01-01', 'REVOKED', NULL),
         ($3, 'FX_RATE', 'USD|CDF',
          jsonb_build_object('from_currency','USD','to_currency','CDF','value','2360','source','fixture'),
          '2026-01-01', 'APPROVED', 'SEL-FX-FIXTURE'),
         ($4, 'FX_RATE', 'USD|CDF',
          jsonb_build_object('from_currency','USD','to_currency','CDF','value','9999','source','fixture'),
          '2026-01-01', 'DRAFT', NULL)",
    )
    .bind(approved_tax)
    .bind(revoked_tax)
    .bind(approved_fx)
    .bind(unapproved_fx)
    .execute(&store.pool)
    .await
    .expect("insert versioned reference fixtures");

    let app = router(AppState::with_central_store(store));
    let products = get_json(&app, "/v1/catalog/products").await;
    assert_eq!(products.as_array().unwrap().len(), 1);
    assert_eq!(products[0]["id"], active_product.to_string());
    assert_eq!(products[0]["price_reference"], "100");

    let tariffs = get_json(&app, "/v1/catalog/tariffs").await;
    assert_eq!(tariffs.as_array().unwrap().len(), 1);
    assert_eq!(tariffs[0]["id"], approved_tariff.to_string());
    assert_eq!(
        tariffs[0]["unit_price"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::new(3, 1)
    );

    let taxes = get_json(&app, "/v1/catalog/tax-profiles").await;
    assert_eq!(taxes.as_array().unwrap().len(), 1);
    assert_eq!(taxes[0]["id"], approved_tax.to_string());
    assert_eq!(taxes[0]["rate"], "0.16");

    let rates = get_json(&app, "/v1/catalog/fx-rates").await;
    assert_eq!(rates.as_array().unwrap().len(), 1);
    assert_eq!(rates[0]["id"], approved_fx.to_string());
    assert_eq!(rates[0]["value"], "2360");

    let snapshot = get_json_authenticated(&app, "/v1/catalog/snapshot", "catalog-fixture").await;
    let payload: CatalogSnapshotPayload = serde_json::from_value(snapshot["payload"].clone())
        .expect("snapshot payload matches its versioned contract");
    assert_eq!(snapshot["sha256"], catalog_snapshot_sha256(&payload));
    assert_eq!(
        payload.products.len(),
        2,
        "snapshot includes revocation tombstones"
    );
    assert_eq!(payload.tariffs.len(), 1);
    assert_eq!(
        payload.tax_profiles.len(),
        2,
        "snapshot includes historical revoked profiles"
    );
    assert_eq!(payload.fx_rates.len(), 1);
    let communication_charges =
        get_json_authenticated(&app, "/v1/catalog/communication-charge", "catalog-fixture").await;
    assert!(communication_charges
        .as_array()
        .unwrap()
        .iter()
        .any(|charge| charge["currency"] == "USD" && charge["amount"] == "10"));
    assert!(!payload.communication_charges.is_empty());
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn tariff_drafts_require_admin_approval_reference_and_non_overlapping_periods() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL tariff administration fixture")
        .await
        .expect("create fixture organization");
    let admin_id = store
        .bootstrap_initial_admin("admin@tariff.test", "FixturePass!8")
        .await
        .expect("bootstrap fixture administrator");
    let app = router(AppState::with_production_central_store(store.clone()));
    let login = post_json(
        &app,
        "/v1/auth/login",
        None,
        None,
        json!({
            "identity_ref": "admin@tariff.test",
            "password": "FixturePass!8"
        }),
    )
    .await;
    assert_eq!(login.status(), axum::http::StatusCode::OK);
    let login_body = json_body(login).await;
    assert_eq!(login_body["user_id"], admin_id.to_string());
    let access_token = login_body["access_token"].as_str().unwrap().to_owned();

    let customer_category = format!("TEST-{}", Uuid::new_v4());
    let draft_request = json!({
        "service_code": "WATER",
        "customer_category": customer_category.clone(),
        "unit": "M3",
        "unit_price": "0.30",
        "currency": "USD",
        "effective_from": "2026-11-04",
        "effective_to": null
    });
    let create = post_json(
        &app,
        "/v1/admin/catalog/tariffs",
        Some(&access_token),
        Some("tariff-draft-001"),
        draft_request.clone(),
    )
    .await;
    assert_eq!(create.status(), axum::http::StatusCode::CREATED);
    let draft = json_body(create).await;
    assert_eq!(draft["status"], "DRAFT");
    assert!(draft["approval_ref"].is_null());
    let tariff_id = draft["id"].as_str().unwrap().to_owned();

    let replay = post_json(
        &app,
        "/v1/admin/catalog/tariffs",
        Some(&access_token),
        Some("tariff-draft-001"),
        draft_request,
    )
    .await;
    assert_eq!(replay.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(replay).await["id"], tariff_id);

    let concurrent_request = json!({
        "service_code": "ELECTRICITY",
        "customer_category": format!("IDEMPOTENCY-{}", Uuid::new_v4()),
        "unit": "KWH",
        "unit_price": "0.000001",
        "currency": "USD",
        "effective_from": "2026-11-04",
        "effective_to": null
    });
    let (left, right) = tokio::join!(
        post_json(
            &app,
            "/v1/admin/catalog/tariffs",
            Some(&access_token),
            Some("tariff-create-concurrent-001"),
            concurrent_request.clone()
        ),
        post_json(
            &app,
            "/v1/admin/catalog/tariffs",
            Some(&access_token),
            Some("tariff-create-concurrent-001"),
            concurrent_request
        )
    );
    let statuses = [left.status(), right.status()];
    assert!(statuses.contains(&axum::http::StatusCode::CREATED));
    assert!(statuses.contains(&axum::http::StatusCode::OK));
    let left_id = json_body(left).await["id"].as_str().unwrap().to_owned();
    let right_id = json_body(right).await["id"].as_str().unwrap().to_owned();
    assert_eq!(left_id, right_id);

    let catalog_before_approval =
        get_json_authenticated(&app, "/v1/catalog/tariffs", &access_token).await;
    assert!(!catalog_before_approval
        .as_array()
        .unwrap()
        .iter()
        .any(|tariff| tariff["id"] == tariff_id));

    let approve_path = format!("/v1/admin/catalog/tariffs/{tariff_id}/approve");
    let approve = post_json(
        &app,
        &approve_path,
        Some(&access_token),
        Some("tariff-approve-001"),
        json!({"approval_ref": "SEL-APPROVAL-FIXTURE-1"}),
    )
    .await;
    assert_eq!(approve.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(approve).await["status"], "APPROVED");
    let catalog = get_json_authenticated(&app, "/v1/catalog/tariffs", &access_token).await;
    let approved = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|tariff| tariff["id"] == tariff_id)
        .expect("approved tariff is published to the catalog");
    assert_eq!(approved["approval_ref"], "SEL-APPROVAL-FIXTURE-1");

    let conflicting = post_json(
        &app,
        "/v1/admin/catalog/tariffs",
        Some(&access_token),
        Some("tariff-draft-002"),
        json!({
            "service_code": "WATER",
            "customer_category": customer_category,
            "unit": "M3",
            "unit_price": "9.99",
            "currency": "USD",
            "effective_from": chrono::Utc::now().date_naive().to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(conflicting.status(), axum::http::StatusCode::CREATED);
    let conflicting_id = json_body(conflicting).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let conflict = post_json(
        &app,
        &format!("/v1/admin/catalog/tariffs/{conflicting_id}/approve"),
        Some(&access_token),
        Some("tariff-approve-002"),
        json!({"approval_ref": "SEL-APPROVAL-FIXTURE-2"}),
    )
    .await;
    assert_eq!(conflict.status(), axum::http::StatusCode::CONFLICT);
    let error = json_body(conflict).await;
    assert_eq!(error["code"], "TARIFF_PERIOD_OVERLAP");

    let persisted =
        sqlx::query("SELECT status, approved_by, approval_ref FROM tariff_versions WHERE id = $1")
            .bind(Uuid::parse_str(&tariff_id).unwrap())
            .fetch_one(&store.pool)
            .await
            .expect("read approved tariff");
    assert_eq!(
        persisted.try_get::<String, _>("status").unwrap(),
        "APPROVED"
    );
    assert_eq!(
        persisted.try_get::<Uuid, _>("approved_by").unwrap(),
        admin_id
    );
    assert_eq!(
        persisted
            .try_get::<Option<String>, _>("approval_ref")
            .unwrap(),
        Some("SEL-APPROVAL-FIXTURE-1".into())
    );
    let draft_status =
        sqlx::query_scalar::<_, String>("SELECT status FROM tariff_versions WHERE id = $1")
            .bind(Uuid::parse_str(&conflicting_id).unwrap())
            .fetch_one(&store.pool)
            .await
            .expect("read overlapping draft");
    assert_eq!(draft_status, "DRAFT");

    let independent_draft = post_json(
        &app,
        "/v1/admin/catalog/tariffs",
        Some(&access_token),
        Some("tariff-draft-003"),
        json!({
            "service_code": "ELECTRICITY",
            "customer_category": format!("TEST-{}", Uuid::new_v4()),
            "unit": "KWH",
            "unit_price": "0.000001",
            "currency": "USD",
            "effective_from": chrono::Utc::now().date_naive().to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(independent_draft.status(), axum::http::StatusCode::CREATED);
    let independent_id = json_body(independent_draft).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let reused_key_for_another_resource = post_json(
        &app,
        &format!("/v1/admin/catalog/tariffs/{independent_id}/approve"),
        Some(&access_token),
        Some("tariff-approve-001"),
        json!({"approval_ref": "SEL-APPROVAL-FIXTURE-1"}),
    )
    .await;
    assert_eq!(
        reused_key_for_another_resource.status(),
        axum::http::StatusCode::CONFLICT
    );
    assert_eq!(
        json_body(reused_key_for_another_resource).await["code"],
        "IDEMPOTENCY_PAYLOAD_MISMATCH"
    );

    let concurrent_category = format!("OVERLAP-{}", Uuid::new_v4()).to_ascii_uppercase();
    let mut concurrent_tariff_ids = Vec::new();
    for (key, price) in [
        ("tariff-overlap-draft-001", "0.000001"),
        ("tariff-overlap-draft-002", "0.000002"),
    ] {
        let response = post_json(
            &app,
            "/v1/admin/catalog/tariffs",
            Some(&access_token),
            Some(key),
            json!({
                "service_code": "ELECTRICITY",
                "customer_category": concurrent_category.clone(),
                "unit": "KWH",
                "unit_price": price,
                "currency": "USD",
                "effective_from": "2026-11-04",
                "effective_to": null
            }),
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        concurrent_tariff_ids.push(json_body(response).await["id"].as_str().unwrap().to_owned());
    }
    let approval_path_one = format!(
        "/v1/admin/catalog/tariffs/{}/approve",
        concurrent_tariff_ids[0]
    );
    let approval_path_two = format!(
        "/v1/admin/catalog/tariffs/{}/approve",
        concurrent_tariff_ids[1]
    );
    let (approval_one, approval_two) = tokio::join!(
        post_json(
            &app,
            &approval_path_one,
            Some(&access_token),
            Some("tariff-overlap-approve-001"),
            json!({"approval_ref": "SEL-OVERLAP-FIXTURE-1"})
        ),
        post_json(
            &app,
            &approval_path_two,
            Some(&access_token),
            Some("tariff-overlap-approve-002"),
            json!({"approval_ref": "SEL-OVERLAP-FIXTURE-2"})
        )
    );
    let approval_statuses = [approval_one.status(), approval_two.status()];
    assert!(approval_statuses.contains(&axum::http::StatusCode::OK));
    assert!(approval_statuses.contains(&axum::http::StatusCode::CONFLICT));
    let approved_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM tariff_versions
         WHERE customer_category = $1 AND status = 'APPROVED'",
    )
    .bind(&concurrent_category)
    .fetch_one(&store.pool)
    .await
    .expect("count concurrently approved tariffs");
    assert_eq!(approved_count, 1);

    let communication_effective_from =
        (chrono::Utc::now().date_naive() + chrono::Duration::days(2)).to_string();
    let communication_draft = post_json(
        &app,
        "/v1/admin/catalog/communication-charge",
        Some(&access_token),
        Some("communication-charge-draft-001"),
        json!({
            "amount": "12.50",
            "currency": "USD",
            "effective_from": communication_effective_from,
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(
        communication_draft.status(),
        axum::http::StatusCode::CREATED
    );
    let communication_draft = json_body(communication_draft).await;
    assert_eq!(communication_draft["status"], "DRAFT");
    let communication_id = communication_draft["id"].as_str().unwrap();
    let communication_approval = post_json(
        &app,
        &format!("/v1/admin/catalog/communication-charge/{communication_id}/approve"),
        Some(&access_token),
        Some("communication-charge-approve-001"),
        json!({
            "approval_ref": "SEL-COMMUNICATION-APPROVAL-1",
            "supersedes_id": "21c00100-0000-4000-8000-000000000001"
        }),
    )
    .await;
    assert_eq!(communication_approval.status(), axum::http::StatusCode::OK);
    assert_eq!(
        json_body(communication_approval).await["status"],
        "APPROVED"
    );
    let communication_versions = get_json_authenticated(
        &app,
        "/v1/admin/catalog/communication-charge",
        &access_token,
    )
    .await;
    assert!(communication_versions
        .as_array()
        .unwrap()
        .iter()
        .any(|version| {
            version["id"] == communication_id
                && version["payload"]["amount"]
                    .as_str()
                    .and_then(|amount| amount.parse::<Decimal>().ok())
                    == Some(Decimal::new(125, 1))
        }),
        "approved communication version should be visible in the admin catalog: {communication_versions:#}"
    );
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn tariff_versions_can_be_superseded_and_revoked_without_deleting_history() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL tariff lifecycle fixture")
        .await
        .expect("create fixture organization");
    store
        .bootstrap_initial_admin("admin@tariff-lifecycle.test", "FixturePass!8")
        .await
        .expect("bootstrap fixture administrator");
    let app = router(AppState::with_production_central_store(store.clone()));
    let login = post_json(
        &app,
        "/v1/auth/login",
        None,
        None,
        json!({
            "identity_ref": "admin@tariff-lifecycle.test",
            "password": "FixturePass!8"
        }),
    )
    .await;
    assert_eq!(login.status(), axum::http::StatusCode::OK);
    let token = json_body(login).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();

    let category = format!("LIFECYCLE-{}", Uuid::new_v4()).to_ascii_uppercase();
    let previous_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency,
          effective_from, effective_to, approval_ref, status)
         VALUES ($1, 'WATER', $2, 'M3', 1.10, 'USD', '2025-01-01', NULL,
                 'SEL-OLD-RATE', 'APPROVED')",
    )
    .bind(previous_id)
    .bind(&category)
    .execute(&store.pool)
    .await
    .expect("insert preceding approved tariff");

    let effective_from = chrono::Utc::now().date_naive();
    let draft = post_json(
        &app,
        "/v1/admin/catalog/tariffs",
        Some(&token),
        Some("lifecycle-create-1"),
        json!({
            "service_code": "WATER",
            "customer_category": category,
            "unit": "M3",
            "unit_price": "0.30",
            "currency": "USD",
            "effective_from": effective_from.to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(draft.status(), axum::http::StatusCode::CREATED);
    let draft_id = json_body(draft).await["id"].as_str().unwrap().to_owned();
    let approve_path = format!("/v1/admin/catalog/tariffs/{draft_id}/approve");
    let approve = post_json(
        &app,
        &approve_path,
        Some(&token),
        Some("lifecycle-approve-1"),
        json!({
            "approval_ref": "SEL-NEW-RATE-APPROVAL",
            "supersedes_tariff_id": previous_id
        }),
    )
    .await;
    if approve.status() != axum::http::StatusCode::OK {
        panic!("tariff approval failed: {}", json_body(approve).await);
    }
    assert_eq!(json_body(approve).await["status"], "APPROVED");

    let prior_end = sqlx::query_scalar::<_, Option<chrono::NaiveDate>>(
        "SELECT effective_to FROM tariff_versions WHERE id = $1",
    )
    .bind(previous_id)
    .fetch_one(&store.pool)
    .await
    .expect("read superseded tariff end date");
    assert_eq!(
        prior_end.unwrap(),
        effective_from - chrono::Duration::days(1)
    );

    let revoke_path = format!("/v1/admin/catalog/tariffs/{draft_id}/revoke");
    let revoke_payload = json!({"change_ref": "SEL-RATE-WITHDRAWAL-1"});
    let revoke = post_json(
        &app,
        &revoke_path,
        Some(&token),
        Some("lifecycle-revoke-1"),
        revoke_payload.clone(),
    )
    .await;
    assert_eq!(revoke.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(revoke).await["status"], "REVOKED");
    let replay = post_json(
        &app,
        &revoke_path,
        Some(&token),
        Some("lifecycle-revoke-1"),
        revoke_payload,
    )
    .await;
    assert_eq!(replay.status(), axum::http::StatusCode::OK);

    let revoked_catalog = get_json_authenticated(&app, "/v1/catalog/tariffs", &token).await;
    assert!(!revoked_catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == draft_id));
    let admin_catalog = get_json_authenticated(&app, "/v1/admin/catalog/tariffs", &token).await;
    assert!(admin_catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == draft_id && row["status"] == "REVOKED"));
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_events
         WHERE subject_id = $1 AND event_type = 'TARIFF_VERSION_REVOKED'",
    )
    .bind(Uuid::parse_str(&draft_id).unwrap())
    .fetch_one(&store.pool)
    .await
    .expect("count revocation audit events");
    assert_eq!(audit_count, 1);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn solar_product_drafts_require_approval_and_keep_prior_price_until_effective_date() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL solar product catalog fixture")
        .await
        .expect("create fixture organization");
    store
        .bootstrap_initial_admin("admin@solar-catalog.test", "FixturePass!8")
        .await
        .expect("bootstrap fixture administrator");
    let app = router(AppState::with_production_central_store(store.clone()));
    let login = post_json(
        &app,
        "/v1/auth/login",
        None,
        None,
        json!({
            "identity_ref": "admin@solar-catalog.test",
            "password": "FixturePass!8"
        }),
    )
    .await;
    assert_eq!(login.status(), axum::http::StatusCode::OK);
    let token = json_body(login).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let sku = format!("KIT-{}", Uuid::new_v4()).to_ascii_uppercase();
    let today = chrono::Utc::now().date_naive();
    let draft = post_json(
        &app,
        "/v1/admin/catalog/products",
        Some(&token),
        Some("solar-product-create-01"),
        json!({
            "sku": sku,
            "label": "Solar kit fixture",
            "price_reference": "125.00",
            "currency": "USD",
            "effective_from": today.to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(draft.status(), axum::http::StatusCode::CREATED);
    let first = json_body(draft).await;
    assert_eq!(first["status"], "DRAFT");
    assert_eq!(first["active"], false);
    let first_id = first["id"].as_str().unwrap().to_owned();

    let before = get_json_authenticated(&app, "/v1/catalog/products", &token).await;
    assert!(!before
        .as_array()
        .unwrap()
        .iter()
        .any(|product| product["id"] == first_id));

    let approve = post_json(
        &app,
        &format!("/v1/admin/catalog/products/{first_id}/approve"),
        Some(&token),
        Some("solar-product-approve-01"),
        json!({"approval_ref": "SEL-SOLAR-CATALOG-APPROVAL-1"}),
    )
    .await;
    assert_eq!(approve.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(approve).await["status"], "APPROVED");

    let future_start = today + chrono::Duration::days(30);
    let second_draft = post_json(
        &app,
        "/v1/admin/catalog/products",
        Some(&token),
        Some("solar-product-create-02"),
        json!({
            "sku": first["sku"],
            "label": "Solar kit fixture revised",
            "price_reference": "140.00",
            "currency": "USD",
            "effective_from": future_start.to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(second_draft.status(), axum::http::StatusCode::CREATED);
    let second_id = json_body(second_draft).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second_approval = post_json(
        &app,
        &format!("/v1/admin/catalog/products/{second_id}/approve"),
        Some(&token),
        Some("solar-product-approve-02"),
        json!({
            "approval_ref": "SEL-SOLAR-CATALOG-APPROVAL-2",
            "supersedes_product_id": first_id
        }),
    )
    .await;
    assert_eq!(second_approval.status(), axum::http::StatusCode::OK);

    let prior = sqlx::query("SELECT active, effective_to, status FROM products WHERE id = $1")
        .bind(Uuid::parse_str(&first_id).unwrap())
        .fetch_one(&store.pool)
        .await
        .expect("read previous solar version");
    assert!(prior.try_get::<bool, _>("active").unwrap());
    assert_eq!(
        prior
            .try_get::<Option<chrono::NaiveDate>, _>("effective_to")
            .unwrap()
            .unwrap()
            .to_string(),
        (future_start - chrono::Duration::days(1)).to_string()
    );
    assert_eq!(prior.try_get::<String, _>("status").unwrap(), "APPROVED");
    let public_catalog = get_json_authenticated(&app, "/v1/catalog/products", &token).await;
    assert!(public_catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|product| product["id"] == first_id));
    assert!(!public_catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|product| product["id"] == second_id));

    let admin_catalog = get_json_authenticated(&app, "/v1/admin/catalog/products", &token).await;
    assert!(admin_catalog
        .as_array()
        .unwrap()
        .iter()
        .any(|product| product["id"] == second_id && product["status"] == "APPROVED"));
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn tax_and_fx_versions_require_approval_and_preserve_effective_history() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL reference administration fixture")
        .await
        .expect("create fixture organization");
    store
        .bootstrap_initial_admin("admin@reference.test", "FixturePass!8")
        .await
        .expect("bootstrap fixture administrator");
    let app = router(AppState::with_production_central_store(store.clone()));
    let login = post_json(
        &app,
        "/v1/auth/login",
        None,
        None,
        json!({
            "identity_ref": "admin@reference.test", "password": "FixturePass!8"
        }),
    )
    .await;
    assert_eq!(login.status(), axum::http::StatusCode::OK);
    let token = json_body(login).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let today = chrono::Utc::now().date_naive();
    let key_suffix = Uuid::new_v4().simple().to_string();
    let tax_key = format!("TAX-SEL-{key_suffix}");
    let tax_create_path = "/v1/admin/catalog/tax-profiles";
    let tax_request = json!({
        "service_code": "WATER", "jurisdiction": format!("SEL-{key_suffix}"),
        "base": "SERVICE", "rate": "0.16", "source": "SEL-approved schedule",
        "effective_from": today.to_string(), "effective_to": null
    });
    let tax_draft_response = post_json(
        &app,
        tax_create_path,
        Some(&token),
        Some(&tax_key),
        tax_request.clone(),
    )
    .await;
    assert_eq!(tax_draft_response.status(), axum::http::StatusCode::CREATED);
    let tax_draft = json_body(tax_draft_response).await;
    assert_eq!(tax_draft["status"], "DRAFT");
    let tax_id = tax_draft["id"].as_str().unwrap().to_owned();
    let replay = post_json(
        &app,
        tax_create_path,
        Some(&token),
        Some(&tax_key),
        tax_request,
    )
    .await;
    assert_eq!(replay.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(replay).await["id"], tax_id);
    let public_before = get_json_authenticated(&app, "/v1/catalog/tax-profiles", &token).await;
    assert!(!public_before
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == tax_id));

    let tax_approve_path = format!("/v1/admin/catalog/tax-profiles/{tax_id}/approve");
    let tax_approved = post_json(
        &app,
        &tax_approve_path,
        Some(&token),
        Some(&format!("approve-tax-{key_suffix}")),
        json!({
            "approval_ref": format!("SEL-TAX-APPROVAL-{key_suffix}"), "supersedes_id": null
        }),
    )
    .await;
    assert_eq!(tax_approved.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(tax_approved).await["status"], "APPROVED");
    assert!(
        get_json_authenticated(&app, "/v1/catalog/tax-profiles", &token)
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == tax_id)
    );

    let future = today + chrono::Duration::days(20);
    let fx_pair = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
    let alpha_pair = |value: &str| {
        value
            .chars()
            .map(|character| match character {
                '0'..='9' => char::from(b'A' + (character as u8 - b'0')),
                _ => character,
            })
            .collect::<String>()
    };
    let from_currency = format!("X{}", alpha_pair(&fx_pair[0..2]));
    let to_currency = format!("Y{}", alpha_pair(&fx_pair[2..4]));
    let fx_request = json!({
        "from_currency": from_currency, "to_currency": to_currency, "value": "2375.50",
        "source": "SEL approved exchange bulletin", "effective_from": today.to_string(),
        "effective_to": null
    });
    let fx_draft_response = post_json(
        &app,
        "/v1/admin/catalog/fx-rates",
        Some(&token),
        Some(&format!("fx-create-{key_suffix}")),
        fx_request,
    )
    .await;
    if fx_draft_response.status() != axum::http::StatusCode::CREATED {
        panic!(
            "FX draft creation failed: {}",
            json_body(fx_draft_response).await
        );
    }
    let fx_draft = json_body(fx_draft_response).await;
    let fx_id = fx_draft["id"].as_str().unwrap().to_owned();

    // The tax endpoint must reject an FX draft without changing it.
    let wrong_route = post_json(
        &app,
        &format!("/v1/admin/catalog/tax-profiles/{fx_id}/approve"),
        Some(&token),
        Some(&format!("wrong-type-{key_suffix}")),
        json!({
            "approval_ref": "SEL-TEST-ONLY", "supersedes_id": null
        }),
    )
    .await;
    assert_eq!(
        wrong_route.status(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    let still_draft: String =
        sqlx::query_scalar("SELECT status FROM catalog_reference_versions WHERE id = $1")
            .bind(Uuid::parse_str(&fx_id).unwrap())
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(still_draft, "DRAFT");

    let fx_approved = post_json(
        &app,
        &format!("/v1/admin/catalog/fx-rates/{fx_id}/approve"),
        Some(&token),
        Some(&format!("approve-fx-{key_suffix}")),
        json!({
            "approval_ref": format!("SEL-FX-APPROVAL-{key_suffix}"), "supersedes_id": null
        }),
    )
    .await;
    assert_eq!(fx_approved.status(), axum::http::StatusCode::OK);
    assert_eq!(json_body(fx_approved).await["status"], "APPROVED");

    let next_fx = post_json(
        &app,
        "/v1/admin/catalog/fx-rates",
        Some(&token),
        Some(&format!("fx-next-{key_suffix}")),
        json!({
            "from_currency": from_currency, "to_currency": to_currency, "value": "2400",
            "source": "SEL next approved bulletin", "effective_from": future.to_string(),
            "effective_to": null
        }),
    )
    .await;
    assert_eq!(next_fx.status(), axum::http::StatusCode::CREATED);
    let next_id = json_body(next_fx).await["id"].as_str().unwrap().to_owned();
    let next_approved = post_json(
        &app,
        &format!("/v1/admin/catalog/fx-rates/{next_id}/approve"),
        Some(&token),
        Some(&format!("approve-fx-next-{key_suffix}")),
        json!({
            "approval_ref": format!("SEL-FX-APPROVAL-NEXT-{key_suffix}"), "supersedes_id": fx_id
        }),
    )
    .await;
    assert_eq!(next_approved.status(), axum::http::StatusCode::OK);
    let previous_end: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT effective_to FROM fx_rate_versions WHERE id = $1")
            .bind(Uuid::parse_str(&fx_id).unwrap())
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(previous_end, Some(future - chrono::Duration::days(1)));
    let admin_fx = get_json_authenticated(&app, "/v1/admin/catalog/fx-rates", &token).await;
    assert!(admin_fx
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == fx_id));
    assert!(admin_fx
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == next_id));
}

async fn get_json(app: &axum::Router, path: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("authorization", "Bearer catalog-fixture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn get_json_authenticated(app: &axum::Router, path: &str, token: &str) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn post_json(
    app: &axum::Router,
    path: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Value,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    app.clone()
        .oneshot(
            builder
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}
