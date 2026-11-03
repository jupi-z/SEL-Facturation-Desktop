use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_api::{router, AppState};
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
         VALUES ($1, 'WATER', 'RESIDENTIAL', 'M3', 1.25, 'USD', '2026-01-01', $2, 'APPROVED'),
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
         (id, from_currency, to_currency, value, effective_from, source, approved)
         VALUES ($1, 'USD', 'CDF', 2360, '2026-01-01', 'fixture', TRUE),
                ($2, 'USD', 'CDF', 9999, '2026-01-01', 'fixture', FALSE)",
    )
    .bind(approved_fx)
    .bind(unapproved_fx)
    .execute(&store.pool)
    .await
    .expect("insert FX fixtures");

    let app = router(AppState::with_central_store(store));
    let products = get_json(&app, "/v1/catalog/products").await;
    assert_eq!(products.as_array().unwrap().len(), 1);
    assert_eq!(products[0]["id"], active_product.to_string());
    assert_eq!(products[0]["price_reference"], "100");

    let tariffs = get_json(&app, "/v1/catalog/tariffs").await;
    assert_eq!(tariffs.as_array().unwrap().len(), 1);
    assert_eq!(tariffs[0]["id"], approved_tariff.to_string());
    assert_eq!(tariffs[0]["unit_price"], "1.25");

    let taxes = get_json(&app, "/v1/catalog/tax-profiles").await;
    assert_eq!(taxes.as_array().unwrap().len(), 1);
    assert_eq!(taxes[0]["id"], approved_tax.to_string());
    assert_eq!(taxes[0]["rate"], "0.16");

    let rates = get_json(&app, "/v1/catalog/fx-rates").await;
    assert_eq!(rates.as_array().unwrap().len(), 1);
    assert_eq!(rates[0]["id"], approved_fx.to_string());
    assert_eq!(rates[0]["value"], "2360");
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
        "unit_price": "1.30",
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
            "effective_from": "2026-11-04",
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
            "effective_from": "2026-11-04",
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
