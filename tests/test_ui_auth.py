import importlib.util
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed in this offline test environment", allow_module_level=True)

from app import create_app


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    app.config["TESTING"] = True
    return app.test_client()


@pytest.fixture
def configured_client(client):
    page = client.get("/setup")
    token = re.search(r'name="_csrf" value="([^"]+)"', page.get_data(as_text=True)).group(1)
    response = client.post(
        "/setup",
        data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"},
    )
    assert response.status_code == 302
    return client


def test_login_uses_auth_shell_without_connected_chrome(configured_client):
    response = configured_client.get("/login")
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert 'class="auth-shell"' in html
    assert 'class="side"' not in html
    assert 'class="top"' not in html
    assert 'class="auth-form"' in html
    assert 'class="panel narrow"' not in html
    assert 'type="button"' in html


def test_setup_uses_same_auth_shell(client):
    response = client.get("/setup")
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert 'class="auth-shell"' in html
    assert 'class="auth-form"' in html
    assert 'class="side"' not in html
    assert 'class="top"' not in html
    assert 'id="password"' in html
    assert 'id="confirm"' in html


def test_login_error_is_announced_near_the_auth_form(configured_client):
    page = configured_client.get("/login")
    token = re.search(r'name="_csrf" value="([^"]+)"', page.get_data(as_text=True)).group(1)

    response = configured_client.post("/login", data={"_csrf": token, "password": "incorrect"})
    html = response.get_data(as_text=True)

    assert response.status_code == 200
    assert 'role="alert"' in html
