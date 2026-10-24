import importlib.util

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is unavailable", allow_module_level=True)

import re
from app import create_app
import db


def _authenticated_client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    setup = client.get("/setup").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', setup).group(1)
    client.post("/setup", data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    login = client.get("/login").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', login).group(1)
    client.post("/login", data={"_csrf": token, "password": "TestPassword2026!"})
    return client


def test_new_invoice_ui_uses_direct_kwh_and_no_local_tax(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/invoices/new").get_data(as_text=True)
    assert 'name="consumption_kwh"' in html
    assert "Taxe locale" not in html


def test_client_ui_has_controlled_customer_type(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/clients").get_data(as_text=True)
    assert 'name="customer_type"' in html
    assert 'value="RESIDENTIAL"' in html
    assert 'value="COMMERCIAL"' in html


def test_customer_type_options_are_read_from_configuration(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO customer_types(code,label,active,sort_order) VALUES(?,?,1,?)",
            ("INDUSTRIAL", "Industriel", 3),
        )
        connection.commit()

    html = client.get("/clients").get_data(as_text=True)
    settings = client.get("/settings").get_data(as_text=True)
    assert 'value="INDUSTRIAL"' in html
    assert 'name="tariff_rate_INDUSTRIAL"' in settings


def test_dynamic_tariff_is_saved_for_configured_customer_type(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO customer_types(code,label,active,sort_order) VALUES(?,?,1,?)",
            ("INDUSTRIAL", "Industriel", 3),
        )
        connection.commit()
    html = client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)
    response = client.post("/settings", data={
        "_csrf": token, "section": "tariffs",
        "tariff_rate_RESIDENTIAL": "0.095",
        "tariff_rate_COMMERCIAL": "0.12",
        "tariff_rate_INDUSTRIAL": "0.15",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        row = connection.execute(
            "SELECT unit_price FROM tariff_plans WHERE customer_type='INDUSTRIAL' "
            "ORDER BY effective_from DESC LIMIT 1"
        ).fetchone()
    assert row["unit_price"] == "0.15"


def test_admin_can_add_an_active_customer_type_with_its_initial_tariff(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)
    response = client.post("/settings", data={
        "_csrf": token, "section": "customer_type", "code": "PUBLIC_SERVICE",
        "label": "Service public", "initial_rate": "0.13", "active": "yes", "confirmed": "yes",
    })
    assert response.status_code == 302
    assert 'value="PUBLIC_SERVICE"' in client.get("/clients").get_data(as_text=True)
    with db.connect() as connection:
        row = connection.execute(
            "SELECT unit_price FROM tariff_plans WHERE customer_type='PUBLIC_SERVICE'"
        ).fetchone()
    assert row["unit_price"] == "0.13"


def test_configured_future_customer_type_can_be_billed(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)
    client.post("/settings", data={
        "_csrf": token, "section": "tariffs", "tariff_rate_RESIDENTIAL": "0.095",
        "tariff_rate_COMMERCIAL": "0.12", "exchange_rate_usd_cdf": "2360",
        "vat_rate": "16", "confirmed": "yes",
    })
    token = re.search(r'name="_csrf" value="([^"]+)"', client.get("/settings").get_data(as_text=True)).group(1)
    client.post("/settings", data={
        "_csrf": token, "section": "customer_type", "code": "INDUSTRIAL",
        "label": "Industriel", "initial_rate": "0.15", "active": "yes", "confirmed": "yes",
    })
    client.post("/clients", data={
        "_csrf": re.search(r'name="_csrf" value="([^"]+)"', client.get("/clients").get_data(as_text=True)).group(1),
        "name": "Client industriel", "address": "Kolwezi", "customer_type": "INDUSTRIAL",
        "contract_number": "I-1", "meter_number": "", "coefficient": "1",
    })
    response = client.post("/invoices/new", data={
        "_csrf": re.search(r'name="_csrf" value="([^"]+)"', client.get("/invoices/new").get_data(as_text=True)).group(1),
        "client_id": "1", "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes",
    })
    assert response.status_code == 200
    assert "15,00" in response.get_data(as_text=True)


def test_tariff_save_updates_the_effective_exchange_rate_catalog(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)
    response = client.post("/settings", data={
        "_csrf": token, "section": "tariffs",
        "tariff_rate_RESIDENTIAL": "0.095", "tariff_rate_COMMERCIAL": "0.12",
        "exchange_rate_usd_cdf": "2500", "vat_rate": "16", "confirmed": "yes",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        row = connection.execute(
            "SELECT rate FROM exchange_rates WHERE base_currency='USD' AND quote_currency='CDF' "
            "ORDER BY effective_on DESC, id DESC LIMIT 1"
        ).fetchone()
    assert row["rate"] == "2500"


def test_operations_policy_can_be_saved_from_settings(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    html = client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)
    response = client.post("/settings", data={
        "_csrf": token, "section": "operations", "invoice_prefix": "DOC",
        "invoice_sequence_width": "4", "backup_retention_count": "7",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        values = dict(connection.execute("SELECT key,value FROM billing_policy"))
    assert values["invoice_prefix"] == "DOC"
    assert values["invoice_sequence_width"] == "4"
    assert values["backup_retention_count"] == "7"
