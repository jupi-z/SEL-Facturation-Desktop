use chrono::Utc;
use rust_decimal::Decimal;
use sel_contracts::CreateCustomer;
use sel_storage::{migrate_postgres, CentralStore};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn receivable_summary_reports_only_unpaid_past_due_installments() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let store = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect central store");
    migrate_postgres(&store.pool).await.expect("run migrations");
    store
        .ensure_organization("SEL overdue fixture")
        .await
        .expect("create organization");
    let customer = store
        .create_customer(&CreateCustomer {
            customer_code: format!("OVERDUE-{}", &organization_id.to_string()[..8]),
            legal_name: "Overdue fixture".into(),
            customer_category: Some("RESIDENTIAL".into()),
            address: None,
            phone: None,
        })
        .await
        .expect("create customer");
    let sale_id = Uuid::new_v4();
    let receivable_id = Uuid::new_v4();
    let contract_id = Uuid::new_v4();
    let plan_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO receivables
         (id, source_type, source_id, customer_id, currency, principal_total, status)
         VALUES ($1, 'SALE', $2, $3, 'USD', 100.00, 'OPEN')",
    )
    .bind(receivable_id)
    .bind(sale_id)
    .bind(customer.id)
    .execute(&store.pool)
    .await
    .expect("fixture receivable");
    sqlx::query(
        "INSERT INTO contracts
         (id, customer_id, contract_type, status, terms_revision, signed_at)
         VALUES ($1, $2, 'SOLAR_INSTALLMENT', 'ACTIVE', 'overdue-v1', CURRENT_DATE)",
    )
    .bind(contract_id)
    .bind(customer.id)
    .execute(&store.pool)
    .await
    .expect("fixture contract");
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
    let overdue_schedule_id = Uuid::new_v4();
    let future_schedule_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO installment_schedule
         (id, plan_id, sequence, due_date, amount_due, schedule_status)
         VALUES ($1, $3, 1, CURRENT_DATE - 1, 40.00, 'OPEN'),
                ($2, $3, 2, CURRENT_DATE + 30, 60.00, 'OPEN')",
    )
    .bind(overdue_schedule_id)
    .bind(future_schedule_id)
    .bind(plan_id)
    .execute(&store.pool)
    .await
    .expect("fixture schedule");

    let summary = store
        .get_receivable_summary(receivable_id)
        .await
        .expect("read overdue summary")
        .expect("receivable exists");
    assert_eq!(summary["paid"], "0.00");
    assert_eq!(summary["balance"], "100.00");
    assert_eq!(summary["overdue_amount"], "40.00");
    assert_eq!(summary["status"], "OVERDUE");

    sqlx::query(
        "INSERT INTO payments (id, customer_id, amount_received, currency, method, paid_at, status)
         VALUES ($1, $2, 40.00, 'USD', 'CASH', $3, 'CENTRAL_ACCEPTED')",
    )
    .bind(Uuid::new_v4())
    .bind(customer.id)
    .bind(Utc::now())
    .execute(&store.pool)
    .await
    .expect("fixture payment");
    let payment_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM payments WHERE customer_id = $1 ORDER BY paid_at DESC LIMIT 1",
    )
    .bind(customer.id)
    .fetch_one(&store.pool)
    .await
    .expect("read fixture payment");
    sqlx::query(
        "INSERT INTO payment_allocations
         (id, payment_id, receivable_id, schedule_id, allocated_amount, currency)
         VALUES ($1, $2, $3, $4, 40.00, 'USD')",
    )
    .bind(Uuid::new_v4())
    .bind(payment_id)
    .bind(receivable_id)
    .bind(overdue_schedule_id)
    .execute(&store.pool)
    .await
    .expect("allocate overdue fixture");
    let settled_summary = store
        .get_receivable_summary(receivable_id)
        .await
        .expect("read settled overdue summary")
        .expect("receivable exists");
    assert_eq!(settled_summary["paid"], "40.00");
    assert_eq!(settled_summary["overdue_amount"], "0.00");
    assert_eq!(settled_summary["status"], "OPEN");
    assert_eq!(
        Decimal::new(100, 0) - Decimal::new(40, 0),
        Decimal::new(60, 0)
    );
}
