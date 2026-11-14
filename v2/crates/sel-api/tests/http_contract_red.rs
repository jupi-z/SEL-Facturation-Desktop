use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sel_api::{router, AppState, TariffBook};
use serde_json::json;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn utility_invoice_always_returns_electricity_and_zero_water_as_json() {
    let state = AppState::new(Uuid::new_v4());
    state
        .configure_tariffs(TariffBook {
            electricity_unit_price: Some("0.095".parse().unwrap()),
            vat_rate: Some("0.16".parse().unwrap()),
            water_unit_price: None,
            communication_charge: Some("10".parse().unwrap()),
            revision: Some("fixture-approved-v1".into()),
        })
        .await;
    let app = router(state.clone());
    let customer = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/customers")
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"customer_code":"C-1","legal_name":"Test SEL"}))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(customer.status(), StatusCode::CREATED);
    let body = customer.into_body().collect().await.unwrap().to_bytes();
    let customer: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!(
                    "/v1/customers/{}",
                    customer["id"].as_str().unwrap()
                ))
                .header("authorization", "Bearer test")
                .header("If-Match", "\"1\"")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"phone":"+243000000000"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()["etag"], "\"2\"");
    let request = json!({"customer_id":customer["id"],"billing_period":"2026-11","electricity":{"quantity":"100","unit":"KWH"},"water":{"quantity":"0","unit":"M3"},"water_zero_confirmed":true,"currency":"USD","tariff_revision":"fixture-approved-v1"});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/billing/utility/issue")
                .header("authorization", "Bearer test")
                .header("Idempotency-Key", "utility-1")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let invoice: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(invoice["lines"].as_array().unwrap().len(), 3);
    assert_eq!(invoice["lines"][1]["service_code"], "WATER");
    assert_eq!(invoice["lines"][1]["quantity"], "0");
    assert_eq!(invoice["lines"][2]["service_code"], "COMMUNICATION");
    let receivable = app
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
    assert_eq!(receivable.status(), StatusCode::OK);
}

#[tokio::test]
async fn customer_listing_supports_bounded_cursor_pagination_and_status_filter() {
    let app = router(AppState::new(Uuid::new_v4()));
    for (code, status) in [
        ("C-001", "ACTIVE"),
        ("C-002", "ARCHIVED"),
        ("C-003", "ACTIVE"),
    ] {
        let created = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/customers")
                    .header("authorization", "Bearer test")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({"customer_code":code,"legal_name":code}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        if status == "ARCHIVED" {
            let customer: serde_json::Value =
                serde_json::from_slice(&created.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            let archived = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(format!(
                            "/v1/customers/{}",
                            customer["id"].as_str().unwrap()
                        ))
                        .header("authorization", "Bearer test")
                        .header("If-Match", "\"1\"")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::to_vec(&json!({"status":"ARCHIVED"})).unwrap(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(archived.status(), StatusCode::OK);
        }
    }

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/customers?limit=1&status=ACTIVE")
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let cursor = first
        .headers()
        .get("x-next-cursor")
        .expect("next cursor")
        .to_str()
        .unwrap()
        .to_owned();
    let first_body: serde_json::Value =
        serde_json::from_slice(&first.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(first_body.as_array().unwrap().len(), 1);
    assert_eq!(first_body[0]["customer_code"], "C-001");

    let second = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/customers?limit=1&status=ACTIVE&cursor={cursor}"
                ))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let second_body: serde_json::Value =
        serde_json::from_slice(&second.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(second_body[0]["customer_code"], "C-003");
}

#[tokio::test]
async fn customer_listing_searches_code_and_name_without_case_sensitivity() {
    let app = router(AppState::new(Uuid::new_v4()));
    for (code, name) in [
        ("SEL-C-001", "Clarisse Bomatsa"),
        ("SEL-C-002", "Amani Kizel"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/customers")
                    .header("authorization", "Bearer test")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({"customer_code":code,"legal_name":name}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    for query in ["clarisse", "c-001"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/customers?q={query}"))
                    .header("authorization", "Bearer test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["customer_code"], "SEL-C-001");
    }
}
