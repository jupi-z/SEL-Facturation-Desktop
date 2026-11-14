use rust_decimal::Decimal;
use sel_domain::billing::{calculate_utility, ServiceCode, UtilityInput, WaterInput};

#[test]
fn zero_water_is_always_visible_without_inventing_a_tariff() {
    let result = calculate_utility(UtilityInput {
        electricity_kwh: Decimal::new(100, 0),
        water_m3: WaterInput::ZeroConfirmed,
        electricity_unit_price: Decimal::new(95, 3),
        vat_rate: Decimal::new(16, 2),
        water_tariff: None,
        communication_charge: Decimal::new(10, 0),
        currency: "USD".into(),
    })
    .expect("valid zero-water utility invoice");

    assert_eq!(result.lines.len(), 3);
    assert_eq!(result.lines[0].service, ServiceCode::Electricity);
    assert_eq!(result.lines[1].service, ServiceCode::Water);
    assert_eq!(result.lines[1].quantity, Decimal::ZERO);
    assert_eq!(result.lines[1].total, Decimal::ZERO);
    assert_eq!(result.lines[2].service, ServiceCode::Communication);
    assert_eq!(result.lines[2].unit, "FORFAIT");
    assert_eq!(result.lines[2].subtotal.to_string(), "10");
    assert_eq!(result.subtotal.to_string(), "19.50");
    assert_eq!(result.tax.to_string(), "3.12");
    assert_eq!(result.total.to_string(), "22.62");
}

#[test]
fn communication_fee_is_mandatory_and_part_of_the_vat_base() {
    let result = calculate_utility(UtilityInput {
        electricity_kwh: Decimal::new(100, 0),
        water_m3: WaterInput::Positive(Decimal::ONE),
        electricity_unit_price: Decimal::new(95, 3),
        vat_rate: Decimal::new(16, 2),
        water_tariff: Some(Decimal::new(30, 2)),
        communication_charge: Decimal::new(10, 0),
        currency: "USD".into(),
    })
    .expect("valid combined invoice");

    assert_eq!(result.lines.len(), 3);
    assert_eq!(result.lines[2].service, ServiceCode::Communication);
    assert_eq!(result.subtotal.to_string(), "19.80");
    assert_eq!(result.tax.to_string(), "3.17");
    assert_eq!(result.total.to_string(), "22.97");
}

#[test]
fn communication_fee_cannot_be_negative() {
    let error = calculate_utility(UtilityInput {
        electricity_kwh: Decimal::ZERO,
        water_m3: WaterInput::ZeroConfirmed,
        electricity_unit_price: Decimal::ZERO,
        vat_rate: Decimal::new(16, 2),
        water_tariff: None,
        communication_charge: Decimal::new(-1, 0),
        currency: "USD".into(),
    })
    .unwrap_err();

    assert_eq!(
        error,
        sel_domain::billing::BillingError::InvalidCommunicationCharge
    );
}

#[test]
fn invoice_vat_is_rounded_once_on_the_combined_taxable_base() {
    let result = calculate_utility(UtilityInput {
        electricity_kwh: Decimal::new(3, 2),
        water_m3: WaterInput::Positive(Decimal::new(1, 1)),
        electricity_unit_price: Decimal::ONE,
        vat_rate: Decimal::new(16, 2),
        water_tariff: Some(Decimal::new(30, 2)),
        communication_charge: Decimal::new(10, 0),
        currency: "USD".into(),
    })
    .expect("valid combined taxable base");

    assert_eq!(result.subtotal.to_string(), "10.06");
    assert_eq!(result.tax.to_string(), "1.61");
    assert_eq!(
        result.lines.iter().map(|line| line.tax).sum::<Decimal>(),
        result.tax
    );
    assert_eq!(result.total.to_string(), "11.67");
}
