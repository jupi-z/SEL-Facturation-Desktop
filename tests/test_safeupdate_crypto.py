import base64
import hashlib
import json
from datetime import datetime, timedelta, timezone

import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from safeupdate_crypto import ManifestVerificationError, PackageVerificationError, verify_package, verify_signed_manifest


def _public_key_text(private_key):
    return base64.b64encode(
        private_key.public_key().public_bytes(
            serialization.Encoding.Raw,
            serialization.PublicFormat.Raw,
        )
    ).decode("ascii")


def _manifest(**overrides):
    now = datetime(2026, 10, 22, 12, 0, tzinfo=timezone.utc)
    value = {
        "schema_version": 2,
        "product": "SEL-Facturation",
        "channel": "stable",
        "version": "0.4.0",
        "sequence": 40,
        "created_at": now.isoformat().replace("+00:00", "Z"),
        "expires_at": (now + timedelta(days=30)).isoformat().replace("+00:00", "Z"),
        "min_supported_version": "0.3.24",
        "package_type": "bootstrapper",
        "windows_code_signed": False,
        "url": "https://github.com/jupi-z/SEL-Facturation-Desktop/releases/download/v0.4.0/SEL-Facturation-Setup-0.4.0.exe",
        "release_url": "https://github.com/jupi-z/SEL-Facturation-Desktop/releases/tag/v0.4.0",
        "size_bytes": 4,
        "sha256": hashlib.sha256(b"test").hexdigest(),
        "notes": "Correctifs SEL",
    }
    value.update(overrides)
    if "version" in overrides and "url" not in overrides:
        version = value["version"]
        value["url"] = f"https://github.com/jupi-z/SEL-Facturation-Desktop/releases/download/v{version}/SEL-Facturation-Setup-{version}.exe"
    return value


def _signed_manifest(private_key, **overrides):
    payload = json.dumps(_manifest(**overrides), ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    signature = private_key.sign(payload)
    envelope = json.dumps({
        "schema": 1,
        "key_id": "test-key",
        "signature_b64": base64.b64encode(signature).decode("ascii"),
    }, separators=(",", ":")).encode("utf-8")
    return payload, envelope


def _verify(payload, envelope, private_key, **kwargs):
    return verify_signed_manifest(
        payload,
        envelope,
        current_version="0.3.24",
        trusted_keys={"test-key": _public_key_text(private_key)},
        now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc),
        **kwargs,
    )


def test_valid_manifest_accepts_unsigned_windows_package_after_ed25519_verification():
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key)

    result = _verify(payload, envelope, private_key)

    assert result["state"] == "available"
    assert result["manifest_trusted"] is True
    assert result["windows_code_signed"] is False
    assert result["key_id"] == "test-key"
    assert result["manifest_sha256"] == hashlib.sha256(payload).hexdigest()


def test_one_manifest_byte_change_is_rejected_before_policy_decision():
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key)
    changed = payload.replace(b"Correctifs", b"Correctifx")

    with pytest.raises(ManifestVerificationError, match="signature"):
        _verify(changed, envelope, private_key)


@pytest.mark.parametrize("envelope", [b"", b"not-json", b'{"schema":1,"key_id":"unknown","signature_b64":"AA=="}'])
def test_missing_malformed_or_unknown_signature_is_rejected(envelope):
    private_key = Ed25519PrivateKey.generate()
    payload, _ = _signed_manifest(private_key)

    with pytest.raises(ManifestVerificationError):
        _verify(payload, envelope, private_key)


def test_no_embedded_trusted_key_fails_closed():
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key)

    with pytest.raises(ManifestVerificationError, match="key"):
        verify_signed_manifest(
            payload,
            envelope,
            current_version="0.3.24",
            trusted_keys={},
            now=datetime(2026, 10, 22, 12, 1, tzinfo=timezone.utc),
        )


def test_duplicate_manifest_keys_are_rejected():
    private_key = Ed25519PrivateKey.generate()
    payload = b'{"schema_version":2,"schema_version":2}'
    envelope = json.dumps({
        "schema": 1,
        "key_id": "test-key",
        "signature_b64": base64.b64encode(private_key.sign(payload)).decode("ascii"),
    }).encode("utf-8")

    with pytest.raises(ManifestVerificationError, match="duplicate"):
        _verify(payload, envelope, private_key)


def test_invalid_utf8_and_json_constants_are_rejected():
    private_key = Ed25519PrivateKey.generate()
    for payload in (b"\xff", b"NaN"):
        envelope = json.dumps({
            "schema": 1,
            "key_id": "test-key",
            "signature_b64": base64.b64encode(private_key.sign(payload)).decode("ascii"),
        }).encode("utf-8")
        with pytest.raises(ManifestVerificationError):
            _verify(payload, envelope, private_key)


@pytest.mark.parametrize("url", [
    "http://github.com/SEL-Facturation-Setup-0.4.0.exe",
    "https://127.0.0.1/SEL-Facturation-Setup-0.4.0.exe",
    "https://user:password@example.test/SEL-Facturation-Setup-0.4.0.exe",
])
def test_manifest_rejects_unsafe_package_urls(url):
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key, url=url)

    with pytest.raises(ManifestVerificationError, match="URL"):
        _verify(payload, envelope, private_key)


@pytest.mark.parametrize("overrides,reason", [
    ({"version": "0.3.23"}, "downgrade"),
    ({"expires_at": "2026-10-22T11:59:00Z"}, "expir"),
    ({"sequence": 39}, "sequence"),
    ({"channel": "preview"}, "channel"),
    ({"min_supported_version": "0.4.1"}, "transition"),
])
def test_manifest_rejects_replay_downgrade_expiry_channel_or_transition(overrides, reason):
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key, **overrides)
    kwargs = {"last_sequence": 40} if overrides.get("sequence") == 39 else {}

    with pytest.raises(ManifestVerificationError, match=reason):
        _verify(payload, envelope, private_key, **kwargs)


def test_duplicate_or_unknown_manifest_fields_are_rejected():
    private_key = Ed25519PrivateKey.generate()
    payload, envelope = _signed_manifest(private_key, unexpected="value")

    with pytest.raises(ManifestVerificationError, match="field"):
        _verify(payload, envelope, private_key)


def test_package_size_and_sha256_are_verified(tmp_path):
    package = tmp_path / "SEL-Facturation-Setup-0.4.0.exe"
    package.write_bytes(b"test")

    verify_package(package, expected_sha256=hashlib.sha256(b"test").hexdigest(), expected_size=4)
    with pytest.raises(PackageVerificationError, match="size"):
        verify_package(package, expected_sha256=hashlib.sha256(b"test").hexdigest(), expected_size=5)
    package.write_bytes(b"evil")
    with pytest.raises(PackageVerificationError, match="SHA-256"):
        verify_package(package, expected_sha256=hashlib.sha256(b"test").hexdigest(), expected_size=4)
