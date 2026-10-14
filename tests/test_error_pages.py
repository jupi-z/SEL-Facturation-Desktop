import importlib.util

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


def test_missing_page_uses_sel_error_screen(client):
    response = client.get("/page-inexistante")
    html = response.get_data(as_text=True)

    assert response.status_code == 404
    assert "SEL Facturation" in html
    assert "404" in html
    assert "Traceback" not in html


def test_bad_request_uses_sel_error_screen(client):
    response = client.post("/setup", data={})
    html = response.get_data(as_text=True)

    assert response.status_code == 400
    assert "SEL Facturation" in html
    assert "400" in html
    assert "Traceback" not in html
