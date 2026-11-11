use rust_decimal::Decimal;
use sel_domain::solar::{
    remaining_balance, validate_schedule, Installment, SolarError, SolarSchedule,
};

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

fn valid_schedule() -> SolarSchedule {
    SolarSchedule {
        principal: Decimal::new(100, 0),
        currency: "USD".into(),
        installments: vec![
            Installment {
                sequence: 1,
                due_date: "2026-12-01".into(),
                amount: Decimal::new(40, 0),
            },
            Installment {
                sequence: 2,
                due_date: "2027-01-01".into(),
                amount: Decimal::new(60, 0),
            },
        ],
    }
}

#[test]
fn rejects_invalid_solar_schedule_order_and_dates() {
    let mut schedule = valid_schedule();
    schedule.installments[1].sequence = 1;
    assert_eq!(
        validate_schedule(&schedule),
        Err(SolarError::InvalidSequence)
    );

    let mut schedule = valid_schedule();
    schedule.installments[1].due_date = "2026-11-30".into();
    assert_eq!(
        validate_schedule(&schedule),
        Err(SolarError::DueDatesNotIncreasing)
    );

    let mut schedule = valid_schedule();
    schedule.installments[0].due_date = "01/12/2026".into();
    assert_eq!(
        validate_schedule(&schedule),
        Err(SolarError::InvalidDueDate)
    );
}

#[test]
fn rejects_zero_principal_and_zero_installment() {
    let mut schedule = valid_schedule();
    schedule.principal = Decimal::ZERO;
    assert_eq!(
        validate_schedule(&schedule),
        Err(SolarError::InvalidPrincipal)
    );

    let mut schedule = valid_schedule();
    schedule.installments[0].amount = Decimal::ZERO;
    schedule.installments[1].amount = Decimal::new(100, 0);
    assert_eq!(
        validate_schedule(&schedule),
        Err(SolarError::NonPositiveInstallment)
    );
}
