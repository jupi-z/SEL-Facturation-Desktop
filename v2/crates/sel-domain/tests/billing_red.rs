use rust_decimal::Decimal;
use sel_domain::billing::{calculate_utility, ServiceCode, UtilityInput, WaterInput};

#[test]
fn zero_water_is_always_visible_without_inventing_a_tariff() {
    let result = calculate_utility(UtilityInput {
        electricity_kwh: Decimal::new(100, 0),
        water_m3: WaterInput::ZeroConfirmed,
        electricity_unit_price: Decimal::new(95, 3),
        electricity_tax_rate: Decimal::new(16, 2),
        water_tariff: None,
        water_tax_rate: None,
        currency: "USD".into(),
    })
    .expect("valid zero-water utility invoice");

    assert_eq!(result.lines.len(), 2);
    assert_eq!(result.lines[0].service, ServiceCode::Electricity);
    assert_eq!(result.lines[1].service, ServiceCode::Water);
    assert_eq!(result.lines[1].quantity, Decimal::ZERO);
    assert_eq!(result.lines[1].total, Decimal::ZERO);
    assert_eq!(result.total.to_string(), "11.02");
}
