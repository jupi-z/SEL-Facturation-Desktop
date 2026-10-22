import json
import pytest
from safeupdate_crypto import ManifestVerificationError, verify_signed_manifest
from pathlib import Path

from version import APP_VERSION


ROOT = Path(__file__).resolve().parents[1]


def test_legacy_public_manifest_is_not_trusted_by_safeupdate_v1():
    manifest = json.loads((ROOT / "updates" / "latest.json").read_text(encoding="utf-8"))

    assert manifest["version"] == APP_VERSION
    assert manifest["channel"] == "preview"
    assert manifest["signed"] is False
    with pytest.raises(ManifestVerificationError):
        verify_signed_manifest(
            json.dumps(manifest, separators=(",", ":")).encode(), b"{}",
            current_version="0.3.24", trusted_keys={"unused": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            },
        )
