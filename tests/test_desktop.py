"""Validation du pont PDF natif : aucune GUI requise pour les tests."""
from __future__ import annotations

import base64
import os
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from desktop import DesktopDocuments


def pdf_payload():
    return base64.b64encode(b"%PDF-1.4\n%essai\n%%EOF\n").decode("ascii")


def test_documents_validate_inputs_and_fiscal_neutrality():
    api = DesktopDocuments()
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


def test_print_pdf_calls_windows_print_verb(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    opened = []
    monkeypatch.setattr(os, "startfile", lambda path, *args: opened.append((path, args)), raising=False)
    api = DesktopDocuments()
    assert api.print_pdf("SEL-2026-000001.pdf", pdf_payload()) == {"ok": True}
    assert opened and opened[0][1] == ("print",)
