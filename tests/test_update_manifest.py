import json
import pytest
from safeupdate_crypto import ManifestVerificationError, verify_signed_manifest
from pathlib import Path

from version import APP_VERSION
from app import DEFAULT_UPDATE_MANIFEST_URL


ROOT = Path(__file__).resolve().parents[1]


def test_safeupdate_default_channel_uses_sel_updates_subdomain():
    assert DEFAULT_UPDATE_MANIFEST_URL == "https://update.sel.shuleops.com/updates/latest.json"


def test_legacy_public_manifest_is_not_trusted_by_safeupdate_v1():
    manifest = json.loads((ROOT / "updates" / "latest.json").read_text(encoding="utf-8"))

    # Keep this public manifest as historical 0.3.24 metadata until a release is authorized.
    assert manifest["version"] == "0.3.24"
    assert manifest["channel"] == "preview"
    assert manifest["signed"] is False
    with pytest.raises(ManifestVerificationError):
        verify_signed_manifest(
            json.dumps(manifest, separators=(",", ":")).encode(), b"{}",
            current_version="0.3.24", trusted_keys={"unused": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            },
        )
