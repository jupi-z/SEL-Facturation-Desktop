use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::Value;
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
    assert_eq!(products[0]["price_reference"], "100.00");

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
