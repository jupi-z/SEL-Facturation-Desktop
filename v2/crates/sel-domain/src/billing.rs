use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::{money, non_negative};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServiceCode {
    Electricity,
    Water,
    Communication,
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
    pub communication_charge: Decimal,
    pub communication_tax_rate: Decimal,
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
    #[error("communication charge cannot be negative")]
    InvalidCommunicationCharge,
    #[error("one VAT rate must apply to all taxable invoice lines")]
    TaxRateMismatch,
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
    non_negative(input.communication_charge, "communication charge")
        .map_err(|_| BillingError::InvalidCommunicationCharge)?;
    non_negative(input.communication_tax_rate, "communication tax")
        .map_err(|_| BillingError::Negative("communication tax".into()))?;

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
            if tax_rate != input.electricity_tax_rate {
                return Err(BillingError::TaxRateMismatch);
            }
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

    let communication_subtotal = money(input.communication_charge);
    let communication_tax = money(communication_subtotal * input.communication_tax_rate);
    let mut communication = UtilityLine {
        service: ServiceCode::Communication,
        quantity: Decimal::ONE,
        unit: "FORFAIT",
        unit_price: Some(communication_subtotal),
        subtotal: communication_subtotal,
        tax: communication_tax,
        total: money(communication_subtotal + communication_tax),
    };

    let mut lines = vec![electricity, water];
    let subtotal =
        money(lines.iter().map(|line| line.subtotal).sum::<Decimal>() + communication.subtotal);
    // VAT is computed once on the full taxable base (including the SEL
    // communication charge). Reconcile the rounded line breakdown to that
    // invoice-level result so displayed VAT and invoice totals always agree.
    let tax = money(subtotal * input.electricity_tax_rate);
    let line_tax_total =
        money(lines.iter().map(|line| line.tax).sum::<Decimal>() + communication.tax);
    let adjustment = money(tax - line_tax_total);
    communication.tax = money(communication.tax + adjustment);
    communication.total = money(communication.subtotal + communication.tax);
    lines.push(communication);
    Ok(UtilityResult {
        currency: input.currency,
        lines,
        subtotal,
        tax,
        total: money(subtotal + tax),
    })
}
