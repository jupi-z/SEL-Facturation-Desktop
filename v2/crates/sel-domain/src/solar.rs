use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::money;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Installment {
    pub sequence: u32,
    pub due_date: String,
    pub amount: Decimal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SolarSchedule {
    pub principal: Decimal,
    pub currency: String,
    pub installments: Vec<Installment>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SolarError {
    #[error("solar schedule must have at least one installment")]
    EmptySchedule,
    #[error("solar schedule amounts must equal the principal")]
    PrincipalMismatch,
    #[error("solar installment amounts cannot be negative")]
    NegativeInstallment,
    #[error("ordinary invoice payments must not create an installment plan")]
    OrdinaryInvoicePlan,
}

pub fn validate_schedule(schedule: &SolarSchedule) -> Result<(), SolarError> {
    if schedule.installments.is_empty() {
        return Err(SolarError::EmptySchedule);
    }
    if schedule
        .installments
        .iter()
        .any(|item| item.amount.is_sign_negative())
    {
        return Err(SolarError::NegativeInstallment);
    }
    let sum = money(schedule.installments.iter().map(|item| item.amount).sum());
    if money(schedule.principal) != sum {
        return Err(SolarError::PrincipalMismatch);
    }
    Ok(())
}

pub fn remaining_balance(principal: Decimal, allocated: Decimal) -> Decimal {
    money(principal - allocated)
}
