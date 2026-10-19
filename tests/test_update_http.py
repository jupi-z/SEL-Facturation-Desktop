import importlib.util
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed", allow_module_level=True)

from app import create_app
from version import APP_VERSION


def csrf(client, path):
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


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


def test_update_status_is_explicitly_not_configured_without_manifest(authenticated_client, monkeypatch):
    monkeypatch.delenv("SEL_UPDATE_MANIFEST_URL", raising=False)

    response = authenticated_client.get("/updates/status")

    assert response.status_code == 200
    assert response.json["version"] == APP_VERSION


def test_updates_page_is_reachable_and_linked_from_navigation(authenticated_client):
    response = authenticated_client.get("/updates")

    assert response.status_code == 200
    html = response.get_data(as_text=True)
    assert "Mises à jour" in html
    assert 'action="/updates/check"' in html
    assert 'href="/updates"' in html


def test_update_status_uses_sel_default_manifest_endpoint(authenticated_client, monkeypatch):
    monkeypatch.delenv("SEL_UPDATE_MANIFEST_URL", raising=False)
    calls = []

    def fake_fetch(url, *, current_version):
        calls.append((url, current_version))
        return {"state": "current", "version": current_version, "reason": "already_current"}

    monkeypatch.setattr("app.fetch_manifest", fake_fetch)

    response = authenticated_client.get("/updates/status")

    assert response.status_code == 200
    assert response.json["state"] == "current"
    assert calls == [("https://sel.shuleops.com/updates/latest.json", APP_VERSION)]


def test_update_check_renders_current_state(authenticated_client, monkeypatch):
    monkeypatch.setattr(
        "app.fetch_manifest",
        lambda url, *, current_version: {"state": "current", "version": current_version},
    )
    response = authenticated_client.post(
        "/updates/check",
        data={"_csrf": csrf(authenticated_client, "/updates")},
        follow_redirects=True,
    )

    assert response.status_code == 200
    assert "dernière version" in response.get_data(as_text=True)
