use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;
use sel_contracts::{
    ActivateSolarContractRequest, CreateSolarSaleRequest, InstallmentRequest,
    PaymentAllocationRequest, RecordPaymentRequest, SyncEnvelope, SyncEventResult,
    UtilityBillingRequest, UtilityServiceInput,
};
use sel_storage::{migrate_postgres, CentralStore};
use sel_sync::payload_digest;
use sqlx::Row;
use uuid::Uuid;

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
        received_amount: Decimal::new(3, 0),
        currency: "USD".into(),
        method: "CASH".into(),
        occurred_at: Utc::now(),
        allocations: vec![PaymentAllocationRequest {
            receivable_id,
            amount: Decimal::new(3, 0),
        }],
    };
    let payload = serde_json::to_value(&request).unwrap();
    let event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "PAYMENT_RECORDED".into(),
        aggregate_id: Uuid::new_v4(),
        aggregate_expected_version: 0,
        occurred_at: Utc::now(),
        offline_grant_id: grant_id,
        payload_sha256: payload_digest(&payload),
        payload,
        client_schema_version: 1,
    };
    let first = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(first, SyncEventResult::Accepted { .. }));
    let count = sqlx::query("SELECT count(*) AS count FROM payments WHERE customer_id = $1")
        .bind(customer_id)
        .fetch_one(&store.pool)
        .await
        .unwrap()
        .get::<i64, _>("count");
    assert_eq!(count, 1);
    let retry = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(retry, SyncEventResult::Duplicate { .. }));
    let allocation_total: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE receivable_id = $1",
    )
    .bind(receivable_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(allocation_total, Decimal::new(3, 0));
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn utility_invoice_sync_persists_two_lines_including_zero_water() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap();
    let organization_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let grant_id = Uuid::new_v4();
    let customer_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
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
    sqlx::query("INSERT INTO customers (id, organization_id, customer_code, legal_name, status) VALUES ($1, $2, 'SYNC-UTILITY-1', 'Utility sync customer', 'ACTIVE')")
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
    for service in ["ELECTRICITY", "WATER"] {
        sqlx::query("INSERT INTO tax_profile_versions (id, service_code, jurisdiction, base, rate, source, effective_from, status) VALUES ($1, $2, 'TEST', 'ENERGY', 0.16, 'UTILITY-TEST-1', $3, 'APPROVED')")
            .bind(Uuid::new_v4())
            .bind(service)
            .bind(effective_from)
            .execute(&store.pool)
            .await
            .unwrap();
    }
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
    };
    let payload = serde_json::to_value(&request).unwrap();
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
    assert_eq!(line_count, 2);
    let water_quantity: Decimal = sqlx::query_scalar(
        "SELECT quantity FROM invoice_lines WHERE invoice_id = $1 AND service_code = 'WATER'",
    )
    .bind(invoice_id)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(water_quantity, Decimal::ZERO);
    let retry = store.apply_sync_event(&event).await.unwrap();
    assert!(matches!(retry, SyncEventResult::Duplicate { .. }));
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
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .unwrap();
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
    sqlx::query("INSERT INTO products (id, sku, product_type, label, price_reference, currency, active) VALUES ($1, $2, 'SOLAR_KIT', 'Sync solar kit', 1000, 'USD', TRUE)")
        .bind(Uuid::new_v4())
        .bind(&product_code)
        .execute(&store.pool)
        .await
        .unwrap();
    let sale_request = CreateSolarSaleRequest {
        customer_id,
        product_code,
        principal: Decimal::new(1000, 0),
        currency: "USD".into(),
        terms_revision: "solar-sync-v1".into(),
    };
    let sale_payload = serde_json::to_value(&sale_request).unwrap();
    let sale_event = SyncEnvelope {
        protocol_version: 1,
        event_id: Uuid::new_v4(),
        device_id,
        organization_id,
        actor_id,
        command_type: "SOLAR_SALE_CREATED".into(),
        aggregate_id: Uuid::new_v4(),
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
    let (receivable_id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM receivables WHERE source_type = 'SALE' AND source_id = $1")
            .bind(sale_id)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    let contract_id = Uuid::new_v4();
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
                amount: Decimal::new(600, 0),
            },
        ],
        currency: "USD".into(),
        approved_terms_revision: "solar-sync-v1".into(),
    };
    let contract_payload = serde_json::to_value(&contract_request).unwrap();
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
}
