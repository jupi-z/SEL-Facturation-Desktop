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
    let store = LocalStore::connect(&database_url, organization_id, device_id)
        .await
        .expect("connect local store");
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
                        "received_amount": "3.00",
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
    assert_eq!(receivable["paid"], "3.00");
    assert_eq!(receivable["balance"], "8.02");

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
    assert_eq!(reopened.pending_count().await.unwrap(), 4);
    let _ = std::fs::remove_file(path);
}
