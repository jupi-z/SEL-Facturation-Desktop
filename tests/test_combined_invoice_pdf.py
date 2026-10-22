import json
from io import BytesIO

import pytest
from pypdf import PdfReader

from domain import build_combined_snapshot, convert_combined_snapshot
from invoice_pdf import internal_reference_payload, render_invoice


def _snapshot():
    electricity = {
        "code": "ELECTRICITY", "mode": "DIRECT_KWH", "quantity": "100", "unit": "kWh",
        "unit_price_excl_tax": "0.095",
        "lines": [{"label": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "9.50"}],
        "taxes": [{"code": "VAT", "rate": "16", "basis": "9.50"}],
    }
    water = {
        "code": "WATER", "mode": "DIRECT_M3", "quantity": "10", "unit": "m3",
        "unit_price_excl_tax": "0.40",
        "lines": [{"label": "Consommation d'eau", "quantity": "10", "unit_price": "0.40", "amount": "4.00"}],
        "taxes": [],
    }
    return build_combined_snapshot(electricity, water, "USD")


def _invoice(snapshot=None):
    return {
        "number": "SEL-2026-000003", "issue_date": "2026-10-20", "due_date": "2026-10-31",
        "billing_period": "2026-10", "currency": "USD", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({
            "name": "Abonné Synthétique", "code": "SEL-C-TEST", "address": "Donnée de test",
            "contract_number": "CONTRAT-TEST", "nif": "NIF-TEST", "center": "KOLWEZI",
            "service_point": "POINT-TEST", "category": "RESIDENTIAL",
        }),
        "company_snapshot": json.dumps({"company_name": "SOCIÉTÉ D’ÉNERGIE DU LUALABA", "address": "Kolwezi"}),
        "calc_snapshot": json.dumps(snapshot or _snapshot()),
    }


def test_combined_pdf_is_a4_and_contains_two_service_sections_and_internal_qr():
    pdf = render_invoice(_invoice(), [])
    reader = PdfReader(BytesIO(pdf))
    text = "\n".join(page.extract_text() or "" for page in reader.pages)

    assert len(reader.pages) == 1
    assert float(reader.pages[0].mediabox.width) == pytest.approx(595.2756, abs=0.01)
    assert float(reader.pages[0].mediabox.height) == pytest.approx(841.8898, abs=0.01)
    assert "CONSOMMATION" in text
    assert "LECTRICIT" in text
    assert "EAU" in text
    assert "kWh" in text
    assert "m3" in text or "m³" in text
    assert "Sous-total électricité" in text
    assert "Sous-total eau" in text
    assert "MONTANT TOTAL" in text
    assert "Référence interne SEL" in text
    assert "DGI" not in text
    assert "AUTHENTIFI" not in text


def test_internal_qr_payload_is_minimal_and_stable_after_payment():
    invoice = _invoice()
    before = json.loads(internal_reference_payload(invoice))
    invoice["payment_total"] = "5.00"
    after = json.loads(internal_reference_payload(invoice))

    assert before == after
    assert set(before) == {"kind", "v", "issuer", "number", "issued", "currency", "total"}
    assert before == {
        "kind": "SEL_INTERNAL_REFERENCE", "v": 1, "issuer": "SEL",
        "number": "SEL-2026-000003", "issued": "2026-10-20", "currency": "USD", "total": "15.02",
    }
    assert "Abonné" not in internal_reference_payload(invoice)
    assert "NIF" not in internal_reference_payload(invoice)


def test_internal_qr_payload_stays_on_source_currency_for_presentation_variant():
    invoice = _invoice()
    invoice["currency"] = "CDF"
    invoice["calc_snapshot"] = json.dumps(convert_combined_snapshot(_snapshot(), "2360", "CDF"))
    invoice["_qr_currency"] = "USD"
    invoice["_qr_total"] = "15.02"

    payload = json.loads(internal_reference_payload(invoice))

    assert payload["currency"] == "USD"
    assert payload["total"] == "15.02"


def test_combined_pdf_escapes_user_text_and_keeps_qr_reference():
    invoice = _invoice()
    client = json.loads(invoice["client_snapshot"])
    client["name"] = "<script>alert('x')</script>"
    invoice["client_snapshot"] = json.dumps(client)
    pdf = render_invoice(invoice, [{"amount": "5.00"}])
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(pdf)).pages)

    # The escaped value is rendered as literal text, never interpreted as markup.
    assert "<script>alert('x')</script>" in text
    assert "Référence interne SEL" in text
    assert "MONTANT À RÉGLER" in text
