use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    ActivateSolarContractRequest, CreateSolarSaleRequest, InstallmentRequest,
    PaymentAllocationRequest, PaymentAllocationSyncPayload, RecordPaymentRequest, SyncEnvelope,
    SyncEventResult, UtilityBillingRequest, UtilityServiceInput,
};
use sel_storage::postgres::PaymentWriteOutcome;
use sel_storage::{migrate_postgres, CentralStore};
use sel_sync::payload_digest;
use sqlx::{postgres::PgPoolOptions, Row};
use std::str::FromStr;
use uuid::Uuid;

async fn isolated_central_store(
    database_url: &str,
    organization_id: Uuid,
) -> (CentralStore, String) {
    let schema = format!("financial_sync_{}", Uuid::new_v4().simple());
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

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn payment_sync_is_applied_before_the_central_ack_and_is_idempotent() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("financial sync fixture")
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'CASHIER', 'ACTIVE', 'financial-sync')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'financial-sync-key', 'ACTIVE')")
        .bind(device_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now(), now() + interval '1 day', 'fixture')")
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let customer_id = Uuid::new_v4();
    sqlx::query("INSERT INTO customers (id, organization_id, customer_code, legal_name, status) VALUES ($1, $2, 'SYNC-PAY-1', 'Sync payment customer', 'ACTIVE')")
        .bind(customer_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let receivable_id = Uuid::new_v4();
    sqlx::query("INSERT INTO receivables (id, source_type, source_id, customer_id, currency, principal_total, status) VALUES ($1, 'INVOICE', $2, $3, 'USD', 10.00, 'OPEN')")
        .bind(receivable_id)
        .bind(Uuid::new_v4())
        .bind(customer_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let request = RecordPaymentRequest {
        customer_id,
        received_amount: Decimal::new(5, 0),
        currency: "USD".into(),
        method: "CASH".into(),
        occurred_at: Utc::now(),
        allocations: vec![PaymentAllocationRequest {
            receivable_id,
            amount: Decimal::new(3, 0),
            schedule_id: None,
        }],
    };
    let payment_id = Uuid::new_v4();
    let payload = serde_json::json!({
        "request": request,
        "payment_id": payment_id,
    });
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "PAYMENT_RECORDED".into(),
        aggregate_id: payment_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    // Simulate a process crash after the durable business/idempotency commit
    // and before the sync acknowledgement/publication transaction.
    let pre_ack = store
        .record_payment_with_id(
            &request,
            payment_id,
            &format!("sync-payment-{}", event.event_id),
            &event.payload_sha256,
        )
        .await
        .unwrap();
    assert!(matches!(pre_ack, PaymentWriteOutcome::Created(_)));
    let first_store = store.clone();
    let second_store = store.clone();
    let first_event = event.clone();
    let second_event = event.clone();
    let (first, second) = tokio::join!(
        first_store.apply_sync_event(&first_event),
        second_store.apply_sync_event(&second_event),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert!(
        matches!(first, SyncEventResult::Accepted { .. })
            || matches!(second, SyncEventResult::Accepted { .. })
    );
    assert!(
        matches!(first, SyncEventResult::Duplicate { .. })
            || matches!(second, SyncEventResult::Duplicate { .. })
    );
    let count = sqlx::query("SELECT count(*) AS count FROM payments WHERE customer_id = $1")
        .bind(customer_id)
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .get::<i64, _>("count");
    assert_eq!(count, 1);
    let stored_payment_id: Uuid =
        sqlx::query_scalar("SELECT id FROM payments WHERE customer_id = $1")
            .bind(customer_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(stored_payment_id, payment_id);

    let allocation_payload = serde_json::to_value(PaymentAllocationSyncPayload {
        payment_id,
        allocations: vec![PaymentAllocationRequest {
            receivable_id,
            amount: Decimal::new(2, 0),
            schedule_id: None,
        }],
    })
    .unwrap();
    let allocation_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "PAYMENT_ALLOCATED".into(),
        aggregate_id: payment_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&allocation_payload),
        payload: allocation_payload,
        client_schema_version: 1,
    };
    let allocation_result = store.apply_sync_event(&allocation_event).await.unwrap();
    assert!(matches!(
        allocation_result,
        SyncEventResult::Accepted { .. }
    ));
    let allocation_retry = store.apply_sync_event(&allocation_event).await.unwrap();
    assert!(matches!(
        allocation_retry,
        SyncEventResult::Duplicate { .. }
    ));
    let retry = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(retry, SyncEventResult::Duplicate { .. }));
    let reversal_id = Uuid::new_v4();
    let reversal_payload = serde_json::json!({
        "payment_id": payment_id,
        "reversal_id": reversal_id,
        "amount": "1.00",
        "reason": "sync correction",
        "actor_id": actor_id
    });
    let reversal_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "PAYMENT_REVERSED".into(),
        aggregate_id: payment_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&reversal_payload),
        payload: reversal_payload,
        client_schema_version: 1,
    };
    let reversal_result = store.apply_sync_event(&reversal_event).await.unwrap();
    assert!(matches!(reversal_result, SyncEventResult::Accepted { .. }));
    let reversal_retry = store.apply_sync_event(&reversal_event).await.unwrap();
    assert!(matches!(reversal_retry, SyncEventResult::Duplicate { .. }));
    let reversal_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM payment_reversals WHERE payment_id = $1")
            .bind(payment_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(reversal_count, 1);
    let allocation_total: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE receivable_id = $1",
    )
    .bind(receivable_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(allocation_total, Decimal::new(5, 0));
    let publication_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM central_changes WHERE organization_id = $1 AND entity = 'payments' AND entity_id = $2",
    )
    .bind(organization_id)
    .bind(payment_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(publication_count, 2);
    let inbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sync_inbox WHERE device_id = $1 AND event_id = $2",
    )
    .bind(device_id)
    .bind(event.event_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(inbox_count, 1);
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn utility_invoice_sync_persists_required_lines_including_zero_water() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let customer_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let (store, schema) = isolated_central_store(&database_url, organization_id).await;
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("utility sync fixture")
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'BILLING_AGENT', 'ACTIVE', 'utility-sync')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'utility-sync-key', 'ACTIVE')")
        .bind(device_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now(), now() + interval '1 day', 'fixture')")
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO customers (id, organization_id, customer_code, legal_name, customer_category, status) VALUES ($1, $2, 'SYNC-UTILITY-1', 'Utility sync customer', 'RESIDENTIAL', 'ACTIVE')")
        .bind(customer_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let effective_from = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    for (service, unit, price) in [
        ("ELECTRICITY", "KWH", Decimal::new(10, 2)),
        ("WATER", "M3", Decimal::new(125, 2)),
    ] {
        sqlx::query("INSERT INTO tariff_versions (id, service_code, customer_category, unit, unit_price, currency, effective_from, approval_ref, status) VALUES ($1, $2, 'RESIDENTIAL', $3, $4, 'USD', $5, 'UTILITY-TEST-1', 'APPROVED')")
            .bind(Uuid::new_v4())
            .bind(service)
            .bind(unit)
            .bind(price)
            .bind(effective_from)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status, approval_ref) VALUES ($1, 'VAT', 'SEL', 'SUBTOTAL', 0.16, 'UTILITY-TEST-1', $2, 'APPROVED', 'UTILITY-TEST-1') ON CONFLICT (id) DO UPDATE SET status = 'APPROVED', effective_from = EXCLUDED.effective_from, effective_to = NULL, rate = 0.16, base = 'SUBTOTAL', jurisdiction = 'SEL'")
        .bind(Uuid::parse_str("00000000-0000-4000-8000-000000000024").unwrap())
        .bind(effective_from)
        .execute(&store.pool)
        .await
        .unwrap();
    let request = UtilityBillingRequest {
        customer_id,
        billing_period: "2026-10".into(),
        electricity: UtilityServiceInput {
            quantity: Decimal::new(100, 0),
            unit: "KWH".into(),
        },
        water: UtilityServiceInput {
            quantity: Decimal::ZERO,
            unit: "M3".into(),
        },
        water_zero_confirmed: true,
        currency: "USD".into(),
        tariff_revision: "UTILITY-TEST-1".into(),
        due_date: Some(NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()),
        meter_readings: None,
    };
    let payload = serde_json::json!({
        "request": request,
        "invoice_id": invoice_id,
        "receivable_id": receivable_id,
    });
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "UTILITY_INVOICE_ISSUED".into(),
        aggregate_id: invoice_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    let result = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(result, SyncEventResult::Accepted { .. }));
    let line_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM invoice_lines WHERE invoice_id = $1")
            .bind(invoice_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(line_count, 3);
    let water_quantity: Decimal = sqlx::query_scalar(
        "SELECT quantity FROM invoice_lines WHERE invoice_id = $1 AND service_code = 'WATER'",
    )
    .bind(invoice_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(water_quantity, Decimal::ZERO);
    let communication_subtotal: Decimal = sqlx::query_scalar(
        "SELECT subtotal FROM invoice_lines WHERE invoice_id = $1 AND service_code = 'COMMUNICATION'",
    )
    .bind(invoice_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(communication_subtotal, Decimal::new(10, 0));
    let stored_receivable_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM receivables WHERE source_type = 'INVOICE' AND source_id = $1",
    )
    .bind(invoice_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(stored_receivable_id, receivable_id);
    let retry = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(retry, SyncEventResult::Duplicate { .. }));

    let positive_invoice_id = Uuid::new_v4();
    let positive_receivable_id = Uuid::new_v4();
    let positive_request = UtilityBillingRequest {
        customer_id,
        billing_period: "2026-11".into(),
        electricity: UtilityServiceInput {
            quantity: Decimal::new(100, 0),
            unit: "KWH".into(),
        },
        water: UtilityServiceInput {
            quantity: Decimal::new(2, 0),
            unit: "M3".into(),
        },
        water_zero_confirmed: false,
        currency: "USD".into(),
        tariff_revision: "UTILITY-TEST-1".into(),
        due_date: Some(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()),
        meter_readings: None,
    };
    let positive_payload = serde_json::json!({
        "request": positive_request,
        "invoice_id": positive_invoice_id,
        "receivable_id": positive_receivable_id,
    });
    let positive_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "UTILITY_INVOICE_ISSUED".into(),
        aggregate_id: positive_invoice_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&positive_payload),
        payload: positive_payload,
        client_schema_version: 1,
    };
    let positive_result = store.apply_sync_event(&positive_event).await.unwrap();
    assert!(matches!(positive_result, SyncEventResult::Accepted { .. }));
    let positive_water: (Decimal, Decimal, Decimal) = sqlx::query_as(
        "SELECT quantity, subtotal, total FROM invoice_lines
         WHERE invoice_id = $1 AND service_code = 'WATER'",
    )
    .bind(positive_invoice_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(positive_water.0, Decimal::new(2, 0));
    assert_eq!(positive_water.1, Decimal::new(250, 2));
    assert_eq!(positive_water.2, Decimal::new(290, 2));
    store.pool.close().await;
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
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn solar_sync_creates_one_receivable_and_one_installment_plan() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let customer_id = Uuid::new_v4();
    let (store, schema) = isolated_central_store(&database_url, organization_id).await;
    migrate_postgres(&store.pool).await.unwrap();
    store
        .ensure_organization("solar sync fixture")
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, organization_id, role, status, identity_ref) VALUES ($1, $2, 'BILLING_AGENT', 'ACTIVE', 'solar-sync')")
        .bind(actor_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (id, organization_id, public_key_id, state) VALUES ($1, $2, 'solar-sync-key', 'ACTIVE')")
        .bind(device_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO offline_grants (id, user_id, device_id, scopes, limits, issued_at, expires_at, signature) VALUES ($1, $2, $3, '{}'::jsonb, '{}'::jsonb, now(), now() + interval '1 day', 'fixture')")
        .bind(grant_id)
        .bind(actor_id)
        .bind(device_id)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO customers (id, organization_id, customer_code, legal_name, status) VALUES ($1, $2, 'SYNC-SOLAR-1', 'Solar sync customer', 'ACTIVE')")
        .bind(customer_id)
        .bind(organization_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let product_code = format!("SYNC-KIT-{}", Uuid::new_v4());
    let expected_sale_id = Uuid::new_v4();
    let expected_receivable_id = Uuid::new_v4();
    sqlx::query("INSERT INTO products (id, sku, product_type, label, price_reference, currency, active) VALUES ($1, $2, 'SOLAR_KIT', 'Sync solar kit', 1000, 'USD', TRUE)")
        .bind(Uuid::new_v4())
        .bind(&product_code)
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status, approval_ref) VALUES ($1, 'VAT', 'SEL', 'SUBTOTAL', 0.16, 'central-vat-sync-v1', CURRENT_DATE, 'APPROVED', 'central-vat-sync-v1') ON CONFLICT (id) DO UPDATE SET status = 'APPROVED', effective_from = CURRENT_DATE, effective_to = NULL, rate = 0.16, base = 'SUBTOTAL', jurisdiction = 'SEL'")
        .bind(Uuid::parse_str("00000000-0000-4000-8000-000000000024").unwrap())
        .execute(&store.pool)
        .await
        .unwrap();
    let sale_request = CreateSolarSaleRequest {
        customer_id,
        product_code,
        principal: Decimal::new(1160, 0),
        currency: "USD".into(),
        terms_revision: "solar-sync-v1".into(),
    };
    let sale_payload = serde_json::json!({
        "request": sale_request,
        "sale_id": expected_sale_id,
        "receivable_id": expected_receivable_id,
    });
    let sale_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "SOLAR_SALE_CREATED".into(),
        aggregate_id: expected_sale_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&sale_payload),
        payload: sale_payload,
        client_schema_version: 1,
    };
    let sale_result = store.apply_sync_event(&sale_event).await.unwrap();
    let sale_id = match sale_result {
        SyncEventResult::Accepted {
            server_entity_id, ..
        } => server_entity_id,
        other => panic!("unexpected sale sync result: {other:?}"),
    };
    assert_eq!(sale_id, expected_sale_id);
    let (receivable_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM receivables WHERE source_type = 'SALE' AND source_id = $1")
            .bind(sale_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(receivable_id, expected_receivable_id);
    let contract_id = Uuid::new_v4();
    let expected_plan_id = Uuid::new_v4();
    let contract_request = ActivateSolarContractRequest {
        sale_id,
        receivable_id,
        schedule: vec![
            InstallmentRequest {
                sequence: 1,
                due_date: NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(),
                amount: Decimal::new(400, 0),
            },
            InstallmentRequest {
                sequence: 2,
                due_date: NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
                amount: Decimal::new(760, 0),
            },
        ],
        currency: "USD".into(),
        approved_terms_revision: "solar-sync-v1".into(),
    };
    let contract_payload = serde_json::json!({
        "request": contract_request,
        "contract_id": contract_id,
        "installment_plan_id": expected_plan_id,
    });
    let contract_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "SOLAR_CONTRACT_ACTIVATED".into(),
        aggregate_id: contract_id,
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&contract_payload),
        payload: contract_payload,
        client_schema_version: 1,
    };
    let contract_result = store.apply_sync_event(&contract_event).await.unwrap();
    assert!(matches!(contract_result, SyncEventResult::Accepted { .. }));
    let stored_plan_id: Uuid =
        sqlx::query_scalar("SELECT id FROM installment_plans WHERE receivable_id = $1")
            .bind(receivable_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(stored_plan_id, expected_plan_id);
    let counts = sqlx::query(
        "SELECT (SELECT count(*) FROM receivables WHERE id = $1) AS receivables,
                (SELECT count(*) FROM installment_plans WHERE receivable_id = $1) AS plans,
                (SELECT count(*) FROM installment_schedule WHERE plan_id = (SELECT id FROM installment_plans WHERE receivable_id = $1)) AS installments",
    )
    .bind(receivable_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(counts.get::<i64, _>("receivables"), 1);
    assert_eq!(counts.get::<i64, _>("plans"), 1);
    assert_eq!(counts.get::<i64, _>("installments"), 2);
    let principal: Decimal =
        sqlx::query_scalar("SELECT principal_total FROM receivables WHERE id = $1")
            .bind(receivable_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(principal, Decimal::new(1160, 0));
    let (subtotal, total, tax_amount): (Decimal, Decimal, Decimal) = sqlx::query_as(
        "SELECT sl.unit_price, sl.total, (sl.tax_snapshot ->> 'amount')::numeric
         FROM sale_lines sl WHERE sl.sale_id = $1",
    )
    .bind(sale_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(subtotal, Decimal::new(1000, 0));
    assert_eq!(tax_amount, Decimal::new(160, 0));
    assert_eq!(total, principal);
    store.pool.close().await;
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
}
