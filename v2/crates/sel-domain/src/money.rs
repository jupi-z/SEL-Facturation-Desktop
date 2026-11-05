use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;

pub fn money(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

pub fn non_negative(value: Decimal, field: &str) -> Result<Decimal, MoneyError> {
    if value.is_sign_negative() {
        return Err(MoneyError::Negative(field.to_owned()));
    }
    Ok(value)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("{0} cannot be negative")]
    Negative(String),
    #[error("currency must not be empty")]
    EmptyCurrency,
}
