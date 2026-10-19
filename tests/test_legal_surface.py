import importlib.util
import re
import sys
from pathlib import Path

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed", allow_module_level=True)

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from app import create_app  # noqa: E402


@pytest.fixture
def client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    app.config["TESTING"] = True
    return app.test_client()


def _csrf(client, path):
    response = client.get(path)
    assert response.status_code == 200
    match = re.search(r'name="_csrf" value="([^"]+)"', response.get_data(as_text=True))
    assert match
    return match.group(1)


def _authenticated(client):
    token = _csrf(client, "/setup")
    assert client.post(
        "/setup",
        data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"},
    ).status_code == 302
    token = _csrf(client, "/login")
    assert client.post("/login", data={"_csrf": token, "password": "TestPassword2026!"}).status_code == 302


def test_about_is_protected_and_contains_verified_developer_attribution(client):
    assert client.get("/about").status_code == 302
    _authenticated(client)
    response = client.get("/about")
    body = response.get_data(as_text=True)
    assert response.status_code == 200
    assert "SOCIÉTÉ D’ÉNERGIE DU LUALABA" in body
    assert "MonarQue Group S.A.S." in body
    assert "Pôle informatique LTEX" in body
    assert "CD/KWZ/RCCM/26-B-00523" in body
    assert "contact@monarquegrp.com" in body


def test_about_does_not_turn_the_product_surface_into_a_compliance_warning(client):
    _authenticated(client)
    body = client.get("/about").get_data(as_text=True)

    assert "DGI" not in body
    assert "authentifi" not in body.lower()
    assert "assiette" not in body.lower()


def test_product_templates_do_not_contain_compliance_disclaimers():
    templates = Path(__file__).resolve().parents[1] / "templates"
    prohibited = ("dgi", "authentifi", "non fiscal", "homologu")

    for template in templates.glob("*.html"):
        content = template.read_text(encoding="utf-8").lower()
        assert not any(term in content for term in prohibited), template.name


def test_base_footer_is_discreet_and_developer_logos_are_not_in_invoice_template():
    base = (Path(__file__).resolve().parents[1] / "templates" / "base.html").read_text(encoding="utf-8")
    invoice = (Path(__file__).resolve().parents[1] / "invoice_pdf.py").read_text(encoding="utf-8")
    assert "Designed and secured by LTEX" in base
    assert "monarque-group-horizontal.png" not in invoice
    assert "ltex.png" not in invoice


def test_developer_brand_assets_are_present_and_non_empty():
    root = Path(__file__).resolve().parents[1] / "static" / "brand"
    for filename in ("monarque-group-horizontal.png", "ltex.png"):
        asset = root / filename
        assert asset.is_file()
        assert asset.stat().st_size > 1000
