use rust_decimal::Decimal;

use crate::money::money;

/// The only VAT arithmetic used by utility billing, solar sales and financial
/// documents. The approved rate itself is read from the central versioned
/// configuration; immutable document snapshots retain the applied rate.
pub fn calculate_vat(taxable_base: Decimal, rate: Decimal) -> Decimal {
    money(taxable_base * rate)
}
