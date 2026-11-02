use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::{money, non_negative};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceCode {
    Electricity,
    Water,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum WaterInput {
    ZeroConfirmed,
    Positive(Decimal),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct UtilityInput {
    pub electricity_kwh: Decimal,
    pub water_m3: WaterInput,
    pub electricity_unit_price: Decimal,
    pub electricity_tax_rate: Decimal,
    pub water_tariff: Option<Decimal>,
    pub water_tax_rate: Option<Decimal>,
    pub currency: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UtilityLine {
    pub service: ServiceCode,
    pub quantity: Decimal,
    pub unit: &'static str,
    pub unit_price: Option<Decimal>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UtilityResult {
    pub currency: String,
    pub lines: Vec<UtilityLine>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BillingError {
    #[error("{0} cannot be negative")]
    Negative(String),
    #[error("water quantity must be confirmed when it is zero")]
    WaterZeroNotConfirmed,
    #[error("a validated water tariff is required for positive water consumption")]
    MissingWaterTariff,
    #[error("a validated water tax profile is required for positive water consumption")]
    MissingWaterTax,
    #[error("currency is required")]
    EmptyCurrency,
}

pub fn calculate_utility(input: UtilityInput) -> Result<UtilityResult, BillingError> {
    if input.currency.trim().is_empty() {
        return Err(BillingError::EmptyCurrency);
    }
    non_negative(input.electricity_kwh, "electricity quantity")
        .map_err(|_| BillingError::Negative("electricity quantity".into()))?;
    non_negative(input.electricity_unit_price, "electricity tariff")
        .map_err(|_| BillingError::Negative("electricity tariff".into()))?;
    non_negative(input.electricity_tax_rate, "electricity tax")
        .map_err(|_| BillingError::Negative("electricity tax".into()))?;

    let electricity_subtotal = money(input.electricity_kwh * input.electricity_unit_price);
    let electricity_tax = money(electricity_subtotal * input.electricity_tax_rate);
    let electricity = UtilityLine {
        service: ServiceCode::Electricity,
        quantity: input.electricity_kwh,
        unit: "KWH",
        unit_price: Some(input.electricity_unit_price),
        subtotal: electricity_subtotal,
        tax: electricity_tax,
        total: money(electricity_subtotal + electricity_tax),
    };

    let water = match input.water_m3 {
        WaterInput::ZeroConfirmed => UtilityLine {
            service: ServiceCode::Water,
            quantity: Decimal::ZERO,
            unit: "M3",
            unit_price: None,
            subtotal: Decimal::ZERO,
            tax: Decimal::ZERO,
            total: Decimal::ZERO,
        },
        WaterInput::Positive(quantity) => {
            non_negative(quantity, "water quantity")
                .map_err(|_| BillingError::Negative("water quantity".into()))?;
            let price = input.water_tariff.ok_or(BillingError::MissingWaterTariff)?;
            let tax_rate = input.water_tax_rate.ok_or(BillingError::MissingWaterTax)?;
            non_negative(price, "water tariff")
                .map_err(|_| BillingError::Negative("water tariff".into()))?;
            non_negative(tax_rate, "water tax")
                .map_err(|_| BillingError::Negative("water tax".into()))?;
            let subtotal = money(quantity * price);
            let tax = money(subtotal * tax_rate);
            UtilityLine {
                service: ServiceCode::Water,
                quantity,
                unit: "M3",
                unit_price: Some(price),
                subtotal,
                tax,
                total: money(subtotal + tax),
            }
        }
    };

    let lines = vec![electricity, water];
    let subtotal = money(lines.iter().map(|line| line.subtotal).sum());
    let tax = money(lines.iter().map(|line| line.tax).sum());
    Ok(UtilityResult {
        currency: input.currency,
        lines,
        subtotal,
        tax,
        total: money(subtotal + tax),
    })
}
