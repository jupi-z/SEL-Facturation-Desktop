import importlib.util
import re

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed", allow_module_level=True)

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


def test_update_status_is_explicitly_not_configured_without_manifest(authenticated_client, monkeypatch):
    monkeypatch.delenv("SEL_UPDATE_MANIFEST_URL", raising=False)

    response = authenticated_client.get("/updates/status")

    assert response.status_code == 200
    assert response.json == {"state": "not_configured", "version": "0.3.1"}
