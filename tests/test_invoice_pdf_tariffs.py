import json
from io import BytesIO

from pypdf import PdfReader

from invoice_pdf import render_invoice


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
