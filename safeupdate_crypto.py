"""Fail-closed cryptographic primitives for SEL SafeUpdate V1.

This module deliberately does not fetch URLs, invoke installers, or read the
business database.  It authenticates exact manifest bytes and verifies the
package bytes described by that manifest.  Production trust roots are passed
by the caller and are expected to come from ``safeupdate_keys.py``.
"""
from __future__ import annotations

import base64
from datetime import datetime, timezone
import hashlib
import hmac
import ipaddress
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey


MAX_MANIFEST_BYTES = 128 * 1024
MAX_SIGNATURE_BYTES = 8 * 1024
MAX_PACKAGE_BYTES = 512 * 1024 * 1024
_SHA256 = re.compile(r"\A[0-9a-fA-F]{64}\Z")
_KEY_ID = re.compile(r"\A[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}\Z")
_VERSION = re.compile(
    r"\A(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?\Z"
)
_MANIFEST_FIELDS = frozenset({
    "schema_version", "product", "channel", "version", "sequence",
    "created_at", "expires_at", "min_supported_version", "package_type",
    "windows_code_signed", "url", "release_url", "size_bytes", "sha256",
    "notes",
})
_REQUIRED_MANIFEST_FIELDS = _MANIFEST_FIELDS
_SIGNATURE_FIELDS = frozenset({"schema", "key_id", "signature_b64"})


class ManifestVerificationError(ValueError):
    """Raised when a manifest cannot be authenticated or safely interpreted."""


class PackageVerificationError(ValueError):
    """Raised when a staged package does not match signed metadata."""


def _duplicate_rejector(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ManifestVerificationError("duplicate JSON field")
        result[key] = value
    return result


def _constant_rejector(value):
    raise ManifestVerificationError(f"JSON constant not allowed: {value}")


def parse_strict_json(payload: bytes, *, max_bytes: int):
    if not isinstance(payload, bytes) or len(payload) == 0 or len(payload) > max_bytes:
        raise ManifestVerificationError("JSON payload exceeds the allowed size")
    try:
        text = payload.decode("utf-8")
        value = json.loads(
            text,
            object_pairs_hook=_duplicate_rejector,
            parse_constant=_constant_rejector,
        )
    except ManifestVerificationError:
        raise
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ManifestVerificationError("invalid JSON or UTF-8") from exc
    return value


def _version(value: str) -> tuple[int, int, int, tuple[str, ...]]:
    match = _VERSION.fullmatch(str(value))
    if not match:
        raise ManifestVerificationError("invalid version")
    pre = tuple(match.group(4).split(".")) if match.group(4) else ()
    return int(match.group(1)), int(match.group(2)), int(match.group(3)), pre


def compare_versions(left: str, right: str) -> int:
    a = _version(left)
    b = _version(right)
    if a[:3] != b[:3]:
        return (a[:3] > b[:3]) - (a[:3] < b[:3])
    if a[3] == b[3]:
        return 0
    if not a[3]:
        return 1
    if not b[3]:
        return -1
    for left_part, right_part in zip(a[3], b[3]):
        if left_part == right_part:
            continue
        left_num, right_num = left_part.isdigit(), right_part.isdigit()
        if left_num and right_num:
            return (int(left_part) > int(right_part)) - (int(left_part) < int(right_part))
        if left_num != right_num:
            return -1 if left_num else 1
        return (left_part > right_part) - (left_part < right_part)
    return (len(a[3]) > len(b[3])) - (len(a[3]) < len(b[3]))


def _parse_time(value: object, field: str) -> datetime:
    if not isinstance(value, str) or len(value) > 64:
        raise ManifestVerificationError(f"invalid {field}")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ManifestVerificationError(f"invalid {field}") from exc
    if parsed.tzinfo is None:
        raise ManifestVerificationError(f"invalid {field} timezone")
    return parsed.astimezone(timezone.utc)


def _safe_https_url(value: object, *, expected_filename: str | None = None) -> str:
    if not isinstance(value, str) or len(value) > 2048:
        raise ManifestVerificationError("invalid URL")
    try:
        parsed = urlsplit(value)
    except ValueError as exc:
        raise ManifestVerificationError("invalid URL") from exc
    hostname = (parsed.hostname or "").lower().rstrip(".")
    if parsed.scheme.lower() != "https" or not hostname or parsed.username or parsed.password:
        raise ManifestVerificationError("HTTPS URL required")
    if parsed.fragment or any(char in value for char in "\r\n"):
        raise ManifestVerificationError("invalid URL")
    if hostname in {"localhost", "localhost.localdomain"} or hostname.endswith(".local"):
        raise ManifestVerificationError("private URL blocked")
    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        address = None
    if address is not None and (address.is_private or address.is_loopback or address.is_link_local or address.is_reserved or address.is_multicast or address.is_unspecified):
        raise ManifestVerificationError("private URL blocked")
    if expected_filename is not None:
        filename = Path(unquote(parsed.path)).name
        if filename != expected_filename:
            raise ManifestVerificationError("package URL filename mismatch")
    return value


def _public_key(value: str | bytes) -> Ed25519PublicKey:
    try:
        raw = value if isinstance(value, bytes) else base64.b64decode(value, validate=True)
        if len(raw) != 32:
            raise ValueError
        return Ed25519PublicKey.from_public_bytes(raw)
    except (ValueError, TypeError) as exc:
        raise ManifestVerificationError("invalid trusted public key") from exc


def _signature_envelope(envelope_bytes: bytes, manifest_bytes: bytes, trusted_keys: dict[str, str | bytes]) -> tuple[str, bytes]:
    envelope = parse_strict_json(envelope_bytes, max_bytes=MAX_SIGNATURE_BYTES)
    if not isinstance(envelope, dict) or set(envelope) != _SIGNATURE_FIELDS:
        raise ManifestVerificationError("invalid signature envelope fields")
    if envelope.get("schema") != 1:
        raise ManifestVerificationError("unsupported signature schema")
    key_id = envelope.get("key_id")
    if not isinstance(key_id, str) or not _KEY_ID.fullmatch(key_id):
        raise ManifestVerificationError("invalid key id")
    if key_id not in trusted_keys:
        raise ManifestVerificationError("unknown key id")
    encoded = envelope.get("signature_b64")
    if not isinstance(encoded, str):
        raise ManifestVerificationError("invalid signature")
    try:
        signature = base64.b64decode(encoded, validate=True)
    except (ValueError, TypeError) as exc:
        raise ManifestVerificationError("invalid signature encoding") from exc
    if len(signature) != 64:
        raise ManifestVerificationError("invalid signature length")
    try:
        _public_key(trusted_keys[key_id]).verify(signature, manifest_bytes)
    except InvalidSignature as exc:
        raise ManifestVerificationError("manifest signature invalid") from exc
    return key_id, signature


def verify_signed_manifest(
    manifest_bytes: bytes,
    signature_envelope_bytes: bytes,
    *,
    current_version: str,
    trusted_keys: dict[str, str | bytes],
    last_sequence: int = 0,
    last_manifest_sha256: str | None = None,
    allowed_channel: str = "stable",
    now: datetime | None = None,
) -> dict:
    """Authenticate exact manifest bytes, then apply update policy."""
    if not trusted_keys:
        raise ManifestVerificationError("no trusted update key provisioned")
    key_id, _ = _signature_envelope(signature_envelope_bytes, manifest_bytes, trusted_keys)
    manifest = parse_strict_json(manifest_bytes, max_bytes=MAX_MANIFEST_BYTES)
    if not isinstance(manifest, dict) or set(manifest) != _REQUIRED_MANIFEST_FIELDS:
        raise ManifestVerificationError("invalid or unknown manifest fields")
    if manifest["schema_version"] != 2:
        raise ManifestVerificationError("unsupported manifest schema")
    if manifest["product"] != "SEL-Facturation":
        raise ManifestVerificationError("unsupported product")
    if manifest["channel"] not in {"stable", "preview"} or manifest["channel"] != allowed_channel:
        raise ManifestVerificationError("channel not allowed")
    version = manifest["version"]
    min_supported = manifest["min_supported_version"]
    _version(version)
    _version(min_supported)
    _version(current_version)
    if compare_versions(current_version, min_supported) < 0:
        raise ManifestVerificationError("transition required")
    if manifest["package_type"] != "bootstrapper":
        raise ManifestVerificationError("unsupported package type")
    if not isinstance(manifest["windows_code_signed"], bool):
        raise ManifestVerificationError("invalid Windows signature metadata")
    if not isinstance(manifest["sequence"], int) or isinstance(manifest["sequence"], bool) or manifest["sequence"] <= 0:
        raise ManifestVerificationError("invalid sequence")
    if manifest["sequence"] < last_sequence:
        raise ManifestVerificationError("sequence regressed")
    manifest_digest = hashlib.sha256(manifest_bytes).hexdigest()
    if manifest["sequence"] == last_sequence and last_manifest_sha256 and not hmac.compare_digest(manifest_digest, last_manifest_sha256):
        raise ManifestVerificationError("sequence replay")
    if not isinstance(manifest["size_bytes"], int) or isinstance(manifest["size_bytes"], bool) or not 1 <= manifest["size_bytes"] <= MAX_PACKAGE_BYTES:
        raise ManifestVerificationError("invalid package size")
    if not isinstance(manifest["sha256"], str) or not _SHA256.fullmatch(manifest["sha256"]):
        raise ManifestVerificationError("invalid package SHA-256")
    if not isinstance(manifest["notes"], str) or len(manifest["notes"]) > 4096:
        raise ManifestVerificationError("invalid notes")
    expected_filename = f"SEL-Facturation-Setup-{version}.exe"
    package_url = _safe_https_url(manifest["url"], expected_filename=expected_filename)
    release_url = _safe_https_url(manifest["release_url"])
    created_at = _parse_time(manifest["created_at"], "created_at")
    expires_at = _parse_time(manifest["expires_at"], "expires_at")
    if expires_at <= created_at:
        raise ManifestVerificationError("invalid expiration")
    now = (now or datetime.now(timezone.utc)).astimezone(timezone.utc)
    if expires_at <= now:
        raise ManifestVerificationError("manifest expired")
    comparison = compare_versions(version, current_version)
    if comparison < 0:
        raise ManifestVerificationError("downgrade blocked")
    state = "available" if comparison > 0 else "current"
    reason = "update_available" if state == "available" else "already_current"
    return {
        "state": state,
        "reason": reason,
        "version": version,
        "url": package_url,
        "release_url": release_url,
        "sha256": manifest["sha256"].lower(),
        "size_bytes": manifest["size_bytes"],
        "product": manifest["product"],
        "channel": manifest["channel"],
        "sequence": manifest["sequence"],
        "notes": manifest["notes"],
        "min_supported_version": min_supported,
        "windows_code_signed": manifest["windows_code_signed"],
        "manifest_trusted": True,
        "key_id": key_id,
        "manifest_sha256": manifest_digest,
        "manifest": manifest,
        "manifest_bytes": manifest_bytes,
        "signature_envelope_bytes": signature_envelope_bytes,
    }


def verify_package(path: str | Path, *, expected_sha256: str, expected_size: int) -> None:
    """Verify exact staged bytes before they can be handed to the supervisor."""
    package = Path(path)
    if not _SHA256.fullmatch(str(expected_sha256)):
        raise PackageVerificationError("invalid expected SHA-256")
    if not isinstance(expected_size, int) or isinstance(expected_size, bool) or not 1 <= expected_size <= MAX_PACKAGE_BYTES:
        raise PackageVerificationError("invalid expected package size")
    try:
        if package.stat().st_size != expected_size:
            raise PackageVerificationError("package size mismatch")
        digest = hashlib.sha256()
        with package.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
    except OSError as exc:
        raise PackageVerificationError("package cannot be read") from exc
    if not hmac.compare_digest(digest.hexdigest(), expected_sha256.lower()):
        raise PackageVerificationError("package SHA-256 mismatch")
