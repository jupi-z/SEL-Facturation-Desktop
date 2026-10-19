import importlib.util
import re
from pathlib import Path

import pytest

if not importlib.util.find_spec("flask"):
    pytest.skip("Flask is not installed", allow_module_level=True)

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
    client.post(
        "/setup",
        data={"_csrf": token, "password": "TestPassword2026!", "confirm": "TestPassword2026!"},
    )
    login = client.get("/login")
    token = re.search(r'name="_csrf" value="([^"]+)"', login.get_data(as_text=True)).group(1)
    client.post("/login", data={"_csrf": token, "password": "TestPassword2026!"})
    return client


def test_logged_in_pages_keep_update_check_off_the_global_shell(authenticated_client):
    html = authenticated_client.get("/").get_data(as_text=True)

    assert 'data-update-status' not in html
    assert 'data-update-poll' not in html
    assert html.count('class="app-footer"') == 1
    assert 'Application développée et sécurisée par LTEX' in html
    assert '<a data-external-link href="https://monarquegrp.com/"' in html
    assert 'rel="noopener noreferrer external">MonarQue Group</a>' in html


def test_external_links_use_the_native_browser_bridge():
    root = Path(__file__).resolve().parents[1]
    base = (root / "templates" / "base.html").read_text(encoding="utf-8")
    about = (root / "templates" / "about.html").read_text(encoding="utf-8")
    script = (root / "static" / "desktop.js").read_text(encoding="utf-8")

    assert base.count('data-external-link') == 2
    assert 'data-external-link href="https://monarquegrp.com/"' in base
    assert 'data-external-link' in about
    assert 'open_external_url' in script


def test_primary_navigation_uses_local_inline_icons_without_remote_dependencies():
    root = Path(__file__).resolve().parents[1]
    base = (root / "templates" / "base.html").read_text(encoding="utf-8")
    icons = (root / "templates" / "_icons.html").read_text(encoding="utf-8")

    assert "{% from '_icons.html' import nav_icon %}" in base
    assert base.count('class="nav-icon"') == 6
    for icon_name in ("dashboard", "clients", "invoice", "history", "settings", "updates"):
        assert f"nav_icon('{icon_name}')" in base
    assert icons.count("<svg") == 1
    assert "currentColor" in icons
    assert "fonts.googleapis.com" not in base
    assert "cdnjs.cloudflare.com" not in base


def test_navigation_icon_layout_is_accessible_and_aligned():
    css = (Path(__file__).resolve().parents[1] / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".nav{display:flex;align-items:center;gap:10px;" in css
    assert ".nav-icon{display:inline-flex;align-items:center;justify-content:center;" in css
    assert ".nav-icon svg{width:18px;height:18px;" in css


def test_update_screen_retains_explicit_update_controls(authenticated_client):
    html = authenticated_client.get("/updates").get_data(as_text=True)

    assert 'action="/updates/check"' in html
    assert "Rechercher une mise à jour" in html
    assert 'data-update-status' not in html


def test_shared_shell_and_action_layout_rules_are_present():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".side{position:sticky;top:0;" in css
    assert "overflow-y:auto" in css
    assert ".main{display:flex;flex-direction:column;min-height:100vh}" in css
    assert ".main>main{flex:1 0 auto}" in css
    assert ".app-footer{" in css
    assert ".form-actions{" in css
    assert ".lifecycle-actions{" in css


def test_action_templates_use_explicit_groups_instead_of_stretched_defaults():
    client_edit = (ROOT / "templates" / "client_edit.html").read_text(encoding="utf-8")
    invoice_detail = (ROOT / "templates" / "invoice_detail.html").read_text(encoding="utf-8")
    new_invoice = (ROOT / "templates" / "new_invoice.html").read_text(encoding="utf-8")
    updates = (ROOT / "templates" / "updates.html").read_text(encoding="utf-8")

    assert 'class="form-actions"' in client_edit
    assert 'class="lifecycle-actions"' in client_edit
    assert 'class="form-actions"' in invoice_detail
    assert 'class="form-actions"' in new_invoice
    assert 'class="form-actions"' in updates
