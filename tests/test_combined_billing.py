from decimal import Decimal

import pytest

from domain import BillingError, build_combined_snapshot, convert_combined_snapshot


def _electricity_service():
    return {
        "code": "ELECTRICITY",
        "mode": "DIRECT_KWH",
        "quantity": "100",
        "unit": "kWh",
        "unit_price_excl_tax": "0.095",
        "lines": [{"label": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "9.50"}],
        "taxes": [{"code": "VAT", "rate": "16", "basis": "9.50"}],
    }


def _water_service():
    return {
        "code": "WATER",
        "mode": "DIRECT_M3",
        "quantity": "10",
        "unit": "m3",
        "unit_price_excl_tax": "0.40",
        "lines": [{"label": "Consommation d'eau", "quantity": "10", "unit_price": "0.40", "amount": "4.00"}],
        "taxes": [],
    }


def test_combined_snapshot_has_two_services_and_one_reconciled_total():
    result = build_combined_snapshot(_electricity_service(), _water_service(), "USD")

    assert result["document_kind"] == "COMBINED_WATER_ELECTRICITY"
    assert result["schema_version"] == 2
    assert [service["code"] for service in result["services"]] == ["ELECTRICITY", "WATER"]
    assert result["services"][0]["subtotal_excl_tax"] == "9.50"
    assert result["services"][0]["taxes"][0]["amount"] == "1.52"
    assert result["services"][1]["unit"] == "m3"
    assert result["subtotal_excl_tax"] == "13.50"
    assert result["tax_total"] == "1.52"
    assert result["total"] == "15.02"


def test_combined_snapshot_rejects_unvalidated_water_service():
    water = _water_service()
    water.pop("unit_price_excl_tax")

    with pytest.raises(BillingError, match="[Tt]arif"):
        build_combined_snapshot(_electricity_service(), water, "USD")


def test_combined_snapshot_never_defaults_water_tax():
    water = _water_service()
    water.pop("taxes")

    with pytest.raises(BillingError, match="tax"):
        build_combined_snapshot(_electricity_service(), water, "USD")


def test_combined_snapshot_rejects_negative_water_quantity():
    water = _water_service()
    water["quantity"] = "-1"

    with pytest.raises(BillingError, match="Quantité"):
        build_combined_snapshot(_electricity_service(), water, "USD")


def test_combined_snapshot_conversion_reconciles():
    source = build_combined_snapshot(_electricity_service(), _water_service(), "USD")

    result = convert_combined_snapshot(source, "2360", "CDF")

    assert result["currency"] == "CDF"
    assert result["conversion_source_currency"] == "USD"
    assert result["conversion_rate"] == "2360"
    assert result["services"][0]["subtotal_excl_tax"] == "22420.00"
    assert result["services"][1]["subtotal_excl_tax"] == "9440.00"
    assert result["tax_total"] == "3587.20"
    assert result["total"] == "35447.20"
    assert sum((Decimal(s["total_incl_tax"]) for s in result["services"]), Decimal("0")) == Decimal(result["total"])
