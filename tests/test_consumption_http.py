import re
from datetime import date, timedelta
from io import BytesIO

from pypdf import PdfReader

import app as app_module
from app import create_app
import db
from invoice_pdf import internal_qr_payload


def _token(client, path):
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


def test_monthly_direct_consumption_invoice_without_meter(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("UPDATE settings SET value='CDF' WHERE key='currency'")
        connection.execute("UPDATE clients SET customer_type='RESIDENTIAL' WHERE 1=0")
        connection.commit()
    response = client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client Résidentiel",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-1",
        "meter_number": "", "coefficient": "1",
    })
    assert response.status_code == 302
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "100", "due_date": "2026-10-31",
    })
    assert preview.status_code == 200
    assert "11,02" in preview.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        row = connection.execute("SELECT id,currency FROM invoices").fetchone()
        invoice_id = row[0]
        assert row[1] == "USD"
    with db.connect() as connection:
        connection.execute("UPDATE exchange_rates SET rate='2500' WHERE base_currency='USD' AND quote_currency='CDF'")
        connection.commit()
    variant = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF")
    assert variant.status_code == 200 and variant.data.startswith(b"%PDF-")
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(variant.data)).pages)
    assert "2500" in text

    captured = {}
    def capture_render(invoice, payments, qr_reference=None):
        captured["currency"] = invoice["currency"]
        captured["qr_reference"] = qr_reference
        return b"%PDF-test"
    monkeypatch.setattr(app_module, "render_invoice", capture_render)
    converted = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF&rate=2600")
    assert converted.status_code == 200
    assert captured["currency"] == "CDF"
    qr_invoice, qr_calc = captured["qr_reference"]
    assert qr_invoice["currency"] == "USD"
    assert '"currency":"USD"' in internal_qr_payload(qr_invoice, qr_calc)


def test_current_month_invoice_still_works_after_mid_month_tariff_save(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})

    settings = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariffs",
        "residential_rate": "0.095", "commercial_rate": "0.12",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    assert settings.status_code == 302

    client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client du mois",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-2",
        "meter_number": "", "coefficient": "1",
    })
    period = date.today().strftime("%Y-%m")
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": period, "consumption_kwh": "100",
        "due_date": date.today().replace(day=28).isoformat(),
    })

    assert preview.status_code == 200
    assert "11,02" in preview.get_data(as_text=True)


def test_invoice_accepts_a_past_due_date_for_late_billing(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariffs",
        "residential_rate": "0.095", "commercial_rate": "0.12",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client échéance passée",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-PAST",
        "meter_number": "", "coefficient": "1",
    })
    past_due = (date.today() - timedelta(days=1)).isoformat()
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": date.today().strftime("%Y-%m"), "consumption_kwh": "100", "due_date": past_due,
    })

    assert preview.status_code == 200
    assert f"Échéance : {past_due}" in preview.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        assert connection.execute("SELECT due_date FROM invoices").fetchone()[0] == past_due
