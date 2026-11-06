use chrono::NaiveDate;
use rust_decimal::Decimal;
use sel_contracts::{
    utility_snapshot_sha256, CreateCustomer, InvoiceLineResponse, UtilityBillingRequest,
    UtilityBillingResponse, UtilityServiceInput,
};
use sel_storage::{migrate_postgres, postgres::UtilityIssueOutcome, CentralStore};
use uuid::Uuid;

fn fixture_invoice(
    customer_id: Uuid,
    invoice_id: Uuid,
    receivable_id: Uuid,
) -> (UtilityBillingRequest, UtilityBillingResponse) {
    let request = UtilityBillingRequest {
        customer_id,
        billing_period: "2026-11".into(),
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
        tariff_revision: "fixture".into(),
        due_date: Some(NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()),
        meter_readings: None,
    };
    let mut response = UtilityBillingResponse {
        invoice_id,
        receivable_id: Some(receivable_id),
        invoice_number: None,
        status: "CENTRAL_ACCEPTED".into(),
        billing_period: request.billing_period.clone(),
        due_date: request.due_date,
        currency: "USD".into(),
        lines: vec![
            InvoiceLineResponse {
                service_code: "ELECTRICITY".into(),
                quantity: Decimal::new(100, 0),
                unit: "KWH".into(),
                unit_price: Some(Decimal::new(10, 2)),
                subtotal: Decimal::new(10, 0),
                tax: Decimal::ZERO,
                total: Decimal::new(10, 0),
            },
            InvoiceLineResponse {
                service_code: "WATER".into(),
                quantity: Decimal::ZERO,
                unit: "M3".into(),
                unit_price: None,
                subtotal: Decimal::ZERO,
                tax: Decimal::ZERO,
                total: Decimal::ZERO,
            },
        ],
        subtotal: Decimal::new(10, 0),
        tax: Decimal::ZERO,
        total: Decimal::new(10, 0),
        snapshot_sha256: String::new(),
    };
    response.snapshot_sha256 = utility_snapshot_sha256(&response).unwrap();
    (request, response)
}

#[tokio::test]
#[ignore = "requires an explicit TEST_DATABASE_URL PostgreSQL integration service"]
async fn concurrent_utility_invoices_same_customer_period_return_one_business_conflict() {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be provided when running ignored PostgreSQL tests");
    let organization_id = Uuid::new_v4();
    let first = CentralStore::connect(&database_url, organization_id)
        .await
        .expect("connect first store");
    migrate_postgres(&first.pool).await.expect("run migrations");
    first
        .ensure_organization("SEL concurrent invoice fixture")
        .await
        .expect("create organization");
    let customer = first
        .create_customer(&CreateCustomer {
            customer_code: format!("INV-CON-{}", &organization_id.to_string()[..8]),
            customer_category: Some("RESIDENTIAL".into()),
            legal_name: "Concurrent invoice fixture".into(),
            address: None,
            phone: None,
        })
        .await
        .expect("create customer");

    let second = first.clone();
    let (left_request, left_response) =
        fixture_invoice(customer.id, Uuid::new_v4(), Uuid::new_v4());
    let (right_request, right_response) =
        fixture_invoice(customer.id, Uuid::new_v4(), Uuid::new_v4());
    let (left, right) = tokio::join!(
        first.issue_utility_invoice(
            &left_request,
            &left_response,
            "invoice-race-left",
            "digest-left"
        ),
        second.issue_utility_invoice(
            &right_request,
            &right_response,
            "invoice-race-right",
            "digest-right"
        ),
    );
    let outcomes = [left, right];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(UtilityIssueOutcome::Created(_))))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    Err(sel_storage::postgres::UtilityIssueError::InvoicePeriodExists)
                )
            })
            .count(),
        1
    );
    let issued = outcomes
        .iter()
        .find_map(|outcome| match outcome {
            Ok(UtilityIssueOutcome::Created(response)) => Some(response),
            _ => None,
        })
        .expect("one invoice must be issued");
    let reference = issued.invoice_number.as_deref().expect("issued reference");
    assert!(reference.starts_with("SEL-"));
    assert_eq!(reference.len(), 19, "invoice reference is date/time only");
    assert!(reference[4..12].bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(reference.as_bytes()[12], b'-');
    assert!(reference[13..].bytes().all(|byte| byte.is_ascii_digit()));
    let persisted = first
        .get_invoice_response(issued.invoice_id)
        .await
        .expect("query invoice")
        .expect("issued invoice exists");
    assert_eq!(persisted.invoice_number.as_deref(), Some(reference));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invoices WHERE customer_id = $1 AND period = '2026-11' AND invoice_number = $2",
    )
    .bind(customer.id)
    .bind(reference)
    .fetch_one(&first.pool)
    .await
    .expect("count invoices");
    assert_eq!(count, 1);
}
