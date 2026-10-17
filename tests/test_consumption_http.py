import re
from datetime import date
from io import BytesIO

from pypdf import PdfReader

from app import create_app
import db


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
