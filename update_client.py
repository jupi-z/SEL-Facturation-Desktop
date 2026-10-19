"""Network and package staging primitives for the Windows updater.

These functions never execute a downloaded file. The caller must verify the
Authenticode publisher and ask the user before handing the staged bootstrapper
to the native installer.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
from tempfile import NamedTemporaryFile
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from update_policy import evaluate_manifest, verify_sha256


MAX_MANIFEST_BYTES = 128 * 1024
MAX_PACKAGE_BYTES = 512 * 1024 * 1024
USER_AGENT = "SEL-Facturation-Updater/1"


def _https(url: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.netloc:
        raise ValueError("URL https obligatoire")


def fetch_manifest(url: str, *, current_version: str, opener=urlopen) -> dict:
    _https(url)
    request = Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"})
    with opener(request, timeout=5) as response:
        _https(response.geturl())
        payload = response.read(MAX_MANIFEST_BYTES + 1)
    if len(payload) > MAX_MANIFEST_BYTES:
        raise ValueError("Manifest trop volumineux")
    try:
        manifest = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError("Manifest JSON invalide") from exc
    if not isinstance(manifest, dict):
        raise ValueError("Manifest invalide")
    decision = evaluate_manifest(manifest, current_version=current_version)
    if decision["state"] == "rejected":
        raise ValueError(decision["reason"])
    return decision


def download_package(manifest: dict, destination: str | Path, *, current_version: str,
                     expected_publisher: str | None = None,
                     signature_verifier=None, opener=urlopen) -> Path:
    if not expected_publisher:
        raise ValueError("signature publisher required")
    decision = evaluate_manifest(manifest, current_version=current_version)
    if decision["state"] != "available":
        raise ValueError(decision.get("reason", "Aucune mise à jour disponible"))
    url = decision["url"]
    _https(url)
    target_dir = Path(destination)
    target_dir.mkdir(parents=True, exist_ok=True)
    target = target_dir / f"SEL-Facturation-Setup-{decision['version']}.exe"
    request = Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/octet-stream"})
    with opener(request, timeout=30) as response:
        _https(response.geturl())
        with NamedTemporaryFile(prefix=".sel-update-", suffix=".tmp", dir=target_dir, delete=False) as temp:
            temporary = Path(temp.name)
            total = 0
            try:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > MAX_PACKAGE_BYTES:
                        raise ValueError("Package de mise à jour trop volumineux")
                    temp.write(chunk)
            except Exception:
                temporary.unlink(missing_ok=True)
                raise
    if not verify_sha256(temporary, decision["sha256"]):
        temporary.unlink(missing_ok=True)
        raise ValueError("Hash SHA-256 du package invalide")
    verifier = signature_verifier or verify_authenticode
    if not verifier(temporary, expected_subject=expected_publisher):
        temporary.unlink(missing_ok=True)
        raise ValueError("Signature Authenticode invalide")
    os.replace(temporary, target)
    return target


def verify_authenticode(path: str | Path, *, expected_subject: str) -> bool:
    """Return true only for a valid Authenticode signature by the expected publisher."""
    if os.name != "nt" or not expected_subject:
        return False
    escaped = str(Path(path)).replace("'", "''")
    command = (
        f"$s=Get-AuthenticodeSignature -LiteralPath '{escaped}'; "
        "if ($s.Status -ne 'Valid' -or -not $s.SignerCertificate) { exit 1 }; "
        "$s.SignerCertificate.Subject"
    )
    result = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", command],
        capture_output=True, text=True, timeout=15, check=False,
    )
    return result.returncode == 0 and expected_subject.lower() in result.stdout.lower()
