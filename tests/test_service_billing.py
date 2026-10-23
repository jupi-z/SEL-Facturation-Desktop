import pytest

from domain import BillingError, calculate_service, combine_services


def test_water_service_uses_cubic_meters_and_explicit_tax_rules():
    result = calculate_service(
        service_code="WATER", quantity="10", unit="M3", unit_price="0.40",
        taxes={"vat_rate": "16", "vat_base": "SUBTOTAL", "local_rate": "0"},
        currency="USD",
    )

    assert result["quantity"] == "10"
    assert result["unit"] == "M3"
    assert result["subtotal"] == "4.00"
    assert result["vat"] == "0.64"
    assert result["total"] == "4.64"
    assert result["lines"][0]["amount"] == "4.00"


def test_zero_water_consumption_keeps_a_zero_line_and_configured_unit_price():
    result = calculate_service(
        service_code="WATER", quantity="0", unit="M3", unit_price="0.40",
        taxes={"vat_rate": "0", "vat_base": "NONE", "local_rate": "0"},
        currency="USD",
    )

    assert result["quantity"] == "0"
    assert result["lines"] == [{
        "label": "Consommation d’eau", "quantity": "0", "unit": "M3",
        "unit_price": "0.40", "amount": "0.00",
    }]
    assert result["total"] == "0.00"


def test_service_calculation_rejects_missing_tax_rules_and_mismatched_unit():
    with pytest.raises(BillingError, match="fiscale"):
        calculate_service(
            service_code="WATER", quantity="1", unit="M3", unit_price="1",
            taxes=None, currency="USD",
        )
    with pytest.raises(BillingError, match="unité"):
        calculate_service(
            service_code="WATER", quantity="1", unit="KWH", unit_price="1",
            taxes={"vat_rate": "0", "vat_base": "NONE", "local_rate": "0"},
            currency="USD",
        )


@pytest.mark.parametrize("quantity", ["-1", "NaN", "Infinity", "abc"])
def test_service_calculation_rejects_invalid_quantities(quantity):
    with pytest.raises(BillingError):
        calculate_service(
            service_code="WATER", quantity=quantity, unit="M3", unit_price="1",
            taxes={"vat_rate": "0", "vat_base": "NONE", "local_rate": "0"},
            currency="USD",
        )


def test_service_composition_requires_one_currency_and_reconciles_totals():
    electricity = calculate_service(
        service_code="ELECTRICITY", quantity="100", unit="KWH", unit_price="0.095",
        taxes={"vat_rate": "16", "vat_base": "SUBTOTAL", "local_rate": "0"},
        currency="USD",
    )
    water = calculate_service(
        service_code="WATER", quantity="10", unit="M3", unit_price="0.40",
        taxes={"vat_rate": "16", "vat_base": "SUBTOTAL", "local_rate": "0"},
        currency="USD",
    )

    invoice = combine_services([electricity, water], currency="USD")
    assert invoice["subtotal"] == "13.50"
    assert invoice["vat"] == "2.16"
    assert invoice["total"] == "15.66"
    assert [item["service_code"] for item in invoice["services"]] == ["ELECTRICITY", "WATER"]

    with pytest.raises(BillingError, match="devise"):
        combine_services([electricity, {**water, "currency": "CDF"}], currency="USD")


def test_service_amount_rounding_uses_half_up():
    result = calculate_service(
        service_code="WATER", quantity="1", unit="M3", unit_price="0.005",
        taxes={"vat_rate": "0", "vat_base": "NONE", "local_rate": "0"},
        currency="USD",
    )
    assert result["subtotal"] == "0.01"
