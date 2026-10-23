import base64
from datetime import datetime, timedelta, timezone
import hashlib
import io
import json
from threading import Event

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from safeupdate_crypto import verify_signed_manifest
from update_client import download_package, fetch_manifest
from update_transaction import TransactionVerificationError, load_transaction, verify_transaction


class Response:
    def __init__(self, payload, url):
        self.payload = io.BytesIO(payload)
        self.url = url

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self, size=-1):
        return self.payload.read(size)

    def geturl(self):
        return self.url


def _key_text(private_key):
    return base64.b64encode(private_key.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw,
    )).decode("ascii")


def _signed_update(payload=b"trusted setup bytes", version="0.4.0"):
    private_key = Ed25519PrivateKey.generate()
    manifest = {
        "schema_version": 2,
        "product": "SEL-Facturation",
        "channel": "stable",
        "version": version,
        "sequence": 40,
        "created_at": "2026-10-22T12:00:00Z",
        "expires_at": "2026-11-22T12:00:00Z",
        "min_supported_version": "0.3.24",
        "package_type": "bootstrapper",
        "windows_code_signed": False,
        "url": f"https://updates.example.test/releases/SEL-Facturation-Setup-{version}.exe",
        "release_url": f"https://updates.example.test/releases/v{version}",
        "size_bytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "notes": "Correctif de test",
    }
    manifest_bytes = json.dumps(manifest, separators=(",", ":")).encode("utf-8")
    signature_bytes = json.dumps({
        "schema": 1,
        "key_id": "test-key",
        "signature_b64": base64.b64encode(private_key.sign(manifest_bytes)).decode("ascii"),
    }, separators=(",", ":")).encode("utf-8")
    keys = {"test-key": _key_text(private_key)}
    update = verify_signed_manifest(
        manifest_bytes, signature_bytes, current_version="0.3.24", trusted_keys=keys,
        now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc),
    )
    return private_key, keys, manifest_bytes, signature_bytes, update, payload


def test_fetch_manifest_authenticates_manifest_and_detached_signature_bytes(tmp_path):
    _, keys, manifest_bytes, signature_bytes, _, _ = _signed_update()
    calls = []

    def opener(request, timeout):
        calls.append(request.full_url)
        if request.full_url.endswith("latest.json"):
            return Response(manifest_bytes, request.full_url)
        return Response(signature_bytes, request.full_url)

    result = fetch_manifest(
        "https://updates.example.test/updates/latest.json",
        current_version="0.3.24",
        trusted_keys=keys,
        state_path=tmp_path / "state.json",
        opener=opener,
        now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc),
    )

    assert result["manifest_trusted"] is True
    assert result["windows_code_signed"] is False
    assert calls == [
        "https://updates.example.test/updates/latest.json",
        "https://updates.example.test/updates/latest.sig.json",
    ]


def test_download_stages_only_authenticated_package_and_reports_real_bytes(tmp_path):
    _, keys, manifest_bytes, signature_bytes, update, payload = _signed_update()
    progress = []

    def opener(request, timeout):
        if request.full_url.endswith(".exe"):
            return Response(payload, request.full_url)
        raise AssertionError(f"unexpected request: {request.full_url}")

    result = download_package(
        update,
        tmp_path,
        current_version="0.3.24",
        trusted_keys=keys,
        progress=lambda received, total: progress.append((received, total)),
        opener=opener,
    )

    assert result["version"] == "0.4.0"
    assert result["transaction_id"]
    assert progress[-1] == (len(payload), len(payload))
    record = load_transaction(result["transaction_id"], staging_root=tmp_path)
    assert record["manifest_bytes"] == base64.b64encode(manifest_bytes).decode("ascii")
    assert record["signature_envelope_bytes"] == base64.b64encode(signature_bytes).decode("ascii")
    assert verify_transaction(
        result["transaction_id"], staging_root=tmp_path,
        current_version="0.3.24", trusted_keys=keys,
    )["version"] == "0.4.0"


def test_download_cancellation_removes_partial_file(tmp_path):
    _, keys, _, _, update, _ = _signed_update(payload=b"payload")
    cancelled = Event()
    cancelled.set()

    with pytest.raises(ValueError, match="cancel"):
        download_package(
            update, tmp_path, current_version="0.3.24", trusted_keys=keys,
            cancel_event=cancelled,
            opener=lambda *_args, **_kwargs: pytest.fail("download should not start"),
        )
    assert not list(tmp_path.rglob("*.exe"))


def test_transaction_rejects_package_modified_after_download(tmp_path):
    _, keys, _, _, update, payload = _signed_update()

    result = download_package(
        update, tmp_path, current_version="0.3.24", trusted_keys=keys,
        opener=lambda request, timeout: Response(payload, request.full_url),
    )
    package = next(tmp_path.rglob("*.exe"))
    package.write_bytes(b"tampered")

    with pytest.raises(TransactionVerificationError, match="SHA-256|size"):
        verify_transaction(
            result["transaction_id"], staging_root=tmp_path,
            current_version="0.3.24", trusted_keys=keys,
        )


def test_transaction_rejects_path_escape(tmp_path):
    _, keys, _, _, update, payload = _signed_update()
    result = download_package(
        update, tmp_path, current_version="0.3.24", trusted_keys=keys,
        opener=lambda request, timeout: Response(payload, request.full_url),
    )
    record_path = next(tmp_path.rglob("transaction.json"))
    record = json.loads(record_path.read_text(encoding="utf-8"))
    record["package_name"] = "..\\outside.exe"
    record_path.write_text(json.dumps(record), encoding="utf-8")

    with pytest.raises(TransactionVerificationError, match="path"):
        verify_transaction(
            result["transaction_id"], staging_root=tmp_path,
            current_version="0.3.24", trusted_keys=keys,
        )
