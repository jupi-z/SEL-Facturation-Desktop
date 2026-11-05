use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use sel_api::{router, AppState};
use sel_contracts::{MeterReading, ServiceAccount};
use sel_storage::{migrate_postgres, CentralStore};
use serde_json::json;
use std::str::FromStr;
use tower::ServiceExt;
use uuid::Uuid;

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn meter_readings_are_persisted_scoped_and_corrected_append_only_over_http() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL meter API fixture")
        .await
        .expect("create fixture organization");
    let app = router(AppState::with_central_store(store.clone()));

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/customers")
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "customer_code":format!("MTR-{}", Uuid::new_v4()),
                        "legal_name":"Meter integration fixture"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), axum::http::StatusCode::CREATED);
    let customer = response_json(created).await;
    let customer_id = customer["id"].as_str().unwrap();

    let account_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/customers/{customer_id}/service-accounts"))
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "service_code":"WATER",
                        "contract_ref":"METER-TEST-CONTRACT",
                        "connection_status":"CONNECTED",
                        "activated_at":"2026-01-01"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(account_response.status(), axum::http::StatusCode::CREATED);
    let account: ServiceAccount = serde_json::from_value(response_json(account_response).await)
        .expect("service account response");

    let meters_before_installation = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/service-accounts/{}/meters", account.id))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        meters_before_installation.status(),
        axum::http::StatusCode::OK
    );
    assert_eq!(
        response_json(meters_before_installation).await,
        json!([]),
        "service accounts must be usable before a physical meter is installed"
    );

    let meter_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/service-accounts/{}/meters", account.id))
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"serial":"WATER-1","unit":"M3"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(meter_response.status(), axum::http::StatusCode::CREATED);
    let meter = response_json(meter_response).await;
    let meter_id = meter["id"].as_str().unwrap();

    let original_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/meters/{meter_id}/readings"))
                .header("authorization", "Bearer test")
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
    assert_eq!(original_response.status(), axum::http::StatusCode::CREATED);
    let original: MeterReading = serde_json::from_value(response_json(original_response).await)
        .expect("original reading response");
    assert_eq!(original.measured_quantity, Decimal::ZERO);

    let corrected_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/meter-readings/{}/corrections", original.id))
                .header("authorization", "Bearer test")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "current":"1.25",
                        "source":"MANUAL",
                        "reason":"Index vérifié"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(corrected_response.status(), axum::http::StatusCode::CREATED);
    let corrected: MeterReading =
        serde_json::from_value(response_json(corrected_response).await).unwrap();
    assert_eq!(corrected.corrected_from_id, Some(original.id));
    assert_eq!(
        corrected.measured_quantity,
        Decimal::from_str("1.25").unwrap()
    );

    let duplicate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/meters/{meter_id}/readings"))
                .header("authorization", "Bearer test")
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
    assert_eq!(
        duplicate_response.status(),
        axum::http::StatusCode::CONFLICT
    );

    let history_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/meters/{meter_id}/readings?period=2026-10"))
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history_response.status(), axum::http::StatusCode::OK);
    let history = response_json(history_response).await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["id"], original.id.to_string());
    assert_eq!(history[1]["id"], corrected.id.to_string());

    let reopened = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("reconnect PostgreSQL");
    let persisted = reopened
        .list_meter_readings(meter_id.parse().unwrap(), Some("2026-10"))
        .await
        .expect("load persisted meter history");
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].id, original.id);
    assert_eq!(persisted[1].id, corrected.id);
}
