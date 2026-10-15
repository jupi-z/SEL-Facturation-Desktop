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


def test_login_headline_keeps_two_desktop_lines():
    html = open("templates/base.html", encoding="utf-8").read()

    assert "Une facturation claire,<br>sur votre poste SEL." in html
    assert "auth-brand-copy" in html


def test_client_directory_uses_modal_for_creation(authenticated_client):
    html = authenticated_client.get("/clients").get_data(as_text=True)

    assert '<dialog id="client-create-modal"' in html
    assert 'aria-modal="true"' in html
    assert 'data-modal-open="client-create-modal"' in html
    assert '<form method="post" class="form">' in html


def test_settings_uses_separate_edit_modals(authenticated_client):
    html = authenticated_client.get("/settings").get_data(as_text=True)

    assert '<dialog id="identity-settings-modal"' in html
    assert '<dialog id="tariff-settings-modal"' in html
    assert 'data-modal-open="identity-settings-modal"' in html
    assert 'data-modal-open="tariff-settings-modal"' in html


def test_identity_settings_can_be_saved_without_reconfirming_tariffs(authenticated_client):
    html = authenticated_client.get("/settings").get_data(as_text=True)
    token = re.search(r'name="_csrf" value="([^"]+)"', html).group(1)

    response = authenticated_client.post(
        "/settings",
        data={
            "_csrf": token,
            "section": "identity",
            "company_name": "SEL TEST",
            "tagline": "Énergie locale",
            "address": "Kolwezi",
            "phone": "",
            "email": "",
            "website": "",
            "rccm": "",
            "nif": "",
            "id_nat": "",
        },
    )

    assert response.status_code == 302
    assert "SEL TEST" in authenticated_client.get("/settings").get_data(as_text=True)


def test_modal_styles_and_keyboard_behavior_are_present():
    css = open("static/app.css", encoding="utf-8").read()
    js = open("static/desktop.js", encoding="utf-8").read()

    assert "dialog.modal" in css
    assert "event.key === 'Escape'" in js
    assert "showModal()" in js
