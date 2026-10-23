"""Generate the exact SafeUpdate V1 manifest and detached Ed25519 signature.

This tool is intentionally offline. It never publishes files and never reads
a secret from GitHub Actions. The private key must be provisioned outside the
repository and is used only for the signing operation.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timedelta, timezone
import getpass
import hashlib
import json
from pathlib import Path
import sys

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from safeupdate_crypto import MAX_PACKAGE_BYTES, _safe_https_url, _version


def _args():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--package", type=Path, required=True)
    p.add_argument("--version", required=True)
    p.add_argument("--sequence", type=int, required=True)
    p.add_argument("--key-id", required=True)
    p.add_argument("--private-key", type=Path, required=True)
    p.add_argument("--release-url", required=True)
    p.add_argument("--package-url", required=True)
    p.add_argument("--output-dir", type=Path, required=True)
    p.add_argument("--min-supported-version", default="0.3.24")
    p.add_argument("--channel", choices=("stable", "preview"), default="stable")
    p.add_argument("--notes", default="")
    p.add_argument("--expires-days", type=int, default=30)
    p.add_argument("--windows-code-signed", action="store_true")
    p.add_argument("--password-env", default="SEL_SAFEUPDATE_KEY_PASSWORD")
    p.add_argument("--no-password", action="store_true", help="Allow an unencrypted test key only; never use for production.")
    return p.parse_args()


def _inside_repo(path: Path) -> bool:
    try:
        path.resolve().relative_to(ROOT.resolve())
    except ValueError:
        return False
    return True


def _load_key(path: Path, args) -> Ed25519PrivateKey:
    if _inside_repo(path):
        raise ValueError("La clé privée doit rester hors du dépôt.")
    raw = path.read_bytes()
    password = None
    if not args.no_password:
        import os
        value = os.environ.get(args.password_env)
        password = (value or getpass.getpass("Phrase secrète de la clé SafeUpdate : ")).encode()
    key = serialization.load_pem_private_key(raw, password=password)
    if not isinstance(key, Ed25519PrivateKey):
        raise ValueError("La clé n'est pas une clé privée Ed25519.")
    return key


def main() -> int:
    args = _args()
    _version(args.version)
    _version(args.min_supported_version)
    if args.sequence <= 0 or not 1 <= args.expires_days <= 365:
        raise ValueError("sequence/expiration invalides")
    package = args.package.resolve()
    if not package.is_file() or package.stat().st_size > MAX_PACKAGE_BYTES:
        raise ValueError("package absent ou trop volumineux")
    expected_name = f"SEL-Facturation-Setup-{args.version}.exe"
    if package.name != expected_name:
        raise ValueError(f"Le package doit s'appeler {expected_name}.")
    package_url = _safe_https_url(args.package_url, expected_filename=expected_name)
    release_url = _safe_https_url(args.release_url)
    key = _load_key(args.private_key.resolve(), args)
    now = datetime.now(timezone.utc).replace(microsecond=0)
    digest = hashlib.sha256()
    size = 0
    with package.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            size += len(chunk)
            digest.update(chunk)
    if not 1 <= size <= MAX_PACKAGE_BYTES:
        raise ValueError("package vide ou trop volumineux")
    manifest = {
        "schema_version": 2, "product": "SEL-Facturation", "channel": args.channel,
        "version": args.version, "sequence": args.sequence,
        "created_at": now.isoformat().replace("+00:00", "Z"),
        "expires_at": (now + timedelta(days=args.expires_days)).isoformat().replace("+00:00", "Z"),
        "min_supported_version": args.min_supported_version, "package_type": "bootstrapper",
        "windows_code_signed": bool(args.windows_code_signed), "url": package_url,
        "release_url": release_url, "size_bytes": size,
        "sha256": digest.hexdigest(), "notes": args.notes,
    }
    manifest_bytes = json.dumps(manifest, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    envelope = {
        "schema": 1, "key_id": args.key_id,
        "signature_b64": base64.b64encode(key.sign(manifest_bytes)).decode("ascii"),
    }
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    (output / "latest.json").write_bytes(manifest_bytes)
    (output / "latest.sig.json").write_bytes(json.dumps(envelope, separators=(",", ":")).encode("utf-8"))
    print(json.dumps({"manifest": str(output / "latest.json"), "signature": str(output / "latest.sig.json"), "sha256": manifest["sha256"], "size_bytes": size}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"SafeUpdate release refused: {exc}", file=sys.stderr)
        raise SystemExit(1)
