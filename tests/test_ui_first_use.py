import re

import pytest

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


def test_unconfigured_dashboard_makes_configuration_the_primary_next_step(authenticated_client):
    html = authenticated_client.get("/").get_data(as_text=True)

    assert 'href="/settings?tab=tariffs"' in html
    assert "Terminer la configuration" in html
    assert '+ Créer un décompte' not in html


def test_invoice_form_explains_both_blocking_prerequisites(authenticated_client):
    html = authenticated_client.get("/invoices/new").get_data(as_text=True)

    assert "Validez d’abord les tarifs et les taxes" in html
    assert 'href="/settings?tab=tariffs"' in html
    assert "Ajoutez d’abord un abonné" in html
    assert 'href="/clients"' in html
    assert 'aria-describedby="invoice-preview-help"' in html
    assert 'id="invoice-preview-help"' in html
