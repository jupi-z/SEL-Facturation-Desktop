import importlib.util
import re
from pathlib import Path

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed in this offline test environment", allow_module_level=True)

from app import create_app

ROOT = Path(__file__).resolve().parents[1]


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
    assert "Contrat et compteur futur" not in html
    assert "Coefficient futur" not in html
    assert "N° du compteur" not in html
    assert "Abonnés et compteurs" not in html
    assert 'for="name"' in html
    assert 'id="name"' in html


def test_settings_form_separates_company_from_tariffs(authenticated_client):
    html = authenticated_client.get("/settings").get_data(as_text=True)

    assert "Identité SEL" in html
    assert "Barème et taxes" in html
    assert 'for="company_name"' in html
    assert 'id="company_name"' in html
    assert 'for="tariff_rate_RESIDENTIAL"' in html
    assert 'id="tariff_rate_RESIDENTIAL"' in html


def test_official_tariff_form_does_not_expose_legacy_tier_or_local_tax_fields(authenticated_client):
    html = authenticated_client.get("/settings").get_data(as_text=True)

    assert 'name="tiers"' not in html
    assert 'name="local_rate"' not in html
    assert 'name="vat_base"' not in html


def test_zero_water_confirmation_is_grouped_with_its_water_field():
    template = (ROOT / "templates" / "new_invoice.html").read_text(encoding="utf-8")
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    water_group = re.search(
        r'<div id="water-consumption-field" class="field-stack water-field">(.*?)<label for="due_date">',
        template,
        re.DOTALL,
    )
    assert water_group, "The water input and its zero confirmation must share one grid cell."
    group_html = water_group.group(1)
    assert 'id="water_m3"' in group_html
    assert 'id="water_zero_confirmed"' in group_html
    assert 'for="water_zero_confirmed"' in group_html
    assert "Je confirme que le relevé d’eau" in group_html
    assert ".water-zero-confirmation{" in css
