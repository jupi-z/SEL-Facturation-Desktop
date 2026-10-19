from pathlib import Path
import re

from version import APP_VERSION
from app import create_app


ROOT = Path(__file__).resolve().parents[1]


def test_application_and_inno_use_central_version():
    assert APP_VERSION == "0.3.11"
    iss = (ROOT / "packaging" / "SEL-Facturation.iss").read_text(encoding="utf-8")
    assert f'#define AppVersion "{APP_VERSION}"' in iss
    assert f"SEL-Facturation-Setup-{APP_VERSION}" in iss


def test_health_exposes_central_version(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    app = create_app()
    app.config["TESTING"] = True
    assert app.test_client().get("/health").json["version"] == APP_VERSION

