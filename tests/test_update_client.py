import base64
from datetime import datetime, timezone
import hashlib
import io
import json

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from safeupdate_crypto import verify_signed_manifest
from update_client import download_package, fetch_manifest, verify_authenticode


class Response:
    def __init__(self, payload, url):
        self.payload = io.BytesIO(payload)
        self.url = url
    def __enter__(self): return self
    def __exit__(self, *args): return False
    def read(self, size=-1): return self.payload.read(size)
    def geturl(self): return self.url


def signed_update(payload=b"trusted setup bytes"):
    key = Ed25519PrivateKey.generate()
    version = "0.4.0"
    manifest = {
        "schema_version": 2, "product": "SEL-Facturation", "channel": "stable",
        "version": version, "sequence": 42,
        "created_at": "2026-10-22T12:00:00Z", "expires_at": "2026-11-22T12:00:00Z",
        "min_supported_version": "0.3.24", "package_type": "bootstrapper",
        "windows_code_signed": False,
        "url": "https://updates.example.test/releases/SEL-Facturation-Setup-0.4.0.exe",
        "release_url": "https://updates.example.test/releases/v0.4.0",
        "size_bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest(), "notes": "test",
    }
    manifest_bytes = json.dumps(manifest, separators=(",", ":")).encode()
    signature_bytes = json.dumps({
        "schema": 1, "key_id": "test-key",
        "signature_b64": base64.b64encode(key.sign(manifest_bytes)).decode(),
    }, separators=(",", ":")).encode()
    keys = {"test-key": base64.b64encode(key.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)).decode()}
    update = verify_signed_manifest(manifest_bytes, signature_bytes, current_version="0.3.24", trusted_keys=keys, now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc))
    return update, keys, manifest_bytes, signature_bytes, payload


def test_fetch_requires_https_before_network():
    with pytest.raises(ValueError, match="HTTPS"):
        fetch_manifest("http://example.test/latest.json", current_version="0.3.24", opener=lambda *_: pytest.fail("network called"))


def test_fetch_requires_detached_ed25519_signature(tmp_path):
    update, keys, manifest_bytes, signature_bytes, _ = signed_update()
    def opener(request, timeout):
        return Response(manifest_bytes if request.full_url.endswith("latest.json") else signature_bytes, request.full_url)
    result = fetch_manifest("https://updates.example.test/updates/latest.json", current_version="0.3.24", trusted_keys=keys, state_path=tmp_path / "state.json", opener=opener, now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc))
    assert result["manifest_trusted"] is True
    assert result["windows_code_signed"] is False


def test_download_accepts_unsigned_windows_binary_when_manifest_is_authenticated(tmp_path):
    update, keys, _, _, payload = signed_update()
    result = download_package(update, tmp_path, current_version="0.3.24", trusted_keys=keys, opener=lambda request, timeout: Response(payload, request.full_url))
    assert result["version"] == "0.4.0"
    assert next(tmp_path.rglob("*.exe")).read_bytes() == payload


def test_download_rejects_missing_manifest_evidence_before_network(tmp_path):
    update, _, _, _, _ = signed_update()
    update.pop("manifest_bytes")
    with pytest.raises(ValueError, match="Preuves"):
        download_package(update, tmp_path, current_version="0.3.24", opener=lambda *_: pytest.fail("network called"))


def test_download_rejects_package_hash_mismatch(tmp_path):
    update, keys, _, _, _ = signed_update()
    with pytest.raises(ValueError, match="SHA-256"):
        download_package(update, tmp_path, current_version="0.3.24", trusted_keys=keys, opener=lambda request, timeout: Response(b"X" * update["size_bytes"], request.full_url))


def test_authenticode_diagnostic_does_not_accept_substring(monkeypatch, tmp_path):
    package = tmp_path / "setup.exe"
    package.write_bytes(b"setup")
    class Result:
        returncode = 0
        stdout = "CN=MonarQue Group S.A.S. (unrelated publisher)"
    monkeypatch.setattr("update_client.subprocess.run", lambda *args, **kwargs: Result())
    assert not verify_authenticode(package, expected_subject="MonarQue Group S.A.S.")
