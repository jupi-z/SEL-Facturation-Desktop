import importlib.util
import json
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed in this offline test environment", allow_module_level=True)

from app import create_app
from db import connect


@pytest.fixture
def authenticated_client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    app.config["TESTING"] = True
    client = app.test_client()
    setup = client.get("/setup")
    token = re.search(r'name="_csrf" value="([^"]+)"', setup.get_data(as_text=True)).group(1)
    client.post(
        "/setup",
        data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"},
    )
    login = client.get("/login")
    token = re.search(r'name="_csrf" value="([^"]+)"', login.get_data(as_text=True)).group(1)
    client.post("/login", data={"_csrf": token, "password": "TestPassword2026!"})
    return client


def test_dashboard_explains_configuration_and_data_protection(authenticated_client):
    response = authenticated_client.get("/")
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert "Configuration à terminer" in html
    assert "Sauvegarde" in html
    assert "Aucune facture émise" in html
    assert "Montant encaissé" in html
    assert "Solde à suivre" in html


def test_dashboard_does_not_call_issued_amount_an_estimation(authenticated_client):
    html = authenticated_client.get("/").get_data(as_text=True)

    assert "ESTIMATION" not in html
    assert "Montant émis" in html


def test_dashboard_paid_amount_uses_current_currency(authenticated_client):
    with connect() as db:
        db.execute("INSERT INTO clients(code,name,address,meter_number) VALUES(?,?,?,?)", ("SEL-C-00001", "Abonné", "Kolwezi", "M1"))
        client_id = db.execute("SELECT id FROM clients WHERE code='SEL-C-00001'").fetchone()[0]
        calc = {"total": "100.00"}
        for number, currency, amount in (("SEL-2026-000001", "CDF", "100.00"), ("SEL-2026-000002", "USD", "999.00")):
            db.execute("INSERT INTO invoices(number,client_id,billing_period,issue_date,client_snapshot,company_snapshot,calc_snapshot,currency) VALUES(?,?,?,?,?,?,?,?)", (number, client_id, number[-2:], "2026-10-15", "{}", "{}", json.dumps(calc), currency))
            invoice_id = db.execute("SELECT id FROM invoices WHERE number=?", (number,)).fetchone()[0]
            db.execute("INSERT INTO payments(invoice_id,amount,method,reference,paid_on) VALUES(?,?,?,?,?)", (invoice_id, amount, "Espèces", "", "2026-10-15"))
    html = authenticated_client.get("/").get_data(as_text=True)

    assert "100,00" in html
    assert "1 099,00" not in html
