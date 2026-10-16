import importlib.util
import json
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed in this offline test environment", allow_module_level=True)

from app import create_app
from db import connect


@pytest.fixture
def prepared_client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    app.config["TESTING"] = True
    client = app.test_client()

    def token(path):
        page = client.get(path)
        return re.search(r'name="_csrf" value="([^"]+)"', page.get_data(as_text=True)).group(1)

    csrf = token("/setup")
    client.post("/setup", data={"_csrf": csrf, "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    csrf = token("/login")
    client.post("/login", data={"_csrf": csrf, "password": "TestPassword2026!"})
    csrf = token("/settings")
    client.post("/settings", data={"_csrf": csrf, "section": "tariffs", "residential_rate": "0.095", "commercial_rate": "0.12", "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes"})
    csrf = token("/clients")
    client.post("/clients", data={"_csrf": csrf, "name": "Abonné A", "address": "Kolwezi", "customer_type": "RESIDENTIAL", "meter_number": "", "coefficient": "1", "contract_number": "C1"})
    return client, token


def invoice_data(csrf):
    return {"_csrf": csrf, "client_id": "1", "billing_period": "2026-08", "consumption_kwh": "200", "due_date": "2026-10-31"}


def test_invoice_entry_shows_preview_without_creating_document(prepared_client):
    client, token = prepared_client
    response = client.post("/invoices/new", data=invoice_data(token("/invoices/new")))
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert "Vérifier le décompte" in html
    assert "Émettre définitivement" in html
    assert "22,04" in html
    with connect() as db:
        assert db.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0


def test_invoice_confirmation_creates_one_document_after_preview(prepared_client):
    client, token = prepared_client
    preview = client.post("/invoices/new", data=invoice_data(token("/invoices/new")))
    csrf = re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1)

    response = client.post("/invoices/new/confirm", data={"_csrf": csrf})

    assert response.status_code == 302
    with connect() as db:
        assert db.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 1


def test_invalid_invoice_preview_does_not_consume_sequence(prepared_client):
    client, token = prepared_client
    data = invoice_data(token("/invoices/new"))
    data["consumption_kwh"] = "not-a-number"

    response = client.post("/invoices/new", data=data)

    assert response.status_code == 200
    assert "Consommation" in response.get_data(as_text=True)
    with connect() as db:
        assert db.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0
        assert db.execute("SELECT COUNT(*) FROM sequences").fetchone()[0] == 0


def test_invalid_resubmission_clears_previous_preview(prepared_client):
    client, token = prepared_client
    valid = invoice_data(token("/invoices/new"))
    client.post("/invoices/new", data=valid)
    invalid = dict(valid, consumption_kwh="not-a-number", _csrf=token("/invoices/new"))
    client.post("/invoices/new", data=invalid)
    csrf = re.search(r'name="_csrf" value="([^"]+)"', client.get("/invoices/new").get_data(as_text=True)).group(1)

    response = client.post("/invoices/new/confirm", data={"_csrf": csrf})

    assert response.status_code == 302
    assert response.location.endswith("/invoices/new")
    with connect() as db:
        assert db.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0
