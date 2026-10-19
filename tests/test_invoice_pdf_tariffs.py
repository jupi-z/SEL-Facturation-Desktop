import base64
import json
from io import BytesIO

from pypdf import PdfReader

from invoice_pdf import render_invoice


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
