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

    assert 'invoke("local_request"' in adapter
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


def test_v2_windows_client_has_local_brand_assets_and_no_remote_runtime_assets() -> None:
    html = (APP / "index.html").read_text(encoding="utf-8")
    css = (APP / "src" / "App.css").read_text(encoding="utf-8")

    assert (APP / "public" / "assets" / "sel-logo-dark.svg").is_file()
    assert (APP / "public" / "assets" / "geist-variable.woff2").is_file()
    assert "https://" not in html
    assert "https://" not in css
    assert "prefers-reduced-motion" in css
