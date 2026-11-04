use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn utility_issue_is_atomic_idempotent_and_survives_restart() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL commercial PostgreSQL fixture")
        .await
        .expect("create organization");
    let approved_by = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'ADMIN', 'ACTIVE', 'fixture')")
        .bind(approved_by)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .expect("fixture user");
    sqlx::query("INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, approved_by, status) VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', 0.095, 'USD', '2026-01-01', 'fixture-approved-v1', $2, 'APPROVED')")
        .bind(Uuid::new_v4())
        .bind(approved_by)
        .execute(&store.pool)
        .await
        .expect("electricity tariff");
    sqlx::query("INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status) VALUES ($1, 'ELECTRICITY', 'FIXTURE', 'SUBTOTAL', 0.16, 'fixture', '2026-01-01', 'APPROVED')")
        .bind(Uuid::new_v4())
        .execute(&store.pool)
        .await
        .expect("electricity tax");

    let app = router(AppState::with_central_store(store.clone()));
    let customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/customers")
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_code": "PG-COMM-1",
                        "legal_name": "Commercial persistence fixture"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), StatusCode::CREATED);
    let customer = json_body(customer).await;
    let request = json!({
        "customer_id": customer["id"],
        "billing_period": "2026-11",
        "electricity": {"quantity": "100", "unit": "KWH"},
        "water": {"quantity": "0", "unit": "M3"},
        "water_zero_confirmed": true,
        "currency": "USD",
        "tariff_revision": "fixture-approved-v1",
        "due_date": "2026-12-15"
    });
    let issue_request = || {
        Request::builder()
            .method("POST")
            .uri("/v1/billing/utility/issue")
            .header("authorization", "Bearer test")
            .header("Idempotency-Key", "pg-utility-key-1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&request).unwrap()))
            .unwrap()
    };
    let first = app.clone().oneshot(issue_request()).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let invoice = json_body(first).await;
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 2);
    assert_eq!(invoice["lines"][1]["quantity"], "0");
    assert_eq!(invoice["total"], "11.02");
    assert_eq!(invoice["billing_period"], "2026-11");
    assert_eq!(invoice["due_date"], "2026-12-15");

    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/invoices?customer_id={}&period=2026-11&limit=10",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    let history = json_body(history).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["invoice_id"], invoice["invoice_id"]);
    assert_eq!(history[0]["status"], "CENTRAL_ACCEPTED");
    assert_eq!(history[0]["period"], "2026-11");
    assert_eq!(history[0]["billing_period"], "2026-11");
    assert_eq!(history[0]["due_date"], "2026-12-15");
    assert!(history[0]["issued_at"].as_str().is_some());

    let retry = app.clone().oneshot(issue_request()).await.unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(json_body(retry).await, invoice);
    let counts = sqlx::query("SELECT (SELECT count(*) FROM invoices WHERE customer_id = $1) AS invoices, (SELECT count(*) FROM invoice_lines WHERE invoice_id = $2) AS lines, (SELECT count(*) FROM receivables WHERE source_id = $2) AS receivables")
        .bind(Uuid::parse_str(customer["id"].as_str().unwrap()).unwrap())
        .bind(Uuid::parse_str(invoice["invoice_id"].as_str().unwrap()).unwrap())
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_eq!(counts.get::<i64, _>("invoices"), 1);
    assert_eq!(counts.get::<i64, _>("lines"), 2);
    assert_eq!(counts.get::<i64, _>("receivables"), 1);
    let payment_request = json!({
        "customer_id": customer["id"],
        "received_amount": "3.00",
        "currency": "USD",
        "method": "CASH",
        "occurred_at": "2026-11-02T10:00:00Z",
        "allocations": [{"receivable_id": invoice["receivable_id"], "amount": "2.00"}]
    });
    let payment_request = Request::builder()
        .method("POST")
        .uri("/v1/payments")
        .header("authorization", "Bearer test")
        .header("Idempotency-Key", "pg-payment-key-1")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&payment_request).unwrap()))
        .unwrap();
    let payment = app.clone().oneshot(payment_request).await.unwrap();
    assert_eq!(payment.status(), StatusCode::CREATED);
    let payment = json_body(payment).await;
    let payment_retry = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/payments")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-payment-key-1")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer["id"],
                        "received_amount": "3.00",
                        "currency": "USD",
                        "method": "CASH",
                        "occurred_at": "2026-11-02T10:00:00Z",
                        "allocations": [{"receivable_id": invoice["receivable_id"], "amount": "2.00"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(payment_retry.status(), StatusCode::OK);
    assert_eq!(json_body(payment_retry).await, payment);
    let payment_list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/payments?customer_id={}&status=CENTRAL_ACCEPTED&limit=10",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(payment_list.status(), StatusCode::OK);
    let payment_list = json_body(payment_list).await;
    assert_eq!(payment_list.as_array().unwrap().len(), 1);
    assert_eq!(payment_list[0]["payment_id"], payment["payment_id"]);
    assert_eq!(payment_list[0]["amount_received"], "3.00");
    let invalid_payment_status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/payments?status=UNKNOWN")
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        invalid_payment_status.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let allocation_request = json!({
        "allocations": [{"receivable_id": invoice["receivable_id"], "amount": "1.00"}]
    });
    let allocation_builder = || {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/v1/payments/{}/allocate",
                payment["payment_id"].as_str().unwrap()
            ))
            .header("authorization", "Bearer test")
            .header("X-Actor-Id", approved_by.to_string())
            .header("Idempotency-Key", "pg-payment-allocation-key-1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&allocation_request).unwrap()))
            .unwrap()
    };
    let allocation = app.clone().oneshot(allocation_builder()).await.unwrap();
    assert_eq!(allocation.status(), StatusCode::CREATED);
    let allocation = json_body(allocation).await;
    assert_eq!(allocation["allocations"].as_array().unwrap().len(), 2);
    let allocation_retry = app.clone().oneshot(allocation_builder()).await.unwrap();
    assert_eq!(allocation_retry.status(), StatusCode::OK);
    assert_eq!(json_body(allocation_retry).await, allocation);
    let receivable = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let receivable_status = receivable.status();
    let receivable = json_body(receivable).await;
    assert_eq!(
        receivable_status,
        StatusCode::OK,
        "receivable body: {receivable}"
    );
    assert_eq!(receivable["paid"], "3.00");
    assert_eq!(receivable["balance"], "8.02");

    let statement = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/customers/{}/statement",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(statement.status(), StatusCode::OK);
    let statement = json_body(statement).await;
    assert!(statement["invoices"].as_array().is_some_and(|invoices| {
        invoices
            .iter()
            .any(|entry| entry["invoice_id"] == invoice["invoice_id"])
    }));
    assert!(statement["payments"].as_array().is_some_and(|payments| {
        payments.iter().any(|entry| {
            entry["payment_id"] == payment["payment_id"]
                && entry["allocations"].as_array().is_some_and(|allocations| {
                    allocations
                        .iter()
                        .any(|allocation| allocation["receivable_id"] == invoice["receivable_id"])
                })
        })
    }));

    let reversal_request = json!({
        "amount": "1.00",
        "reason": "fixture correction"
    });
    let reversal_builder = || {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/v1/payments/{}/reverse",
                payment["payment_id"].as_str().unwrap()
            ))
            .header("authorization", "Bearer test")
            .header("X-Actor-Id", approved_by.to_string())
            .header("Idempotency-Key", "pg-reversal-key-1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&reversal_request).unwrap()))
            .unwrap()
    };
    let reversal = app.clone().oneshot(reversal_builder()).await.unwrap();
    assert_eq!(reversal.status(), StatusCode::CREATED);
    let reversal = json_body(reversal).await;
    assert_eq!(reversal["amount"], "1.00");
    assert_eq!(reversal["remaining_reversible_amount"], "2.00");
    let reversal_retry = app.clone().oneshot(reversal_builder()).await.unwrap();
    assert_eq!(reversal_retry.status(), StatusCode::OK);
    assert_eq!(json_body(reversal_retry).await, reversal);

    let after_partial = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_partial.status(), StatusCode::OK);
    let after_partial = json_body(after_partial).await;
    assert_eq!(after_partial["paid"], "2.00");
    assert_eq!(after_partial["balance"], "9.02");

    let final_reversal = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/payments/{}/reverse",
                    payment["payment_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .header("X-Actor-Id", approved_by.to_string())
                .header("Idempotency-Key", "pg-reversal-key-2")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "amount": "2.00",
                        "reason": "fixture full correction"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(final_reversal.status(), StatusCode::CREATED);
    let final_reversal = json_body(final_reversal).await;
    assert_eq!(final_reversal["remaining_reversible_amount"], "0.00");
    let statement_after_reversals = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/customers/{}/statement",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(statement_after_reversals.status(), StatusCode::OK);
    let statement_after_reversals = json_body(statement_after_reversals).await;
    let payment_history = statement_after_reversals["payments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["payment_id"] == payment["payment_id"])
        .unwrap();
    let mut reversal_amounts = payment_history["reversals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|reversal| reversal["amount"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    reversal_amounts.sort();
    assert_eq!(reversal_amounts, ["1.00", "2.00"]);

    let after_full = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_full.status(), StatusCode::OK);
    let after_full = json_body(after_full).await;
    assert_eq!(after_full["paid"], "0.00");
    assert_eq!(after_full["balance"], "11.02");

    let credit_request = json!({
        "amount": "2.00",
        "reason": "approved commercial correction"
    });
    let credit_builder = || {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/v1/receivables/{}/credit",
                invoice["receivable_id"].as_str().unwrap()
            ))
            .header("authorization", "Bearer test")
            .header("X-Actor-Id", approved_by.to_string())
            .header("Idempotency-Key", "pg-credit-note-key-1")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&credit_request).unwrap()))
            .unwrap()
    };
    let credit = app.clone().oneshot(credit_builder()).await.unwrap();
    assert_eq!(credit.status(), StatusCode::CREATED);
    let credit = json_body(credit).await;
    assert_eq!(credit["amount"], "2.00");
    assert_eq!(credit["remaining_balance"], "9.02");
    let credit_retry = app.clone().oneshot(credit_builder()).await.unwrap();
    assert_eq!(credit_retry.status(), StatusCode::OK);
    assert_eq!(json_body(credit_retry).await, credit);
    let after_credit = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after_credit.status(), StatusCode::OK);
    let after_credit = json_body(after_credit).await;
    assert_eq!(after_credit["credited"], "2.00");
    assert_eq!(after_credit["balance"], "9.02");
    let credit_publications: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM central_changes WHERE organization_id = $1 AND entity = 'credit_notes'",
    )
    .bind(organization_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(credit_publications, 1);
    let reversal_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payment_reversals WHERE payment_id = $1")
            .bind(Uuid::parse_str(payment["payment_id"].as_str().unwrap()).unwrap())
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(reversal_count, 2);
    let receipt = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/payments/{}/document-data",
                    payment["payment_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.status(), StatusCode::OK);
    let receipt = json_body(receipt).await;
    assert_eq!(receipt["document_schema_version"], 1);
    assert_eq!(receipt["document_type"], "PAYMENT_RECEIPT");
    assert_eq!(receipt["payment"]["payment_id"], payment["payment_id"]);
    assert_eq!(receipt["payment"]["amount_received"], "3.00");
    assert_eq!(
        receipt["customer"]["legal_name"],
        "Commercial persistence fixture"
    );
    assert_eq!(receipt["allocations"].as_array().unwrap().len(), 2);
    assert_eq!(receipt["reversals"].as_array().unwrap().len(), 2);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type = 'PAYMENT_REVERSED' AND subject_id = $1",
    )
    .bind(Uuid::parse_str(payment["payment_id"].as_str().unwrap()).unwrap())
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 2);
    let publication_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM central_changes WHERE organization_id = $1 AND entity = 'payment_reversals'",
    )
    .bind(organization_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(publication_count, 2);
    drop(app);
    drop(store);

    let reopened = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("reconnect central store");
    let app = router(AppState::with_central_store(reopened));
    let loaded = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/invoices/{}",
                    invoice["invoice_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let loaded_status = loaded.status();
    let loaded_bytes = loaded.into_body().collect().await.unwrap().to_bytes();
    let loaded_text = String::from_utf8_lossy(&loaded_bytes);
    assert_eq!(loaded_status, StatusCode::OK, "loaded body: {loaded_text}");
    let loaded_body: serde_json::Value = serde_json::from_slice(&loaded_bytes).unwrap();
    assert_eq!(loaded_body, invoice);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn payment_journal_and_receipt_are_scoped_to_the_store_organization() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let other_organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect current organization store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL payment read scope fixture")
        .await
        .expect("create current organization");
    let other_store = CentralStore::connect(&database_url, other_organization_id)
        .await
        .expect("connect other organization store");
    other_store
        .ensure_organization("Other organization payment scope fixture")
        .await
        .expect("create other organization");

    let current_customer_id = Uuid::new_v4();
    let other_customer_id = Uuid::new_v4();
    for (customer_id, organization, code, name) in [
        (
            current_customer_id,
            organization_id,
            "SEL-PAY-SCOPE",
            "SEL customer",
        ),
        (
            other_customer_id,
            other_organization_id,
            "OTHER-PAY-SCOPE",
            "Other customer",
        ),
    ] {
        sqlx::query(
            "INSERT INTO customers (id, organization_id, customer_code, legal_name, status)
             VALUES ($1, $2, $3, $4, 'ACTIVE')",
        )
        .bind(customer_id)
        .bind(organization)
        .bind(code)
        .bind(name)
        .execute(&store.pool)
        .await
        .expect("insert scoped customer");
    }
    let current_payment_id = Uuid::new_v4();
    let other_payment_id = Uuid::new_v4();
    for (payment_id, customer_id) in [
        (current_payment_id, current_customer_id),
        (other_payment_id, other_customer_id),
    ] {
        sqlx::query(
            "INSERT INTO payments (id, customer_id, amount_received, currency, method, paid_at, status)
             VALUES ($1, $2, 2.00, 'USD', 'CASH', now(), 'CENTRAL_ACCEPTED')",
        )
        .bind(payment_id)
        .bind(customer_id)
        .execute(&store.pool)
        .await
        .expect("insert scoped payment");
    }

    let journal = store
        .list_payment_responses(None, None, 100)
        .await
        .expect("read organization payment journal");
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["payment_id"], current_payment_id.to_string());
    assert!(store
        .payment_document_data(current_payment_id)
        .await
        .expect("read in-scope receipt")
        .is_some());
    assert!(store
        .payment_document_data(other_payment_id)
        .await
        .expect("check out-of-scope receipt")
        .is_none());
}
