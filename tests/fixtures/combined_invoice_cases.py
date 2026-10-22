"""Synthetic-only combined invoice fixtures for PDF review.

The water values in this module are deliberately fictitious and must never be
used as production defaults or presented as SEL-approved tariffs.
"""
import json

from domain import build_combined_snapshot


def combined_snapshot():
    electricity = {
        "code": "ELECTRICITY", "mode": "DIRECT_KWH", "quantity": "391.64", "unit": "kWh",
        "unit_price_excl_tax": "0.095",
        "lines": [{"label": "Consommation électrique — fixture", "quantity": "391.64", "unit_price": "0.095", "amount": "37.21"}],
        "taxes": [{"code": "VAT", "rate": "16", "basis": "37.21"}],
    }
    water = {
        "code": "WATER", "mode": "DIRECT_M3", "quantity": "25", "unit": "m3",
        "unit_price_excl_tax": "0.40",
        "lines": [{"label": "Consommation d'eau — fixture", "quantity": "25", "unit_price": "0.40", "amount": "10.00"}],
        "taxes": [],
    }
    return build_combined_snapshot(electricity, water, "USD")


def invoice(*, number="SEL-SYNTH-0001", status="ISSUED", note="", name="Abonné de test"):
    return {
        "number": number,
        "issue_date": "2026-10-20",
        "due_date": "2026-10-31",
        "billing_period": "2026-10",
        "currency": "USD",
        "status": status,
        "note": note,
        "client_snapshot": json.dumps({
            "name": name, "code": "SEL-SYNTH-001", "address": "Adresse fictive — recette",
            "contract_number": "CONTRAT-SYNTH", "nif": "", "center": "KOLWEZI",
            "service_point": "POINT-SYNTH", "category": "RESIDENTIAL",
        }, ensure_ascii=False),
        "company_snapshot": json.dumps({
            "company_name": "SOCIÉTÉ D’ÉNERGIE DU LUALABA",
            "address": "Coordonnées fictives — recette",
        }, ensure_ascii=False),
        "calc_snapshot": json.dumps(combined_snapshot(), ensure_ascii=False),
    }
