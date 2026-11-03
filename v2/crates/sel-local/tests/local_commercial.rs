use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_contracts::{CreateCustomer, UpdateCustomer};
use sel_local::router_with_store;
use sel_storage::LocalStore;
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn local_commercial_operations_are_persisted_and_offline_pending() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-{}.sqlite", Uuid::new_v4()));
    let database_url = format!("sqlite://{}", path.display());
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let mut store = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("connect local store");
    store
        .configure_offline_identity(Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("configure local actor identity");
    sqlx::query(
        "INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.095', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', '0.50', 'USD', '2026-01-01', 'local-approved-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .expect("seed local tariffs");
    sqlx::query(
        "INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', '0.16', 'fixture', '2026-01-01', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .expect("seed local taxes");

    let app = router_with_store(store.clone());
    let customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/customers")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&CreateCustomer {
                        customer_code: "LOCAL-001".into(),
                        legal_name: "Local customer".into(),
                        address: None,
                        phone: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), StatusCode::CREATED);
    let customer = json_body(customer).await;
    let updated_customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/local/v1/customers/{}",
                    customer["id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .header("if-match", "1")
                .body(Body::from(
                    serde_json::to_vec(&UpdateCustomer {
                        legal_name: Some("Updated local customer".into()),
                        address: Some("Local address".into()),
                        phone: None,
                        status: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let updated_customer_status = updated_customer.status();
    let updated_customer_body = updated_customer
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        updated_customer_status,
        StatusCode::OK,
        "update body: {}",
        String::from_utf8_lossy(&updated_customer_body)
    );
    let updated_customer: serde_json::Value =
        serde_json::from_slice(&updated_customer_body).unwrap();
    assert_eq!(updated_customer["legal_name"], "Updated local customer");
    assert_eq!(updated_customer["revision"], 2);
    let invoice_request = json!({
        "customer_id": customer["id"],
        "billing_period": "2026-11",
        "electricity": {"quantity": "100", "unit": "KWH"},
        "water": {"quantity": "0", "unit": "M3"},
        "water_zero_confirmed": true,
        "currency": "USD",
        "tariff_revision": "local-approved-v1"
    });
    let invoice = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/billing/utility/issue")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&invoice_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invoice.status(), StatusCode::CREATED);
    let invoice = json_body(invoice).await;
    assert_eq!(invoice["status"], "LOCAL_PENDING");
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 2);
    assert_eq!(invoice["lines"][1]["quantity"], "0");
    assert_eq!(invoice["total"], "11.02");

    let void_invoice_request = json!({
        "customer_id": customer["id"],
        "billing_period": "2026-12",
        "electricity": {"quantity": "100", "unit": "KWH"},
        "water": {"quantity": "0", "unit": "M3"},
        "water_zero_confirmed": true,
        "currency": "USD",
        "tariff_revision": "local-approved-v1"
    });
    let void_invoice = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/billing/utility/issue")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&void_invoice_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(void_invoice.status(), StatusCode::CREATED);
    let void_invoice = json_body(void_invoice).await;
    let void_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/invoices/{}/void",
                    void_invoice["invoice_id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"reason": "duplicate period fixture"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(void_response.status(), StatusCode::CREATED);
    let void_response = json_body(void_response).await;
    assert_eq!(void_response["status"], "LOCAL_PENDING");
    let void_status: String = sqlx::query_scalar("SELECT status FROM invoices WHERE id = $1")
        .bind(void_invoice["invoice_id"].as_str().unwrap())
        .fetch_one(&store.pool)
        .await
        .expect("read void invoice status");
    assert_eq!(void_status, "VOID");

    let payment = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/payments")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer["id"],
                        "received_amount": "4.00",
                        "currency": "USD",
                        "method": "CASH",
                        "occurred_at": "2026-11-02T10:00:00Z",
                        "allocations": [{"receivable_id": invoice["receivable_id"], "amount": "3.00"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let payment_status = payment.status();
    let payment_body = payment.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        payment_status,
        StatusCode::CREATED,
        "payment body: {}",
        String::from_utf8_lossy(&payment_body)
    );
    let payment: serde_json::Value = serde_json::from_slice(&payment_body).unwrap();

    let allocation_request = json!({
        "allocations": [{
            "receivable_id": invoice["receivable_id"],
            "amount": "1.00"
        }]
    });
    let allocation = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/payments/{}/allocate",
                    payment["payment_id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .header("idempotency-key", "local-payment-allocation-1")
                .body(Body::from(serde_json::to_vec(&allocation_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let allocation_status = allocation.status();
    let allocation_body = allocation.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        allocation_status,
        StatusCode::CREATED,
        "allocation body: {}",
        String::from_utf8_lossy(&allocation_body)
    );
    let replay = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/payments/{}/allocate",
                    payment["payment_id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .header("idempotency-key", "local-payment-allocation-1")
                .body(Body::from(serde_json::to_vec(&allocation_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);

    let receivable = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/local/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let receivable_status = receivable.status();
    let receivable_bytes = receivable.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        receivable_status,
        StatusCode::OK,
        "receivable body: {}",
        String::from_utf8_lossy(&receivable_bytes)
    );
    let receivable: serde_json::Value = serde_json::from_slice(&receivable_bytes).unwrap();
    assert_eq!(receivable["paid"], "4.00");
    assert_eq!(receivable["balance"], "7.02");

    let reversal = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/payments/{}/reverse",
                    payment["payment_id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "amount": "1.00",
                        "reason": "local correction"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let reversal_status = reversal.status();
    let reversal_body = reversal.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        reversal_status,
        StatusCode::CREATED,
        "reversal body: {}",
        String::from_utf8_lossy(&reversal_body)
    );
    let after_reversal = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/local/v1/receivables/{}",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let after_reversal_status = after_reversal.status();
    let after_reversal_body = after_reversal
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(
        after_reversal_status,
        StatusCode::OK,
        "after reversal body: {}",
        String::from_utf8_lossy(&after_reversal_body)
    );
    let after_reversal: serde_json::Value = serde_json::from_slice(&after_reversal_body).unwrap();
    assert_eq!(after_reversal["paid"], "3.00");
    assert_eq!(after_reversal["balance"], "8.02");

    let credit = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/local/v1/receivables/{}/credit",
                    invoice["receivable_id"].as_str().unwrap()
                ))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "amount": "2.00",
                        "reason": "local service correction"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let credit_status = credit.status();
    let credit_bytes = credit.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        credit_status,
        StatusCode::CREATED,
        "credit body: {}",
        String::from_utf8_lossy(&credit_bytes)
    );
    let credit: serde_json::Value = serde_json::from_slice(&credit_bytes).unwrap();
    assert_eq!(credit["status"], "LOCAL_PENDING");
    assert_eq!(credit["remaining_balance"], "6.02");
    let credit_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM credit_notes WHERE receivable_id = $1")
            .bind(invoice["receivable_id"].as_str().unwrap())
            .fetch_one(&store.pool)
            .await
            .expect("count local credit notes");
    assert_eq!(credit_count, 1);
    let credited_receivable = store
        .get_receivable_summary(invoice["receivable_id"].as_str().unwrap().parse().unwrap())
        .await
        .expect("read credited receivable")
        .expect("credited receivable exists");
    assert_eq!(credited_receivable["credited"], "2.00");
    assert_eq!(credited_receivable["balance"], "6.02");

    drop(app);
    drop(store);
    let reopened = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("reopen local store");
    assert!(reopened
        .get_invoice_response(invoice["invoice_id"].as_str().unwrap().parse().unwrap())
        .await
        .expect("read invoice after restart")
        .is_some());
    assert_eq!(reopened.pending_count().await.unwrap(), 9);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn local_solar_sale_and_contract_are_persisted_offline() {
    let path = std::env::temp_dir().join(format!("sel-v2-local-solar-{}.sqlite", Uuid::new_v4()));
    let store = LocalStore::connect(
        &format!("sqlite://{}", path.display()),
        Uuid::new_v4(),
        Uuid::new_v4(),
    )
    .await
    .expect("connect local store");
    sqlx::query(
        "INSERT INTO products (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, 'KIT-LOCAL-1', 'SOLAR_KIT', 'Local kit', '120.00', 'USD', 1)",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .expect("seed local solar product");
    let (customer, _) = store
        .create_customer(&CreateCustomer {
            customer_code: "SOLAR-LOCAL-1".into(),
            legal_name: "Local solar customer".into(),
            address: None,
            phone: None,
        })
        .await
        .expect("create local solar customer");
    let app = router_with_store(store.clone());
    let quote = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/solar/quotes")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "product_code": "KIT-LOCAL-1",
                        "currency": "USD",
                        "terms_revision": "solar-local-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(quote.status(), StatusCode::OK);
    let quote_body = json_body(quote).await;
    assert_eq!(quote_body["product_code"], "KIT-LOCAL-1");
    assert_eq!(quote_body["principal"], "120.00");
    assert_eq!(quote_body["status"], "QUOTED_FROM_APPROVED_CATALOG");
    let rejected_sale = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/solar/sales")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer.id,
                        "product_code": "KIT-LOCAL-1",
                        "principal": "119.00",
                        "currency": "USD",
                        "terms_revision": "solar-local-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected_sale.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let sale = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/solar/sales")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer.id,
                        "product_code": "KIT-LOCAL-1",
                        "principal": "120.00",
                        "currency": "USD",
                        "terms_revision": "solar-local-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sale.status(), StatusCode::CREATED);
    let sale = json_body(sale).await;
    let contract = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/solar/contracts")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "sale_id": sale["sale_id"],
                        "receivable_id": sale["receivable_id"],
                        "schedule": [
                            {"sequence":1,"due_date":"2026-12-01","amount":"60.00"},
                            {"sequence":2,"due_date":"2027-01-01","amount":"60.00"}
                        ],
                        "currency":"USD",
                        "approved_terms_revision":"solar-local-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(contract.status(), StatusCode::CREATED);
    let plan_id = json_body(contract).await["installment_plan_id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let plan = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/local/v1/installment-plans/{plan_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan_body = json_body(plan).await;
    assert_eq!(plan_body["principal"], "120.00");
    assert_eq!(plan_body["paid"], "0.00");
    assert_eq!(plan_body["remaining"], "120.00");
    assert_eq!(plan_body["schedule"].as_array().unwrap().len(), 2);
    assert_eq!(plan_body["schedule"][0]["status"], "OPEN");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM sale_lines WHERE sale_id = $1),
           (SELECT count(*) FROM installment_plans WHERE id = $2),
           (SELECT count(*) FROM installment_schedule WHERE plan_id = $2)",
    )
    .bind(sale["sale_id"].as_str().unwrap())
    .bind(plan_id.to_string())
    .fetch_one(&store.pool)
    .await
    .expect("read local solar projections");
    assert_eq!(counts, (1, 1, 2));
    assert_eq!(store.pending_count().await.unwrap(), 3);
    let _ = std::fs::remove_file(path);
}
