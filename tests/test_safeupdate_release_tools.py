import base64
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


ROOT = Path(__file__).resolve().parents[1]


def _private_key_file(tmp_path):
    key = Ed25519PrivateKey.generate()
    path = tmp_path / "release-key.pem"
    path.write_bytes(key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    ))
    return key, path


def test_generate_and_verify_signed_update_offline(tmp_path):
    private, key_path = _private_key_file(tmp_path)
    package = tmp_path / "SEL-Facturation-Setup-0.4.0.exe"
    package.write_bytes(b"approved package")
    public = base64.b64encode(private.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw,
    )).decode("ascii")
    output = tmp_path / "published"
    generate = subprocess.run([
        sys.executable, str(ROOT / "tools" / "generate_signed_update.py"),
        "--package", str(package), "--version", "0.4.0", "--sequence", "41",
        "--key-id", "test-key", "--private-key", str(key_path),
        "--release-url", "https://updates.example.test/releases/v0.4.0",
        "--package-url", "https://updates.example.test/releases/SEL-Facturation-Setup-0.4.0.exe",
        "--output-dir", str(output), "--expires-days", "30", "--no-password",
    ], capture_output=True, text=True, check=False)
    assert generate.returncode == 0, generate.stderr
    verify = subprocess.run([
        sys.executable, str(ROOT / "tools" / "verify_signed_update.py"),
        "--manifest", str(output / "latest.json"),
        "--signature", str(output / "latest.sig.json"),
        "--package", str(package), "--key-id", "test-key", "--public-key-base64", public,
        "--current-version", "0.3.24",
    ], capture_output=True, text=True, check=False)
    assert verify.returncode == 0, verify.stderr
    result = json.loads(verify.stdout)
    assert result["manifest_trusted"] is True
    assert result["sha256"] == hashlib.sha256(package.read_bytes()).hexdigest()


def test_generator_refuses_private_key_inside_repository(tmp_path):
    _, key_path = _private_key_file(ROOT)
    package = tmp_path / "SEL-Facturation-Setup-0.4.0.exe"
    package.write_bytes(b"x")
    result = subprocess.run([
        sys.executable, str(ROOT / "tools" / "generate_signed_update.py"),
        "--package", str(package), "--version", "0.4.0", "--sequence", "1",
        "--key-id", "test-key", "--private-key", str(key_path),
        "--release-url", "https://updates.example.test/releases/v0.4.0",
        "--package-url", "https://updates.example.test/releases/SEL-Facturation-Setup-0.4.0.exe",
        "--output-dir", str(tmp_path / "out"), "--no-password",
    ], capture_output=True, text=True, check=False)
    assert result.returncode != 0
    key_path.unlink(missing_ok=True)
