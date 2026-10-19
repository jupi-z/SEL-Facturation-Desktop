import importlib.util

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is unavailable", allow_module_level=True)

import re
from app import create_app


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
