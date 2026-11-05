use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, Row};
use std::str::FromStr;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn invoice_void_is_audited_and_idempotent() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let schema = format!("invoice_void_{}", Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("connect test database to create isolated schema");
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .expect("create isolated schema");
    let options = sqlx::postgres::PgConnectOptions::from_str(&database_url)
        .expect("parse test database URL")
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .expect("connect to isolated schema");
    admin_pool.close().await;
    let store = CentralStore::from_pool(pool, organization_id);
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL invoice void fixture")
        .await
        .expect("create organization");
    let actor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref)
         VALUES ($1, $2, 'ADMIN', 'ACTIVE', 'void-fixture-admin')",
    )
    .bind(actor_id)
    .bind(organization_id)
    .execute(&store.pool)
    .await
    .expect("insert actor");
    // Integration test binaries share the disposable CI database. Use a
    // historical effective period so unrelated tariff/tax fixtures created
    // by earlier tests cannot make the legacy tax lookup ambiguous.
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency,
          effective_from, approval_ref, approved_by, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', 0.095, 'USD',
                 '2000-01-01', 'void-fixture-v1', $2, 'APPROVED')",
    )
    .bind(Uuid::new_v4())
    .bind(actor_id)
    .execute(&store.pool)
    .await
    .expect("insert tariff");
    sqlx::query(
        "INSERT INTO tax_profile_versions
         (id, service_code, jurisdiction, base, rate, source, effective_from, effective_to, status)
         VALUES ($1, 'ELECTRICITY', 'FIXTURE', 'SUBTOTAL', 0.16, 'fixture',
                 '2000-01-01', '2000-12-31', 'APPROVED')",
    )
    .bind(Uuid::new_v4())
    .execute(&store.pool)
    .await
    .expect("insert tax");

    let app = router(AppState::with_central_store(store.clone()));
    let customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/customers")
                .header("authorization", "Bearer fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_code": "VOID-CUSTOMER",
                        "legal_name": "Void fixture customer",
                        "customer_category": "RESIDENTIAL"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), StatusCode::CREATED);
    let customer = json_body(customer).await;
    let issue = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/billing/utility/issue")
                .header("authorization", "Bearer fixture")
                .header("Idempotency-Key", "void-issue-key")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer["id"],
                        "billing_period": "2000-12",
                        "electricity": {"quantity": "100", "unit": "KWH"},
                        "water": {"quantity": "0", "unit": "M3"},
                        "water_zero_confirmed": true,
                        "currency": "USD",
                        "tariff_revision": "void-fixture-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let issue_status = issue.status();
    let issue_payload = json_body(issue).await;
    assert_eq!(
        issue_status,
        StatusCode::CREATED,
        "utility issue failed: {issue_payload}"
    );
    let invoice = issue_payload;
    let void_request = || {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/v1/invoices/{}/void",
                invoice["invoice_id"].as_str().unwrap()
            ))
            .header("authorization", "Bearer fixture")
            .header("X-Actor-Id", actor_id.to_string())
            .header("Idempotency-Key", "void-invoice-key")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({"reason": "duplicate fixture invoice"})).unwrap(),
            ))
            .unwrap()
    };
    let first_void = app.clone().oneshot(void_request()).await.unwrap();
    assert_eq!(first_void.status(), StatusCode::OK);
    let first_void = json_body(first_void).await;
    assert_eq!(first_void["status"], "VOID");
    let retry = app.clone().oneshot(void_request()).await.unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(json_body(retry).await, first_void);

    let void_history = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/invoices?customer_id={}&period=2000-12&status=VOID",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer fixture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(void_history.status(), StatusCode::OK);
    let void_history = json_body(void_history).await;
    assert_eq!(void_history.as_array().unwrap().len(), 1);
    assert_eq!(void_history[0]["invoice_id"], invoice["invoice_id"]);
    assert_eq!(void_history[0]["status"], "VOID");

    let invalid_status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/invoices?status=UNKNOWN")
                .header("authorization", "Bearer fixture")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_status.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let counts = sqlx::query(
        "SELECT i.status AS invoice_status, r.status AS receivable_status,
                (SELECT count(*) FROM invoice_voids WHERE invoice_id = i.id) AS voids,
                (SELECT count(*) FROM audit_events
                 WHERE event_type = 'INVOICE_VOIDED' AND subject_id = i.id) AS audits
         FROM invoices i
         JOIN receivables r ON r.source_id = i.id AND r.source_type = 'INVOICE'
         WHERE i.id = $1",
    )
    .bind(Uuid::parse_str(invoice["invoice_id"].as_str().unwrap()).unwrap())
    .fetch_one(&store.pool)
    .await
    .expect("read void state");
    assert_eq!(counts.get::<String, _>("invoice_status"), "VOID");
    assert_eq!(counts.get::<String, _>("receivable_status"), "VOID");
    assert_eq!(counts.get::<i64, _>("voids"), 1);
    assert_eq!(counts.get::<i64, _>("audits"), 1);
    drop(app);
    store.pool.close().await;
    let cleanup_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("reconnect to clean isolated schema");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&cleanup_pool)
        .await
        .expect("drop isolated schema");
}
