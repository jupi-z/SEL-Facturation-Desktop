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
