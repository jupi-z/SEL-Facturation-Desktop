import pytest

from domain import BillingError, calculate_flat_rate, calculate_from_readings, convert_amount


def test_residential_flat_rate_is_calculated_before_vat():
    result = calculate_flat_rate(consumption_kwh="100", unit_price="0.095", vat_rate="16")
    assert result["energy"] == "9.50"
    assert result["vat"] == "1.52"
    assert result["total"] == "11.02"
    assert result["local_tax"] == "0.00"
    assert result["currency"] == "USD"


def test_commercial_flat_rate_is_calculated_before_vat():
    result = calculate_flat_rate(consumption_kwh="100", unit_price="0.12", vat_rate="16")
    assert result["energy"] == "12.00"
    assert result["vat"] == "1.92"
    assert result["total"] == "13.92"


def test_direct_consumption_does_not_require_meter_readings():
    result = calculate_flat_rate(consumption_kwh="12.5", unit_price="0.095")
    assert result["kwh"] == "12.5"


def test_readings_are_supported_as_an_optional_future_mode():
    result = calculate_from_readings(previous="10", current="15", coefficient="2", unit_price="0.095")
    assert result["kwh"] == "10"
    assert result["energy"] == "0.95"


def test_decreasing_reading_is_rejected():
    with pytest.raises(BillingError, match="inférieur"):
        calculate_from_readings(previous="15", current="10", unit_price="0.095")


def test_cdf_conversion_keeps_the_usd_source_amount():
    result = convert_amount("11.02", "2360")
    assert result == {"source_amount": "11.02", "source_currency": "USD", "rate": "2360", "currency": "CDF", "amount": "26007.20"}
