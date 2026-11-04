use axum::body::Body;
use axum::http::Request;
use chrono::Utc;
use http_body_util::BodyExt;
use sel_api::{router, AppState};
use sel_contracts::{
    solar_schedule_id, CreateCustomer, RecordPaymentRequest, UtilityBillingResponse,
};
use sel_local::router_with_store;
use sel_local::sync_once;
use sel_storage::{migrate_postgres, CentralStore, LocalStore};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::str::FromStr;
use tower::ServiceExt;
use uuid::Uuid;

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn seed_local_catalog(store: &LocalStore, effective_from: chrono::NaiveDate) {
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', '0.095', 'USD', $3, 'convergence-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', '0.50', 'USD', $4, 'convergence-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .bind(effective_from.to_string())
    .bind(effective_from.to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_profile_versions
         (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', '0.16', 'convergence-fixture', $3, 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', '0.16', 'convergence-fixture', $4, 'APPROVED')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(Uuid::new_v4().to_string())
    .bind(effective_from.to_string())
    .bind(effective_from.to_string())
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products
         (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, 'KIT-CONVERGENCE-1', 'SOLAR_KIT', 'Convergence kit', '120.00', 'USD', 1)
         ON CONFLICT(sku, effective_from) DO NOTHING",
    )
    .bind(Uuid::new_v4().to_string())
    .execute(&store.pool)
    .await
    .unwrap();
}

async fn seed_central_catalog(store: &CentralStore, effective_from: chrono::NaiveDate) {
    sqlx::query(
        "INSERT INTO tariff_versions
         (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status)
         VALUES ($1, 'ELECTRICITY', 'RESIDENTIAL', 'KWH', 0.095, 'USD', $3, 'convergence-v1', 'APPROVED'),
                ($2, 'WATER', 'RESIDENTIAL', 'M3', 0.50, 'USD', $4, 'convergence-v1', 'APPROVED')",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(effective_from)
    .bind(effective_from)
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO tax_profile_versions
         (id, service_code, jurisdiction, base, rate, source, effective_from, status)
         VALUES ($1, 'ELECTRICITY', 'LOCAL', 'SUBTOTAL', 0.16, 'convergence-fixture', $3, 'APPROVED'),
                ($2, 'WATER', 'LOCAL', 'SUBTOTAL', 0.16, 'convergence-fixture', $4, 'APPROVED')",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(effective_from)
    .bind(effective_from)
    .execute(&store.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products
         (id, sku, product_type, label, price_reference, currency, active)
         VALUES ($1, 'KIT-CONVERGENCE-1', 'SOLAR_KIT', 'Convergence kit', 120.00, 'USD', TRUE)
         ON CONFLICT(sku, effective_from) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .execute(&store.pool)
    .await
    .unwrap();
}

async fn isolated_central_store(
    database_url: &str,
    organization_id: Uuid,
) -> (CentralStore, String) {
    let schema = format!("convergence_{}", Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .unwrap();

    let options = sqlx::postgres::PgConnectOptions::from_str(database_url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await
        .unwrap();
    admin_pool.close().await;
    (CentralStore::from_pool(pool, organization_id), schema)
}

async fn seed_local_customer(store: &LocalStore) -> sel_contracts::Customer {
    store
        .create_customer(&CreateCustomer {
            customer_code: "CONVERGENCE-1".into(),
            legal_name: "Two device customer".into(),
            address: Some("Local address".into()),
            phone: None,
        })
        .await
        .unwrap()
        .0
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn two_sqlite_devices_converge_through_real_postgres_and_http() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let billing_year = 3_000 + (Uuid::new_v4().as_u128() % 6_000) as u32;
    let effective_from = chrono::NaiveDate::from_ymd_opt(billing_year as i32, 1, 1).unwrap();
    let billing_period = format!("{billing_year}-11");
    let due_date = format!("{billing_year}-11-30");
    let organization_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let device_one = Uuid::new_v4();
    let device_two = Uuid::new_v4();
    let grant_one = Uuid::new_v4();
    let grant_two = Uuid::new_v4();
    let (central, schema) = isolated_central_store(&database_url, organization_id).await;
    migrate_postgres(&central.pool).await.unwrap();
    central
        .ensure_organization("two-device convergence")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users (id, organization_id, role, status, identity_ref)
         VALUES ($1, $2, 'SUPERVISOR', 'ACTIVE', 'two-device-convergence')",
    )
    .bind(actor_id)
    .bind(organization_id)
    .execute(&central.pool)
    .await
    .unwrap();
    for (device_id, grant_id) in [(device_one, grant_one), (device_two, grant_two)] {
        sqlx::query(
            "INSERT INTO devices (id, organization_id, public_key_id, state)
             VALUES ($1, $2, $3, 'ACTIVE')",
        )
        .bind(device_id)
        .bind(organization_id)
        .bind(format!("convergence-{device_id}"))
        .execute(&central.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO offline_grants
             (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature)
             VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now(), now() + interval '1 day', 'convergence-fixture')",
        )
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .execute(&central.pool)
        .await
        .unwrap();
    }
    seed_central_catalog(&central, effective_from).await;
    let central_for_verify = central.clone();

    let base = std::env::temp_dir().join(format!("sel-v2-convergence-{}", Uuid::new_v4()));
    let path_one = base.with_extension("one.sqlite");
    let path_two = base.with_extension("two.sqlite");
    let mut first = LocalStore::connect(
        &format!("sqlite://{}", path_one.display()),
        organization_id,
        device_one,
    )
    .await
    .unwrap();
    let mut second = LocalStore::connect(
        &format!("sqlite://{}", path_two.display()),
        organization_id,
        device_two,
    )
    .await
    .unwrap();
    first
        .configure_offline_identity(actor_id, grant_one)
        .await
        .unwrap();
    second
        .configure_offline_identity(actor_id, grant_two)
        .await
        .unwrap();
    seed_local_catalog(&first, effective_from).await;
    seed_local_catalog(&second, effective_from).await;

    let customer = seed_local_customer(&first).await;
    let invoice_request = json!({
        "customer_id": customer.id,
        "billing_period": billing_period,
        "electricity": {"quantity":"100","unit":"KWH"},
        "water": {"quantity":"0","unit":"M3"},
        "water_zero_confirmed": true,
        "currency":"USD",
        "tariff_revision":"convergence-v1",
        "due_date":due_date
    });
    let local_invoice = router_with_store(first.clone())
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
    assert_eq!(local_invoice.status(), axum::http::StatusCode::CREATED);
    let local_invoice: UtilityBillingResponse =
        serde_json::from_value(json_body(local_invoice).await).unwrap();
    let local_receivable_id = local_invoice
        .receivable_id
        .expect("combined utility invoice must create a receivable");
    let local_payment = router_with_store(first.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/local/v1/payments")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "customer_id": customer.id,
                        "received_amount":"3.00",
                        "currency":"USD",
                        "method":"CASH",
                        "occurred_at":"2026-11-03T10:00:00Z",
                        "allocations":[{"receivable_id":local_receivable_id,"amount":"3.00"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(local_payment.status(), axum::http::StatusCode::CREATED);

    drop(first);
    let mut first = LocalStore::connect(
        &format!("sqlite://{}", path_one.display()),
        organization_id,
        device_one,
    )
    .await
    .unwrap();
    first
        .configure_offline_identity(actor_id, grant_one)
        .await
        .unwrap();

    let token = format!("convergence-token-{}", Uuid::new_v4());
    std::env::set_var("SEL_API_AUTH_TOKEN", &token);
    std::env::set_var("SEL_ALLOW_LEGACY_TOKEN", "1");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router(AppState::with_production_central_store(central)),
        )
        .await
        .unwrap();
    });
    let endpoint = format!("http://{address}");
    let first_sync = sync_once(&first, &endpoint, &token).await.unwrap();
    assert_eq!(first_sync.accepted, 3);
    let second_sync = sync_once(&second, &endpoint, &token).await.unwrap();
    assert_eq!(second_sync.pulled, 3);
    let central_invoice = central_for_verify
        .get_invoice_response(local_invoice.invoice_id)
        .await
        .unwrap()
        .unwrap();
    let first_invoice = first
        .get_invoice_response(local_invoice.invoice_id)
        .await
        .unwrap()
        .unwrap();
    let second_invoice = second
        .get_invoice_response(local_invoice.invoice_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_invoice.invoice_id, second_invoice.invoice_id);
    assert_eq!(first_invoice.total, central_invoice.total);
    assert_eq!(
        first_invoice.snapshot_sha256,
        second_invoice.snapshot_sha256
    );
    assert_eq!(
        first_invoice.snapshot_sha256,
        central_invoice.snapshot_sha256
    );
    assert_eq!(first_invoice.status, "CENTRAL_ACCEPTED");
    for store in [&first, &second] {
        let balance = store
            .get_receivable_summary(local_receivable_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(balance["paid"], "3.00");
        assert_eq!(balance["balance"], "8.02");
    }

    let (sale, _) = first
        .create_solar_sale_offline(&sel_contracts::CreateSolarSaleRequest {
            customer_id: customer.id,
            product_code: "KIT-CONVERGENCE-1".into(),
            principal: "120.00".parse().unwrap(),
            currency: "USD".into(),
            terms_revision: "solar-convergence-v1".into(),
        })
        .await
        .unwrap();
    let (contract, _) = first
        .activate_solar_contract_offline(
            Uuid::new_v4(),
            &sel_contracts::ActivateSolarContractRequest {
                sale_id: sale.sale_id,
                receivable_id: sale.receivable_id,
                schedule: vec![
                    sel_contracts::InstallmentRequest {
                        sequence: 1,
                        due_date: "2026-12-01".parse().unwrap(),
                        amount: "60.00".parse().unwrap(),
                    },
                    sel_contracts::InstallmentRequest {
                        sequence: 2,
                        due_date: "2027-01-01".parse().unwrap(),
                        amount: "60.00".parse().unwrap(),
                    },
                ],
                currency: "USD".into(),
                approved_terms_revision: "solar-convergence-v1".into(),
            },
        )
        .await
        .unwrap();
    let plan_id: Uuid = contract["installment_plan_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let solar_sync = sync_once(&first, &endpoint, &token).await.unwrap();
    assert_eq!(solar_sync.accepted, 2);
    let solar_pull = sync_once(&second, &endpoint, &token).await.unwrap();
    assert_eq!(solar_pull.pulled, 2);
    for store in [&first, &second] {
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT count(*) FROM receivables WHERE id = $1),
               (SELECT count(*) FROM installment_schedule WHERE plan_id = $2)",
        )
        .bind(sale.receivable_id.to_string())
        .bind(plan_id.to_string())
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 2));
        let schedule_id = solar_schedule_id(plan_id, 1);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM installment_schedule WHERE plan_id = $1 AND sequence = 1",
            )
            .bind(plan_id.to_string())
            .fetch_one(&store.pool)
            .await
            .unwrap(),
            schedule_id.to_string()
        );
    }

    let payment_one = RecordPaymentRequest {
        customer_id: customer.id,
        received_amount: "70.00".parse().unwrap(),
        currency: "USD".into(),
        method: "CASH".into(),
        occurred_at: Utc::now(),
        allocations: vec![sel_contracts::PaymentAllocationRequest {
            receivable_id: sale.receivable_id,
            amount: "70.00".parse().unwrap(),
            schedule_id: Some(solar_schedule_id(plan_id, 1)),
        }],
    };
    let mut payment_two = payment_one.clone();
    payment_two.occurred_at = Utc::now();
    let (first_payment, _) = first.record_payment(&payment_one).await.unwrap();
    let (second_payment, _) = second.record_payment(&payment_two).await.unwrap();
    let (first_sync, second_sync) = tokio::join!(
        sync_once(&first, &endpoint, &token),
        sync_once(&second, &endpoint, &token),
    );
    let first_sync = first_sync.unwrap();
    let second_sync = second_sync.unwrap();
    assert_eq!(first_sync.accepted + second_sync.accepted, 1);
    assert_eq!(first_sync.rejected + second_sync.rejected, 1);
    assert_eq!(first.pending_count().await.unwrap(), 0);
    assert_eq!(second.pending_count().await.unwrap(), 0);
    let central = central_for_verify;
    let payment_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM payments p
         JOIN payment_allocations pa ON pa.payment_id = p.id
         WHERE pa.receivable_id = $1",
    )
    .bind(sale.receivable_id)
    .fetch_one(&central.pool)
    .await
    .unwrap();
    assert_eq!(payment_count, 1);
    let first_payment_id: Uuid = first_payment["payment_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let second_payment_id: Uuid = second_payment["payment_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let first_status: String = sqlx::query_scalar("SELECT status FROM payments WHERE id = $1")
        .bind(first_payment_id.to_string())
        .fetch_one(&first.pool)
        .await
        .unwrap();
    let second_status: String = sqlx::query_scalar("SELECT status FROM payments WHERE id = $1")
        .bind(second_payment_id.to_string())
        .fetch_one(&second.pool)
        .await
        .unwrap();
    assert_eq!(
        [first_status.as_str(), second_status.as_str()]
            .into_iter()
            .filter(|status| *status == "CENTRAL_ACCEPTED")
            .count(),
        1
    );
    assert_eq!(
        [first_status.as_str(), second_status.as_str()]
            .into_iter()
            .filter(|status| *status == "REJECTED")
            .count(),
        1
    );
    let second_balance = second
        .get_receivable_summary(sale.receivable_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second_balance["paid"], "70.00");
    assert_eq!(second_balance["balance"], "50.00");
    server.abort();
    let _ = server.await;
    std::env::remove_var("SEL_API_AUTH_TOKEN");
    std::env::remove_var("SEL_ALLOW_LEGACY_TOKEN");
    drop(first);
    drop(second);
    central.pool.close().await;
    let cleanup_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .unwrap();
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&cleanup_pool)
        .await
        .unwrap();
    cleanup_pool.close().await;
    let _ = std::fs::remove_file(path_one);
    let _ = std::fs::remove_file(path_two);
}
