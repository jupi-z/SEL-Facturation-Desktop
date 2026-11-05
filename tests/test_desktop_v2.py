from __future__ import annotations

import json
from pathlib import Path

import pytest

import desktop_v2


def test_v2_client_assets_are_complete_and_local() -> None:
    assert desktop_v2.client_asset("index.html").is_file()
    assert desktop_v2.client_asset("app.css").is_file()
    assert desktop_v2.client_asset("app.js").is_file()
    assert desktop_v2.client_asset("assets/sel-logo-dark.svg").is_file()
    assert desktop_v2.client_asset("assets/geist-variable.woff2").is_file()
    assert desktop_v2.client_asset("assets/OFL-Geist.txt").is_file()
    assert desktop_v2.client_asset("assets/ASSET-NOTICES.txt").is_file()
    html = desktop_v2.client_asset("index.html").read_text(encoding="utf-8")
    css = desktop_v2.client_asset("app.css").read_text(encoding="utf-8")
    assert "https://" not in html
    assert 'src="assets/sel-logo-dark.svg"' in html
    assert 'url("assets/geist-variable.woff2")' in css
    assert 'font-family: "Geist"' in css
    assert "window.pywebview.api" in desktop_v2.client_asset("app.js").read_text(
        encoding="utf-8"
    )


def test_v2_navigation_uses_local_icons_and_accessible_current_state() -> None:
    html = desktop_v2.client_asset("index.html").read_text(encoding="utf-8")

    assert 'aria-label="Navigation principale"' in html
    assert 'class="nav-section-label"' not in html
    assert html.count('class="nav-icon"') == 6
    assert 'aria-current="page"' in html
    assert "https://" not in html
    script = desktop_v2.client_asset("app.js").read_text(encoding="utf-8")
    assert 'setAttribute("aria-current", "page")' in script
    assert 'classList.toggle("pending", Boolean(pending))' in script
    assert '"En attente de synchronisation"' in script


def test_first_launch_configuration_persists_stable_identity(tmp_path: Path) -> None:
    config_path = tmp_path / "config.json"
    first = desktop_v2.load_or_create_config(config_path)
    second = desktop_v2.load_or_create_config(config_path)

    assert first == second
    assert first["organization_id"]
    assert first["device_id"]
    assert len(first["auth_token"]) >= 32
    assert Path(first["database_path"]).parent == tmp_path
    assert json.loads(config_path.read_text(encoding="utf-8")) == first


def test_local_bridge_rejects_non_local_paths() -> None:
    bridge = desktop_v2.LocalApiBridge("http://127.0.0.1:8090", "secret-token")
    with pytest.raises(ValueError):
        bridge.request("https://example.com")
    with pytest.raises(ValueError):
        bridge.request("/v1/sync/status")
    with pytest.raises(ValueError):
        bridge.request("/local/v1/customers", "DELETE")


def test_packaging_smoke_does_not_require_rust_binary() -> None:
    assert desktop_v2.packaging_smoke() == {
        "ok": True,
        "client_assets": 7,
        "native_binary": "deferred-to-build",
    }


def test_v2_client_uses_safe_dom_rendering_and_explicit_invoice_preview() -> None:
    script = desktop_v2.client_asset("app.js").read_text(encoding="utf-8")

    assert "innerHTML" not in script
    assert "textContent" in script
    assert "/local/v1/billing/utility/preview" in script
    assert "Aucune facture n’est encore émise." in script
    assert "Émettre la facture" in script
    assert "invalidatePreview" in script
    assert '["input", "change"]' in script


def test_v2_payment_form_selects_customer_receivables_from_local_statement() -> None:
    html = desktop_v2.client_asset("index.html").read_text(encoding="utf-8")
    script = desktop_v2.client_asset("app.js").read_text(encoding="utf-8")

    assert 'id="payment-receivable"' in html
    assert 'name="receivable_id"' in html
    assert "/local/v1/customers/" in script
    assert "/statement" in script
    assert "populateReceivables" in script
