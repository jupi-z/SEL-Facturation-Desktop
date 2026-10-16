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
    client.post("/setup", data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    login = client.get("/login")
    token = re.search(r'name="_csrf" value="([^"]+)"', login.get_data(as_text=True)).group(1)
    client.post("/login", data={"_csrf": token, "password": "TestPassword2026!"})
    return client


def test_subscriber_form_groups_data_and_has_explicit_labels(authenticated_client):
    html = authenticated_client.get("/clients").get_data(as_text=True)

    assert "Identité de l’abonné" in html
    assert "Contrat et compteur futur" in html
    assert 'for="name"' in html
    assert 'id="name"' in html
    assert 'for="meter_number"' in html
    assert 'id="meter_number"' in html


def test_settings_form_separates_company_from_tariffs(authenticated_client):
    html = authenticated_client.get("/settings").get_data(as_text=True)

    assert "Identité SEL" in html
    assert "Barème et taxes" in html
    assert 'for="company_name"' in html
    assert 'id="company_name"' in html
    assert 'for="residential_rate"' in html
    assert 'id="residential_rate"' in html
