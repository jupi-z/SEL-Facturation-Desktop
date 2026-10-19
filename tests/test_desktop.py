"""Validation du pont PDF natif : aucune GUI requise pour les tests."""
from __future__ import annotations

import base64
import hashlib
import os
import sqlite3
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from desktop import DesktopDocuments, startup_error_message


def pdf_payload():
    return base64.b64encode(b"%PDF-1.4\n%essai\n%%EOF\n").decode("ascii")


def test_startup_error_identifies_database_failures_separately_from_webview2():
    database_message = startup_error_message(sqlite3.OperationalError("table currency_catalog has no column named label"))
    webview_message = startup_error_message(RuntimeError("WebView2 indisponible"))

    assert "base de données locale" in database_message
    assert "WebView2" not in database_message
    assert "WebView2" in webview_message


def test_documents_validate_inputs_and_fiscal_neutrality():
    api = DesktopDocuments()
    assert not hasattr(api, "window")
    assert hasattr(api, "_window")
    assert api._validate_pdf("SEL-2026-000001.pdf", pdf_payload()).startswith(b"%PDF-")
    for name in ("../dossier.pdf", "other.pdf", "SEL-2026-000001.exe"):
        with pytest.raises(ValueError):
            api._validate_pdf(name, pdf_payload())
    with pytest.raises(ValueError):
        api._validate_pdf("SEL-2026-000001.pdf", base64.b64encode(b"hello").decode("ascii"))
    with pytest.raises(ValueError):
        api._validate_pdf("SEL-2026-000001.pdf", "not+base64====")


def test_open_pdf_uses_os_default_viewer_and_app_data(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    opened = []
    monkeypatch.setattr(os, "startfile", lambda path, *args: opened.append((path, args)), raising=False)
    api = DesktopDocuments()
    response = api.open_pdf("SEL-2026-000001.pdf", pdf_payload())
    assert response == {"ok": True}
    assert len(opened) == 1
    target = Path(opened[0][0])
    assert target.is_relative_to(tmp_path)
    assert target.read_bytes().startswith(b"%PDF-")


def test_open_external_url_delegates_to_windows_default_handler(monkeypatch):
    opened = []
    monkeypatch.setattr(os, "startfile", lambda target: opened.append(target), raising=False)

    response = DesktopDocuments().open_external_url("https://monarquegrp.com/")

    assert response == {"ok": True}
    assert opened == ["https://monarquegrp.com/"]


def test_open_external_url_rejects_non_external_schemes(monkeypatch):
    monkeypatch.setattr(os, "startfile", lambda *_args: pytest.fail("URL should not be opened"), raising=False)

    response = DesktopDocuments().open_external_url("javascript:alert(1)")

    assert response["ok"] is False


def test_print_pdf_calls_windows_print_verb(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    opened = []
    monkeypatch.setattr(os, "startfile", lambda path, *args: opened.append((path, args)), raising=False)
    api = DesktopDocuments()
    assert api.print_pdf("SEL-2026-000001.pdf", pdf_payload()) == {"ok": True}
    assert opened and opened[0][1] == ("print",)


def test_print_pdf_falls_back_to_open_viewer_when_print_association_fails(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    opened = []

    def startfile(path, *args):
        opened.append((path, args))
        if args == ("print",):
            raise OSError("No print association")

    monkeypatch.setattr(os, "startfile", startfile, raising=False)
    api = DesktopDocuments()
    response = api.print_pdf("SEL-2026-000001.pdf", pdf_payload())
    assert response["ok"] is False
    assert response["fallback"] == "open"
    assert opened[0][1] == ("print",)
    assert opened[1][1] == ()


def test_install_update_launches_only_verified_staged_bootstrapper(tmp_path, monkeypatch):
    staging = tmp_path / "updates"
    staging.mkdir()
    package = staging / "SEL-Facturation-Setup-0.4.0.exe"
    payload = b"signed setup placeholder"
    package.write_bytes(payload)
    launched = []

    monkeypatch.setattr("desktop.staging_dir", lambda: staging)
    monkeypatch.setenv("SEL_UPDATE_EXPECTED_PUBLISHER", "Test Publisher")
    monkeypatch.setattr("desktop.verify_authenticode", lambda *args, **kwargs: True)
    monkeypatch.setattr("desktop.subprocess.Popen", lambda args, **kwargs: launched.append((args, kwargs)))

    api = DesktopDocuments()
    response = api.install_update(package.name, hashlib.sha256(payload).hexdigest())

    assert response == {"ok": True}
    assert launched == [([str(package), "/norestart"], {"close_fds": True})]


def test_install_update_rejects_path_traversal_and_wrong_hash(tmp_path, monkeypatch):
    staging = tmp_path / "updates"
    staging.mkdir()
    package = staging / "SEL-Facturation-Setup-0.4.0.exe"
    package.write_bytes(b"setup")
    monkeypatch.setattr("desktop.staging_dir", lambda: staging)

    api = DesktopDocuments()
    assert api.install_update("..\\setup.exe", "0" )["ok"] is False
    assert api.install_update(package.name, "0" )["ok"] is False


def test_desktop_script_reports_print_fallback_as_guidance():
    script = (Path(__file__).resolve().parents[1] / "static" / "desktop.js").read_text(encoding="utf-8")
    assert 'result.fallback === \'open\'' in script
    assert 'Ctrl+P' in script


def test_temporary_pdf_cleanup_removes_only_expired_files(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    folder = tmp_path / "documents-temporaires"
    folder.mkdir()
    expired = folder / "expired.pdf"
    recent = folder / "recent.pdf"
    expired.write_bytes(b"%PDF-expired")
    recent.write_bytes(b"%PDF-recent")
    old_timestamp = expired.stat().st_mtime - (8 * 24 * 60 * 60)
    os.utime(expired, (old_timestamp, old_timestamp))

    api = DesktopDocuments()
    api.open_pdf("SEL-2026-000001.pdf", pdf_payload())

    assert not expired.exists()
    assert recent.exists()
