import base64
import json
from io import BytesIO

from pypdf import PdfReader

from invoice_pdf import internal_qr_payload, render_invoice


def test_pdf_uses_full_official_logo_and_direct_billing_layout():
    source = __import__('pathlib').Path(__file__).resolve().parents[1]
    code = (source / 'invoice_pdf.py').read_text(encoding='utf-8')
    assert "'sel-icon.png'" in code
    assert "'sel-official-logo.png'" not in code
    assert "'sel-logo.png'" not in code
    assert "#003078" in code
    assert (source / 'static' / 'sel-icon.png').exists()
    svg = (source / 'packaging' / 'sel-icon-source.svg').read_text(encoding='utf-8')
    embedded_png = base64.b64decode(svg.split('data:image/png;base64,', 1)[1].split('"', 1)[0])
    assert (source / 'static' / 'sel-icon.png').read_bytes() == embedded_png


def test_new_fixed_rate_pdf_shows_type_rate_vat_and_no_local_tax():
    invoice = {
        "number": "SEL-2026-000101", "issue_date": "2026-10-16", "due_date": "2026-10-31",
        "billing_period": "2026-10", "currency": "USD", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({"name": "Client Résidentiel", "address": "Kolwezi", "meter_number": "", "contract_number": "C1"}),
        "company_snapshot": json.dumps({"company_name": "SEL", "address": "Kolwezi", "phone": "", "email": "", "website": "", "rccm": "", "nif": "", "id_nat": ""}),
        "calc_snapshot": json.dumps({"pricing_model": "FLAT_KWH", "customer_type": "RESIDENTIAL", "unit_price": "0.095", "kwh": "100", "lines": [{"range": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "9.50"}], "energy": "9.50", "local_rate": "0", "local_tax": "0.00", "vat_rate": "16", "vat_basis": "9.50", "vat": "1.52", "fee": "0.00", "total": "11.02"}),
    }
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(render_invoice(invoice, []))).pages)
    assert "RESIDENTIAL" in text or "Résidentiel" in text
    assert "0,095" in text or "0.095" in text
    assert "TVA" in text and "16" in text
    assert "Taxe locale" not in text
    assert "INFORMATIONS COMPTEUR" not in text
    assert "N° compteur" not in text
    assert "Coefficient" not in text
    assert "PÉRIODE ET FACTURATION" in text


def test_cdf_variant_pdf_discloses_source_currency_and_rate():
    invoice = {
        "number": "SEL-2026-000102", "issue_date": "2026-10-16", "due_date": "2026-10-31",
        "billing_period": "2026-10", "currency": "CDF", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({"name": "Client", "address": "Kolwezi", "category": "", "meter_number": "", "contract_number": "C1"}),
        "company_snapshot": json.dumps({"company_name": "SEL", "address": "Kolwezi"}),
        "calc_snapshot": json.dumps({"pricing_model": "FLAT_KWH", "customer_type": "RESIDENTIAL", "unit_price": "0.095", "kwh": "100", "lines": [{"range": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "22420.00"}], "energy": "22420.00", "local_rate": "0", "local_tax": "0.00", "vat_rate": "16", "vat_basis": "22420.00", "vat": "3587.20", "fee": "0.00", "total": "26007.20", "conversion_source_currency": "USD", "conversion_rate": "2360"}),
    }
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(render_invoice(invoice, []))).pages)
    assert "2360" in text
    assert "USD" in text and "CDF" in text


def test_pdf_uses_the_snapshot_label_for_a_future_customer_type():
    invoice = {
        "number": "SEL-2026-000103", "issue_date": "2026-10-16", "due_date": "2026-10-31",
        "billing_period": "2026-10", "currency": "USD", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({"name": "Client industriel", "address": "Kolwezi", "meter_number": "", "contract_number": "C1"}),
        "company_snapshot": json.dumps({"company_name": "SEL", "address": "Kolwezi"}),
        "calc_snapshot": json.dumps({"pricing_model": "FLAT_KWH", "customer_type": "INDUSTRIAL", "customer_type_label": "Industriel", "unit_price": "0.15", "kwh": "100", "lines": [{"range": "Consommation", "quantity": "100", "unit_price": "0.15", "amount": "15.00"}], "energy": "15.00", "local_rate": "0", "local_tax": "0.00", "vat_rate": "16", "vat_basis": "15.00", "vat": "2.40", "fee": "0.00", "total": "17.40"}),
    }
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(render_invoice(invoice, []))).pages)
    assert "Industriel" in text


def test_v2_invoice_mentions_both_services_without_adding_water_charges():
    invoice = {
        "number": "SEL-2026-000104", "issue_date": "2026-10-20", "due_date": "2026-10-31",
        "billing_period": "2026-09", "currency": "USD", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({"name": "Ameno Kizele", "address": "Kolwezi", "contract_number": "C1"}),
        "company_snapshot": json.dumps({"company_name": "SEL", "address": "Kolwezi"}),
        "calc_snapshot": json.dumps({"document_template_version": 2, "pricing_model": "FLAT_KWH", "customer_type": "RESIDENTIAL", "customer_type_label": "Résidentiel", "unit_price": "0.095", "kwh": "100", "lines": [{"range": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "9.50"}], "energy": "9.50", "local_rate": "0", "local_tax": "0.00", "vat_rate": "16", "vat_basis": "9.50", "vat": "1.52", "fee": "0.00", "total": "11.02"}),
    }

    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(render_invoice(invoice, []))).pages)

    normalized = " ".join(text.split())
    assert "FACTURE D’EAU ET D’ÉLECTRICITÉ" in normalized


def test_combined_pdf_shows_water_zero_line_and_service_totals():
    invoice = {
        "number": "SEL-2026-000105", "issue_date": "2026-10-20", "due_date": "2026-10-31",
        "billing_period": "2026-09", "currency": "USD", "status": "ISSUED", "note": "",
        "client_snapshot": json.dumps({"name": "Client Eau", "address": "Kolwezi", "contract_number": "C1"}),
        "company_snapshot": json.dumps({"company_name": "SEL", "address": "Kolwezi"}),
        "calc_snapshot": json.dumps({
            "document_template_version": 3, "currency": "USD", "total": "11.02", "customer_type_label": "Résidentiel",
            "services": [
                {"service_code": "ELECTRICITY", "currency": "USD", "quantity": "100", "unit": "KWH", "unit_price": "0.095", "subtotal": "9.50", "local_tax": "0.00", "local_rate": "0", "vat": "1.52", "vat_rate": "16", "total": "11.02", "lines": [{"label": "Consommation d’électricité", "quantity": "100", "unit": "KWH", "unit_price": "0.095", "amount": "9.50"}]},
                {"service_code": "WATER", "currency": "USD", "quantity": "0", "unit": "M3", "unit_price": "0.40", "subtotal": "0.00", "local_tax": "0.00", "local_rate": "0", "vat": "0.00", "vat_rate": "0", "total": "0.00", "lines": [{"label": "Consommation d’eau", "quantity": "0", "unit": "M3", "unit_price": "0.40", "amount": "0.00"}]},
            ],
        }),
    }

    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(render_invoice(invoice, []))).pages)

    assert "Consommation d’eau" in text
    assert "0 M3" in text or "0 m³" in text
    assert "0,40" in text or "0.40" in text
    assert "11,02" in text
    assert "Électricité" in text
    assert text.count("Résidentiel") == 1
    assert "Tarif unitaire HT" not in text
    assert "Prix unitaire" in text and "0,095" in text
    assert "MONTANT TOTAL (USD)" in text
    assert "Référence interne" in text
    assert "DGI" not in text
    assert "Montant eau" not in text


def test_internal_qr_payload_is_compact_and_contains_no_customer_data():
    invoice = {"number": "SEL-2026-000104", "issue_date": "2026-10-20", "currency": "USD"}
    calc = {"total": "11.02"}

    payload = json.loads(internal_qr_payload(invoice, calc))

    assert payload == {
        "kind": "SEL_INTERNAL_REFERENCE", "v": 1, "issuer": "SEL",
        "number": "SEL-2026-000104", "issued": "2026-10-20", "currency": "USD", "total": "11.02",
    }
    assert "customer" not in payload and "client" not in payload and "Kolwezi" not in json.dumps(payload)
