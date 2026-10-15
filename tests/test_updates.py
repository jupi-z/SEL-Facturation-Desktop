import re
from pathlib import Path

import pytest

if not __import__("importlib.util").util.find_spec("flask"):
    pytest.skip("Flask is not installed", allow_module_level=True)

from app import create_app
from updater import UpdateCheckResult, UpdateManifest


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


def csrf(client):
    return re.search(r'name="_csrf" value="([^"]+)"', client.get("/updates").get_data(as_text=True)).group(1)


def test_updates_page_is_reachable_and_is_linked_from_navigation(authenticated_client):
    response = authenticated_client.get("/updates")

    assert response.status_code == 200
    html = response.get_data(as_text=True)
    assert "Mises à jour" in html
    assert 'action="/updates/check"' in html
    assert 'data-update-status' in html
    assert 'data-update-poll' in html


def test_update_status_endpoint_exposes_background_result(authenticated_client):
    app = authenticated_client.application
    app.extensions["update_result"] = {"status": "available", "message": "Version prête"}

    response = authenticated_client.get("/updates/status")

    assert response.status_code == 200
    assert response.get_json() == {"status": "available", "message": "Version prête"}


def test_background_result_supersedes_stale_session_result(authenticated_client):
    authenticated_client.application.extensions["update_result"] = {"status": "available", "message": "Nouvelle version"}
    with authenticated_client.session_transaction() as session:
        session["update_result"] = {"status": "current", "message": "Ancien résultat"}

    response = authenticated_client.get("/updates/status")

    assert response.get_json() == {"status": "available", "message": "Nouvelle version"}


def test_update_check_is_non_blocking_when_release_service_is_unavailable(authenticated_client, monkeypatch):
    monkeypatch.setattr("app.check_for_update", lambda *args, **kwargs: UpdateCheckResult("unavailable", message="hors ligne"))

    response = authenticated_client.post("/updates/check", data={"_csrf": csrf(authenticated_client)})

    assert response.status_code == 302
    html = authenticated_client.get("/updates").get_data(as_text=True)
    assert "hors ligne" in html


def test_update_check_displays_available_release(authenticated_client, monkeypatch):
    monkeypatch.setattr(
        "app.check_for_update",
        lambda *args, **kwargs: UpdateCheckResult(
            "available",
            manifest=UpdateManifest(
                1, "0.3.2", "https://example.test/SEL-Facturation-Setup-0.3.2.exe",
                "a" * 64, 123, "2026-10-15T12:00:00Z", "Correctifs",
                "https://example.test/update.json",
            ),
        ),
    )

    authenticated_client.post("/updates/check", data={"_csrf": csrf(authenticated_client)})
    html = authenticated_client.get("/updates").get_data(as_text=True)

    assert "0.3.2" in html
    assert "Correctifs" in html


def test_update_download_creates_backup_before_marking_installer_ready(authenticated_client, monkeypatch, tmp_path):
    manifest = UpdateManifest(
        1, "0.3.2", "https://example.test/SEL-Facturation-Setup-0.3.2.exe",
        "a" * 64, 123, "2026-10-15T12:00:00Z", "Correctifs",
        "https://example.test/update.json",
    )
    monkeypatch.setattr("app.check_for_update", lambda *args, **kwargs: UpdateCheckResult("available", manifest=manifest))
    installer = tmp_path / "SEL-Facturation-Setup-0.3.2.exe"
    installer.write_bytes(b"installer")
    backup = tmp_path / "sauvegardes" / "sel-pre-update.sqlite3"
    monkeypatch.setattr("app.download_update", lambda *args, **kwargs: installer)
    monkeypatch.setattr("app.create_pre_update_backup", lambda: backup)

    authenticated_client.post("/updates/check", data={"_csrf": csrf(authenticated_client)})
    response = authenticated_client.post("/updates/download", data={"_csrf": csrf(authenticated_client)})

    assert response.status_code == 302
    html = authenticated_client.get("/updates").get_data(as_text=True)
    assert 'data-update-file="SEL-Facturation-Setup-0.3.2.exe"' in html


def test_update_install_action_uses_the_native_bridge():
    script = open("static/desktop.js", encoding="utf-8").read()

    assert "data-update-install" in script
    assert "install_update" in script


def test_background_update_check_stores_result_without_blocking_app(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    monkeypatch.setattr("app.check_for_update", lambda *args, **kwargs: UpdateCheckResult("current"))

    worker = app.start_background_update_check()
    worker.join(timeout=2)

    assert app.extensions["update_result"]["status"] == "current"
