import re
import time

import pytest

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


def wait_state(client, expected):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        result = client.get("/updates/status").json
        if result["state"] == expected:
            return result
        time.sleep(0.01)
    return result


def test_update_status_is_asynchronous_and_reports_current(authenticated_client):
    updater = authenticated_client.application.extensions["safeupdate"]
    updater.fetcher = lambda *args, **kwargs: {"state": "current", "version": APP_VERSION, "reason": "already_current", "manifest_trusted": True}
    first = authenticated_client.get("/updates/status")
    assert first.status_code == 200
    assert first.json["version"] == APP_VERSION
    assert wait_state(authenticated_client, "current")["state"] == "current"


def test_updates_page_is_reachable_and_linked_from_navigation(authenticated_client):
    updater = authenticated_client.application.extensions["safeupdate"]
    updater.fetcher = lambda *args, **kwargs: {"state": "current", "version": APP_VERSION}
    response = authenticated_client.get("/updates")
    assert response.status_code == 200
    html = response.get_data(as_text=True)
    assert "Mises à jour" in html
    assert 'action="/updates/check"' in html
    assert 'href="/updates"' in html


def test_invalid_channel_is_not_presented_as_a_download(authenticated_client):
    updater = authenticated_client.application.extensions["safeupdate"]
    updater.fetcher = lambda *args, **kwargs: (_ for _ in ()).throw(ValueError("manifest signature invalid"))
    authenticated_client.get("/updates/status")
    result = wait_state(authenticated_client, "invalid")
    assert "cryptographique" in result["message"]


def test_update_check_only_starts_work_and_never_downloads(authenticated_client):
    updater = authenticated_client.application.extensions["safeupdate"]
    calls = []
    updater.fetcher = lambda *args, **kwargs: calls.append(True) or {"state": "current", "version": APP_VERSION}
    token = csrf(authenticated_client, "/updates")
    if updater._worker:
        updater._worker.join(1)
    calls.clear()
    updater._set(state="idle", phase="idle")
    response = authenticated_client.post("/updates/check", data={"_csrf": token}, follow_redirects=True)
    assert response.status_code == 200
    assert wait_state(authenticated_client, "current")["state"] == "current"
    assert calls == [True]
