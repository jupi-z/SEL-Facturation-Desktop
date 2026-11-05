use rust_decimal::Decimal;
use sel_domain::solar::{remaining_balance, validate_schedule, Installment, SolarSchedule};

#[test]
fn solar_schedule_is_one_obligation_and_partial_payment_reduces_it_once() {
    let schedule = SolarSchedule {
        principal: Decimal::new(100, 0),
        currency: "USD".into(),
        installments: vec![
            Installment {
                sequence: 1,
                due_date: "2026-12-01".into(),
                amount: Decimal::new(50, 0),
            },
            Installment {
                sequence: 2,
                due_date: "2027-01-01".into(),
                amount: Decimal::new(50, 0),
            },
        ],
    };
    validate_schedule(&schedule).expect("exact schedule");
    assert_eq!(
        remaining_balance(schedule.principal, Decimal::new(20, 0)),
        Decimal::new(80, 0)
    );
    assert_eq!(
        remaining_balance(schedule.principal, Decimal::new(50, 0)),
        Decimal::new(50, 0)
    );
}
