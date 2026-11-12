use chrono::NaiveDate;
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
    #[error("solar installment amounts must be greater than zero")]
    NonPositiveInstallment,
    #[error("solar installment sequences must be unique and contiguous from one")]
    InvalidSequence,
    #[error("solar installment due dates must use YYYY-MM-DD")]
    InvalidDueDate,
    #[error("solar installment due dates must be strictly increasing")]
    DueDatesNotIncreasing,
    #[error("solar principal must be greater than zero")]
    InvalidPrincipal,
    #[error("ordinary invoice payments must not create an installment plan")]
    OrdinaryInvoicePlan,
}

pub fn validate_schedule(schedule: &SolarSchedule) -> Result<(), SolarError> {
    if schedule.installments.is_empty() {
        return Err(SolarError::EmptySchedule);
    }
    if schedule.principal <= Decimal::ZERO {
        return Err(SolarError::InvalidPrincipal);
    }
    if schedule
        .installments
        .iter()
        .any(|item| item.amount.is_sign_negative())
    {
        return Err(SolarError::NegativeInstallment);
    }
    if schedule
        .installments
        .iter()
        .any(|item| item.amount == Decimal::ZERO)
    {
        return Err(SolarError::NonPositiveInstallment);
    }
    let mut previous_due_date = None;
    for (index, installment) in schedule.installments.iter().enumerate() {
        if installment.sequence != index as u32 + 1 {
            return Err(SolarError::InvalidSequence);
        }
        let due_date = NaiveDate::parse_from_str(&installment.due_date, "%Y-%m-%d")
            .map_err(|_| SolarError::InvalidDueDate)?;
        if previous_due_date.is_some_and(|previous| due_date <= previous) {
            return Err(SolarError::DueDatesNotIncreasing);
        }
        previous_due_date = Some(due_date);
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
