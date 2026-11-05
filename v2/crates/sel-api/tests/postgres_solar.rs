use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::NaiveDate;
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_contracts::{
    ActivateSolarContractRequest, CreateCustomer, CreateSolarSaleRequest, InstallmentRequest,
};
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
async fn solar_sale_and_installment_contract_are_atomic_and_idempotent() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("SEL solar fixture")
        .await
        .unwrap();
    let product_code = format!("KIT-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO products (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, $2, 'SOLAR_KIT', 'Solar kit fixture', 1000, 'USD', TRUE)",
    )
    .bind(Uuid::new_v4())
    .bind(&product_code)
    .execute(&store.pool)
    .await
    .unwrap();

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
                        "customer_code": "PG-SOLAR-1",
                        "legal_name": "Solar fixture"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), StatusCode::CREATED);
    let customer = json_body(customer).await;
    let quote = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/solar/quotes")
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "product_code": product_code,
                        "currency": "USD",
                        "terms_revision": "solar-terms-v1"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(quote.status(), StatusCode::OK);
    let quote_body = json_body(quote).await;
    assert_eq!(quote_body["product_code"], product_code);
    assert_eq!(quote_body["principal"], "1000.00");
    assert_eq!(quote_body["status"], "QUOTED_FROM_APPROVED_CATALOG");
    let sale_request = json!({
        "customer_id": customer["id"],
        "product_code": product_code,
        "principal": "1000.00",
        "currency": "USD",
        "terms_revision": "solar-terms-v1"
    });
    let mut mismatched_sale = sale_request.clone();
    mismatched_sale["principal"] = json!("999.00");
    let rejected_sale = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/solar/sales")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-solar-sale-price-mismatch")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&mismatched_sale).unwrap()))
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
                .uri("/v1/solar/sales")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-solar-sale-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&sale_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sale.status(), StatusCode::CREATED);
    let sale_body = json_body(sale).await;
    assert_eq!(sale_body["principal"], "1000.00");
    let sale_retry = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/solar/sales")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-solar-sale-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&sale_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sale_retry.status(), StatusCode::OK);
    assert_eq!(json_body(sale_retry).await, sale_body);

    let contract_request = json!({
        "sale_id": sale_body["sale_id"],
        "receivable_id": sale_body["receivable_id"],
        "schedule": [
            {"sequence": 1, "due_date": "2026-12-01", "amount": "400.00"},
            {"sequence": 2, "due_date": "2027-01-01", "amount": "600.00"}
        ],
        "currency": "USD",
        "approved_terms_revision": "solar-terms-v1"
    });
    let contract = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/solar/contracts")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-solar-contract-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&contract_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(contract.status(), StatusCode::OK);
    let contract_body = json_body(contract).await;
    assert_eq!(contract_body["principal"], "1000.00");
    assert_eq!(contract_body["schedule"].as_array().unwrap().len(), 2);
    let contract_retry = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/solar/contracts")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "pg-solar-contract-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&contract_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(contract_retry.status(), StatusCode::OK);
    assert_eq!(json_body(contract_retry).await, contract_body);

    let plan = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/installment-plans/{}",
                    contract_body["installment_plan_id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan_body = json_body(plan).await;
    assert_eq!(plan_body["id"], contract_body["installment_plan_id"]);
    assert_eq!(plan_body["principal"], "1000.00");
    assert_eq!(plan_body["paid"], "0.00");
    assert_eq!(plan_body["remaining"], "1000.00");
    assert_eq!(plan_body["schedule"].as_array().unwrap().len(), 2);
    assert_eq!(plan_body["schedule"][0]["status"], "OPEN");

    let counts = sqlx::query(
        "SELECT
           (SELECT count(*) FROM sales WHERE id = $1) AS sales,
           (SELECT count(*) FROM receivables WHERE id = $2) AS receivables,
           (SELECT count(*) FROM installment_plans WHERE receivable_id = $2) AS plans,
           (SELECT count(*) FROM installment_schedule WHERE plan_id = $3) AS installments",
    )
    .bind(Uuid::parse_str(sale_body["sale_id"].as_str().unwrap()).unwrap())
    .bind(Uuid::parse_str(sale_body["receivable_id"].as_str().unwrap()).unwrap())
    .bind(Uuid::parse_str(contract_body["installment_plan_id"].as_str().unwrap()).unwrap())
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(counts.get::<i64, _>("sales"), 1);
    assert_eq!(counts.get::<i64, _>("receivables"), 1);
    assert_eq!(counts.get::<i64, _>("plans"), 1);
    assert_eq!(counts.get::<i64, _>("installments"), 2);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn concurrent_solar_contract_activation_creates_one_schedule_and_one_conflict() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let first = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
    migrate_postgres(&first.pool).await.unwrap();
    first
        .ensure_organization("SEL concurrent solar fixture")
        .await
        .unwrap();

    let product_code = format!("KIT-RACE-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO products (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, $2, 'SOLAR_KIT', 'Concurrent solar fixture', 1000, 'USD', TRUE)",
    )
    .bind(Uuid::new_v4())
    .bind(&product_code)
    .execute(&first.pool)
    .await
    .unwrap();
    let customer = first
        .create_customer(&CreateCustomer {
            customer_code: format!("SOL-RACE-{}", &organization_id.to_string()[..8]),
            customer_category: Some("RESIDENTIAL".into()),
            legal_name: "Concurrent solar fixture".into(),
            address: None,
            phone: None,
        })
        .await
        .unwrap();
    let sale = first
        .create_solar_sale(
            &CreateSolarSaleRequest {
                customer_id: customer.id,
                product_code,
                principal: "1000.00".parse().unwrap(),
                currency: "USD".into(),
                terms_revision: "race-v1".into(),
            },
            "solar-race-sale",
            "solar-race-sale-digest",
        )
        .await
        .unwrap();
    let sale = match sale {
        sel_storage::postgres::SolarSaleWriteOutcome::Created(value)
        | sel_storage::postgres::SolarSaleWriteOutcome::Replay(value) => value,
    };
    let input = ActivateSolarContractRequest {
        sale_id: sale.sale_id,
        receivable_id: sale.receivable_id,
        schedule: vec![InstallmentRequest {
            sequence: 1,
            due_date: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
            amount: "1000.00".parse().unwrap(),
        }],
        currency: "USD".into(),
        approved_terms_revision: "race-v1".into(),
    };
    let second = first.clone();
    let (left, right) = tokio::join!(
        first.activate_solar_contract(
            Uuid::new_v4(),
            &input,
            "solar-race-left",
            "solar-race-left-digest",
        ),
        second.activate_solar_contract(
            Uuid::new_v4(),
            &input,
            "solar-race-right",
            "solar-race-right-digest",
        ),
    );
    let outcomes = [left, right];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                Ok(sel_storage::postgres::SolarContractWriteOutcome::Created(_))
            ))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    Err(sel_storage::postgres::SolarContractWriteError::ContractAlreadyExists)
                )
            })
            .count(),
        1
    );
    let plans: i64 =
        sqlx::query_scalar("SELECT count(*) FROM installment_plans WHERE receivable_id = $1")
            .bind(sale.receivable_id)
            .fetch_one(&first.pool)
            .await
            .unwrap();
    assert_eq!(plans, 1);
    let installments: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installment_schedule s
         JOIN installment_plans p ON p.id = s.plan_id
         WHERE p.receivable_id = $1",
    )
    .bind(sale.receivable_id)
    .fetch_one(&first.pool)
    .await
    .unwrap();
    assert_eq!(installments, 1);
}
