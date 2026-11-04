from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "v2" / "windows-app"


def test_v2_windows_client_uses_the_approved_tauri_react_stack() -> None:
    package = json.loads((APP / "package.json").read_text(encoding="utf-8"))
    manifest = (APP / "src-tauri" / "Cargo.toml").read_text(encoding="utf-8")
    config = json.loads((APP / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))

    assert "react" in package["dependencies"]
    assert "@tauri-apps/api" in package["dependencies"]
    assert "build" in package["scripts"]
    assert 'name = "sel-v2-windows"' in manifest
    assert config["productName"] == "SEL Facturation"
    assert config["app"]["windows"][0]["width"] == 1440


def test_v2_windows_client_keeps_local_api_credentials_out_of_javascript() -> None:
    adapter = (APP / "src" / "api.ts").read_text(encoding="utf-8")
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert 'invoke<LocalResponse<T>>("local_request"' in adapter
    assert "Authorization" not in adapter
    assert "/local/v1/" in native
    assert "Bearer " in native
    assert "local_request" in native
    assert '"https://' not in adapter


def test_v2_windows_client_exposes_the_figma_primary_workflows() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    for label in (
        "Vue d’ensemble",
        "Abonnés",
        "Factures",
        "Encaissements",
        "Kits solaires",
        "Créances",
        "Synchronisation",
        "Administration",
        "Prévisualiser la facture",
    ):
        assert label in app

    assert 'name="tax_id"' not in app
    assert "Le dossier local enregistre le code abonné, le nom, l’adresse et le téléphone." in app


def test_v2_windows_client_has_local_brand_assets_and_no_remote_runtime_assets() -> None:
    html = (APP / "index.html").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert (APP / "public" / "assets" / "sel-logo-dark.svg").is_file()
    assert (APP / "public" / "assets" / "geist-variable.woff2").is_file()
    assert (APP / "src" / "vite-env.d.ts").is_file()
    assert "https://" not in html
    assert "https://" not in css
    assert "prefers-reduced-motion" in css


def test_v2_windows_shell_preserves_figma_desktop_geometry_and_tokens() -> None:
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert "--navy: #012a5e" in css
    assert "--blue: #075bbb" in css
    assert "--yellow: #fddc02" in css
    assert ".sidebar { position: sticky" in css
    assert "flex: 0 0 250px" in css
    assert ".topbar { display: flex; flex: 0 0 76px" in css
    assert "height: 93px" in css
    assert "width: 20px; height: 20px; color: #98b9e0" in css


def test_dashboard_uses_recent_local_operations_without_demo_financial_data() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")

    assert 'title="Dossiers / activité récente"' in app
    assert 'headers={["Référence", "Abonné", "Opération", "Montant", "État"]}' in app
    assert "...invoices.map" in app
    assert "...payments.map" in app
    assert "...sales.map" in app
    assert "value={customers.length}" in app
    assert "value={invoices.length}" in app
    assert 'onClick={() => setView("invoices")}>Nouvelle facture' in app
    assert "218,40" not in app
    assert "1 124,20" not in app


def test_financial_amounts_are_formatted_without_client_side_calculation() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    formatter = app.split("function formatMoney(", 1)[1].split("\n}\n", 1)[0]

    assert "padEnd(2, \"0\")" in formatter
    assert "replace(/\\B(?=(\\d{3})+(?!\\d))/g" in formatter
    assert "Number(" not in formatter
    assert "formatMoney(data.total, data.currency)" in app
    assert "formatMoney(row.balance, row.currency)" in app


def test_solar_sales_use_the_approved_local_catalog_and_rust_quote() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    storage = (ROOT / "v2" / "crates" / "sel-storage" / "src" / "sqlite.rs").read_text(encoding="utf-8")

    assert '"/local/v1/solar/products"' in local_api
    assert "list_active_solar_products" in storage
    assert '"/local/v1/solar/products"' in app
    assert '"/local/v1/solar/quotes"' in app
    assert "Vérifier le devis" in app
    assert "principal: result.principal" in app
    assert 'name="principal"' not in app
    assert "innerHTML" not in app
    assert "<NavIcon" in app


def test_solar_catalog_route_uses_local_api_error_contract() -> None:
    local_api = (ROOT / "v2" / "crates" / "sel-local" / "src" / "lib.rs").read_text(encoding="utf-8")
    handler = local_api.split("async fn list_solar_products(", 1)[1].split("\n}\n", 1)[0]

    assert "Result<Json<Vec<serde_json::Value>>, (StatusCode, String)>" in handler
    assert ".map_err(internal_error)?" in handler
    assert "ApiError" not in handler
    assert "storage_error" not in handler


def test_historical_invoice_view_never_offers_to_issue_an_existing_invoice() -> None:
    app = (APP / "src" / "App.tsx").read_text(encoding="utf-8")
    history_view = app.split("{selectedInvoice &&", 1)[1].split("</>}", 1)[0]

    assert "<Preview data={selectedInvoice}" in history_view
    assert "onConfirm=" not in history_view
    assert "{onConfirm &&" in app


def test_tauri_bridge_fails_closed_and_offloads_local_http() -> None:
    native = (APP / "src-tauri" / "src" / "lib.rs").read_text(encoding="utf-8")

    assert "start_agent(app.handle(), &data_dir).ok()" not in native
    assert "spawn_blocking" in native
    assert "MAX_LOCAL_RESPONSE_BYTES: usize = 16 * 1024 * 1024" in native
    assert "take((MAX_LOCAL_RESPONSE_BYTES + 1) as u64)" in native
    assert "env_clear()" in native
    assert "https://monarquegrp.com" in native
    assert "open_monarque_site" in native
    assert "open_monarque_site" in (APP / "src" / "api.ts").read_text(encoding="utf-8")


def test_tauri_windows_bundle_creates_sidecar_resource_directory() -> None:
    workflow = (ROOT / ".github" / "workflows" / "sel-v2-tauri-windows.yml").read_text(encoding="utf-8")

    assert "New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination)" in workflow
    assert "Copy-Item -LiteralPath $source -Destination $destination -Force" in workflow
