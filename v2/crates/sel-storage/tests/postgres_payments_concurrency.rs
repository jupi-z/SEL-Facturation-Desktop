use chrono::Utc;
use rust_decimal::Decimal;
use sel_contracts::{PaymentAllocationRequest, RecordPaymentRequest};
use sel_storage::{
    migrate_postgres,
    postgres::{PaymentAllocationWriteError, PaymentAllocationWriteOutcome, PaymentWriteError},
    CentralStore,
};
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
                        schedule_id: None,
                    },
                    PaymentAllocationRequest {
                        receivable_id,
                        amount: Decimal::new(7, 0),
                        schedule_id: None,
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
async fn duplicate_and_concurrent_allocations_cannot_overpay_one_installment() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL installment allocation fixture")
        .await
        .expect("create organization");

    let customer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, organization_id, customer_code, legal_name, status)
         VALUES ($1, $2, $3, 'Installment allocation fixture', 'ACTIVE')",
    )
    .bind(customer_id)
    .bind(organization_id)
    .bind(format!("INST-{}", &customer_id.to_string()[..8]))
    .execute(&store.pool)
    .await
    .expect("fixture customer");

    let contract_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO contracts (id, customer_id, contract_type, status, terms_revision)
         VALUES ($1, $2, 'SOLAR_INSTALLMENT', 'ACTIVE', 'fixture-v1')",
    )
    .bind(contract_id)
    .bind(customer_id)
    .execute(&store.pool)
    .await
    .expect("fixture contract");

    let receivable_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'SALE', $2, $3, 'USD', 100.00, 'OPEN')",
    )
    .bind(receivable_id)
    .bind(Uuid::new_v4())
    .bind(customer_id)
    .execute(&store.pool)
    .await
    .expect("fixture receivable");

    let plan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installment_plans
         (id, receivable_id, contract_id, principal_amount, currency, start_date, status, terms_snapshot)
         VALUES ($1, $2, $3, 100.00, 'USD', CURRENT_DATE, 'ACTIVE', '{}'::jsonb)",
    )
    .bind(plan_id)
    .bind(receivable_id)
    .bind(contract_id)
    .execute(&store.pool)
    .await
    .expect("fixture installment plan");

    let schedule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installment_schedule
         (id, plan_id, sequence, due_date, amount_due, schedule_status)
         VALUES ($1, $2, 1, CURRENT_DATE, 10.00, 'OPEN')",
    )
    .bind(schedule_id)
    .bind(plan_id)
    .execute(&store.pool)
    .await
    .expect("fixture installment");

    let duplicated = store
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
                        schedule_id: Some(schedule_id),
                    },
                    PaymentAllocationRequest {
                        receivable_id,
                        amount: Decimal::new(7, 0),
                        schedule_id: Some(schedule_id),
                    },
                ],
            },
            "duplicate-installment-allocation",
            "duplicate-installment-digest",
        )
        .await;
    assert!(matches!(
        duplicated,
        Err(PaymentWriteError::InstallmentOverallocated)
    ));

    let missing_schedule = store
        .record_payment(
            &RecordPaymentRequest {
                customer_id,
                received_amount: Decimal::new(7, 0),
                currency: "USD".into(),
                method: "CASH".into(),
                occurred_at: Utc::now(),
                allocations: vec![PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(7, 0),
                    schedule_id: None,
                }],
            },
            "missing-installment-target",
            "missing-installment-target-digest",
        )
        .await;
    assert!(matches!(
        missing_schedule,
        Err(PaymentWriteError::InstallmentRequired)
    ));

    let payment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO payments (id, customer_id, amount_received, currency, method, paid_at, status)
         VALUES ($1, $2, 14.00, 'USD', 'CASH', now(), 'CENTRAL_ACCEPTED')",
    )
    .bind(payment_id)
    .bind(customer_id)
    .execute(&store.pool)
    .await
    .expect("fixture accepted payment");
    let missing_schedule_allocation = store
        .allocate_payment(
            payment_id,
            &[PaymentAllocationRequest {
                receivable_id,
                amount: Decimal::new(7, 0),
                schedule_id: None,
            }],
            Uuid::new_v4(),
            "allocate-without-installment",
            "allocate-without-installment-digest",
        )
        .await;
    assert!(matches!(
        missing_schedule_allocation,
        Err(PaymentAllocationWriteError::InstallmentRequired)
    ));
    let allocate_duplicate = store
        .allocate_payment(
            payment_id,
            &[
                PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(7, 0),
                    schedule_id: Some(schedule_id),
                },
                PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(7, 0),
                    schedule_id: Some(schedule_id),
                },
            ],
            Uuid::new_v4(),
            "allocate-duplicate-installment",
            "allocate-duplicate-installment-digest",
        )
        .await;
    assert!(matches!(
        allocate_duplicate,
        Err(PaymentAllocationWriteError::InstallmentOverallocated)
    ));

    let second = store.clone();
    let payment_request = |amount| RecordPaymentRequest {
        customer_id,
        received_amount: amount,
        currency: "USD".into(),
        method: "CASH".into(),
        occurred_at: Utc::now(),
        allocations: vec![PaymentAllocationRequest {
            receivable_id,
            amount,
            schedule_id: Some(schedule_id),
        }],
    };
    let left_request = payment_request(Decimal::new(7, 0));
    let right_request = payment_request(Decimal::new(7, 0));
    let (left, right) = tokio::join!(
        store.record_payment(&left_request, "installment-race-a", "digest-a"),
        second.record_payment(&right_request, "installment-race-b", "digest-b"),
    );
    let accepted = [&left, &right]
        .into_iter()
        .filter(|result| result.is_ok())
        .count();
    let rejected = [&left, &right]
        .into_iter()
        .filter(|result| matches!(result, Err(PaymentWriteError::InstallmentOverallocated)))
        .count();
    assert_eq!((accepted, rejected), (1, 1));

    let (paid, schedule_paid): (Decimal, Decimal) = sqlx::query_as(
        "SELECT
           (SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE receivable_id = $1),
           (SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE schedule_id = $2)",
    )
    .bind(receivable_id)
    .bind(schedule_id)
    .fetch_one(&store.pool)
    .await
    .expect("read final allocation totals");
    assert_eq!(paid, Decimal::new(7, 0));
    assert_eq!(schedule_paid, Decimal::new(7, 0));
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
            schedule_id: None,
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
        .filter(|result| result.is_ok())
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

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn post_record_payment_allocation_is_atomic_idempotent_and_grouped() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL post-payment allocation fixture")
        .await
        .expect("create organization");
    let customer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, organization_id, customer_code, legal_name, status)
         VALUES ($1, $2, $3, $4, 'ACTIVE')",
    )
    .bind(customer_id)
    .bind(organization_id)
    .bind(format!("ALLOC-{}", &customer_id.to_string()[..8]))
    .bind("Post-payment allocation fixture")
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
    let payment_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO payments (id, customer_id, amount_received, currency, method, paid_at, status)
         VALUES ($1, $2, 10.00, 'USD', 'CASH', now(), 'CENTRAL_ACCEPTED')",
    )
    .bind(payment_id)
    .bind(customer_id)
    .execute(&store.pool)
    .await
    .expect("fixture payment");

    let first = store
        .allocate_payment(
            payment_id,
            &[
                sel_contracts::PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(3, 0),
                    schedule_id: None,
                },
                sel_contracts::PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(2, 0),
                    schedule_id: None,
                },
            ],
            actor_id,
            "post-allocate-key",
            "post-allocate-digest",
        )
        .await
        .expect("allocate payment");
    assert!(matches!(first, PaymentAllocationWriteOutcome::Created(_)));

    let replay = store
        .allocate_payment(
            payment_id,
            &[
                sel_contracts::PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(3, 0),
                    schedule_id: None,
                },
                sel_contracts::PaymentAllocationRequest {
                    receivable_id,
                    amount: Decimal::new(2, 0),
                    schedule_id: None,
                },
            ],
            actor_id,
            "post-allocate-key",
            "post-allocate-digest",
        )
        .await
        .expect("replay allocation");
    assert!(matches!(replay, PaymentAllocationWriteOutcome::Replay(_)));

    let overallocated = store
        .allocate_payment(
            payment_id,
            &[sel_contracts::PaymentAllocationRequest {
                receivable_id,
                amount: Decimal::new(6, 0),
                schedule_id: None,
            }],
            actor_id,
            "post-allocate-over-key",
            "post-allocate-over-digest",
        )
        .await;
    assert!(matches!(
        overallocated,
        Err(PaymentAllocationWriteError::AllocationExceedsPayment)
    ));

    let (allocation_count, allocated_total): (i64, Decimal) = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(allocated_amount), 0)
         FROM payment_allocations WHERE payment_id = $1",
    )
    .bind(payment_id)
    .fetch_one(&store.pool)
    .await
    .expect("read allocation totals");
    assert_eq!(allocation_count, 1);
    assert_eq!(allocated_total, Decimal::new(5, 0));
}
