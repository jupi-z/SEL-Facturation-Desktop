import importlib.util
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed in this offline test environment", allow_module_level=True)

from app import create_app


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


@pytest.mark.parametrize(
    ("path", "label"),
    [
        ("/", "Vue d'ensemble"),
            ("/clients", "Abonnés"),
        ("/invoices/new", "Nouveau décompte"),
        ("/invoices", "Historique des décomptes"),
        ("/settings", "Paramètres de facturation"),
    ],
)
def test_authenticated_navigation_has_one_active_task(authenticated_client, path, label):
    response = authenticated_client.get(path)
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert f'aria-current="page"' in html
    assert html.count('aria-current="page"') == 1
    assert label in html


def test_connected_pages_use_stable_billing_vocabulary(authenticated_client):
    dashboard = authenticated_client.get("/").get_data(as_text=True)
    invoice_form = authenticated_client.get("/invoices/new").get_data(as_text=True)

    assert "Clients et compteurs" not in dashboard
    assert "Nouvelle facture" not in invoice_form
    assert "Nouveau décompte de consommation" in invoice_form
