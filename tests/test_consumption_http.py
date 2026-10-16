import re

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
    variant = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF&rate=2360")
    assert variant.status_code == 200 and variant.data.startswith(b"%PDF-")
