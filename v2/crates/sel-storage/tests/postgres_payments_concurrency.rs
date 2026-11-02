use chrono::Utc;
use rust_decimal::Decimal;
use sel_contracts::{PaymentAllocationRequest, RecordPaymentRequest};
use sel_storage::{migrate_postgres, postgres::PaymentWriteError, CentralStore};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn duplicate_allocations_are_aggregated_before_balance_validation() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL payment aggregation fixture")
        .await
        .expect("create organization");
    let customer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, organization_id, customer_code, legal_name, status)
         VALUES ($1, $2, $3, $4, 'ACTIVE')",
    )
    .bind(customer_id)
    .bind(organization_id)
    .bind(format!("PAY-{}", &customer_id.to_string()[..8]))
    .bind("Payment aggregation fixture")
    .execute(&store.pool)
    .await
    .expect("fixture customer");
    let receivable_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'INVOICE', $2, $3, 'USD', 10.00, 'OPEN')",
    )
    .bind(receivable_id)
    .bind(Uuid::new_v4())
    .bind(customer_id)
    .execute(&store.pool)
    .await
    .expect("fixture receivable");

    let result = store
        .record_payment(
            &RecordPaymentRequest {
                customer_id,
                received_amount: Decimal::new(14, 0),
                currency: "USD".into(),
                method: "CASH".into(),
                occurred_at: Utc::now(),
                allocations: vec![
                    PaymentAllocationRequest {
                        receivable_id,
                        amount: Decimal::new(7, 0),
                    },
                    PaymentAllocationRequest {
                        receivable_id,
                        amount: Decimal::new(7, 0),
                    },
                ],
            },
            "duplicate-allocation-key",
            "duplicate-allocation-digest",
        )
        .await;

    assert!(matches!(result, Err(PaymentWriteError::Overallocated)));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM payments WHERE customer_id = $1),
            (SELECT count(*) FROM payment_allocations WHERE receivable_id = $2)",
    )
    .bind(customer_id)
    .bind(receivable_id)
    .fetch_one(&store.pool)
    .await
    .expect("read payment counts");
    assert_eq!(counts, (0, 0));
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn concurrent_payments_cannot_both_consume_the_same_balance() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let first = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect first store");
    migrate_postgres(&first.pool).await.expect("run migrations");
    first
        .ensure_organization("SEL concurrent payment fixture")
        .await
        .expect("create organization");
    let customer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, organization_id, customer_code, legal_name, status)
         VALUES ($1, $2, $3, $4, 'ACTIVE')",
    )
    .bind(customer_id)
    .bind(organization_id)
    .bind(format!("CON-{}", &customer_id.to_string()[..8]))
    .bind("Concurrent payment fixture")
    .execute(&first.pool)
    .await
    .expect("fixture customer");
    let receivable_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'INVOICE', $2, $3, 'USD', 10.00, 'OPEN')",
    )
    .bind(receivable_id)
    .bind(Uuid::new_v4())
    .bind(customer_id)
    .execute(&first.pool)
    .await
    .expect("fixture receivable");
    let second = first.clone();
    let make_request = || RecordPaymentRequest {
        customer_id,
        received_amount: Decimal::new(6, 0),
        currency: "USD".into(),
        method: "CASH".into(),
        occurred_at: Utc::now(),
        allocations: vec![PaymentAllocationRequest {
            receivable_id,
            amount: Decimal::new(6, 0),
        }],
    };
    let left_request = make_request();
    let right_request = make_request();
    let (left, right) = tokio::join!(
        first.record_payment(&left_request, "concurrent-left", "digest-left"),
        second.record_payment(&right_request, "concurrent-right", "digest-right"),
    );
    let accepted = [&left, &right]
        .into_iter()
        .filter(|result| matches!(result, Ok(_)))
        .count();
    let rejected = [&left, &right]
        .into_iter()
        .filter(|result| matches!(result, Err(PaymentWriteError::Overallocated)))
        .count();
    assert_eq!(accepted, 1);
    assert_eq!(rejected, 1);
    let total: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE receivable_id = $1",
    )
    .bind(receivable_id)
    .fetch_one(&first.pool)
    .await
    .expect("read allocated total");
    assert_eq!(total, Decimal::new(6, 0));
}
