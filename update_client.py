"""Authenticated SafeUpdate V1 transport and staging primitives.

The network layer only fetches and stages bytes. It never executes an
installer and it never treats Authenticode as the trust root. Trust comes
from the pinned Ed25519 key and the exact SHA-256/size declared by the
authenticated manifest.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import tempfile
from time import sleep
from urllib.parse import urlsplit, urlunsplit
from urllib.request import Request, urlopen

from safeupdate_crypto import (
    MAX_MANIFEST_BYTES,
    MAX_SIGNATURE_BYTES,
    ManifestVerificationError,
    _safe_https_url,
    verify_package,
    verify_signed_manifest,
)
from safeupdate_keys import DEFAULT_UPDATE_CHANNEL, TRUSTED_UPDATE_KEYS
from update_transaction import (
    default_staging_root,
    load_sequence_state,
    new_transaction_directory,
    save_sequence_state,
    state_path as default_state_path,
    write_transaction,
)


MAX_PACKAGE_BYTES = 512 * 1024 * 1024
USER_AGENT = "SEL-Facturation-SafeUpdate/1"
UPDATE_FILENAME = re.compile(r"SEL-Facturation-Setup-\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?\.exe\Z", re.IGNORECASE)


def staging_dir() -> Path:
    """Return the private user staging directory used by SafeUpdate."""
    return default_staging_root()


def _https(url: str, *, expected_filename: str | None = None) -> str:
    return _safe_https_url(url, expected_filename=expected_filename)


def _signature_url(manifest_url: str) -> str:
    parsed = urlsplit(_https(manifest_url, expected_filename="latest.json"))
    path = parsed.path.rsplit("/", 1)[0] + "/latest.sig.json"
    return urlunsplit((parsed.scheme, parsed.netloc, path, "", ""))


def _read_response(response, limit: int) -> bytes:
    payload = response.read(limit + 1)
    if len(payload) > limit:
        raise ValueError("Réponse de mise à jour trop volumineuse")
    return payload


def _fetch_bytes(url: str, *, limit: int, accept: str, opener):
    request = Request(url, headers={"User-Agent": USER_AGENT, "Accept": accept})
    with opener(request, timeout=15) as response:
        final_url = _https(response.geturl())
        return _read_response(response, limit), final_url


def fetch_manifest(
    url: str,
    *,
    current_version: str,
    trusted_keys: dict | None = None,
    state_path: str | Path | None = None,
    allowed_channel: str = DEFAULT_UPDATE_CHANNEL,
    opener=urlopen,
    now=None,
    sleep_fn=sleep,
) -> dict:
    """Fetch and authenticate the exact manifest and detached signature."""
    manifest_url = _https(url, expected_filename="latest.json")
    signature_url = _signature_url(manifest_url)
    keys = TRUSTED_UPDATE_KEYS if trusted_keys is None else trusted_keys
    sequence_path = Path(state_path) if state_path is not None else default_state_path()
    sequence_state = load_sequence_state(path=sequence_path)
    last_error = None
    for attempt in range(2):
        try:
            manifest_bytes, final_manifest_url = _fetch_bytes(
                manifest_url, limit=MAX_MANIFEST_BYTES, accept="application/json", opener=opener
            )
            signature_bytes, final_signature_url = _fetch_bytes(
                signature_url, limit=MAX_SIGNATURE_BYTES, accept="application/json", opener=opener
            )
            if final_manifest_url != manifest_url or final_signature_url != signature_url:
                raise ValueError("Redirection du canal de mise à jour refusée")
            result = verify_signed_manifest(
                manifest_bytes,
                signature_bytes,
                current_version=current_version,
                trusted_keys=keys,
                allowed_channel=allowed_channel,
                last_sequence=sequence_state["sequence"],
                last_manifest_sha256=sequence_state["manifest_sha256"] or None,
                now=now,
            )
            save_sequence_state(result["sequence"], result["manifest_sha256"], path=sequence_path)
            result["manifest_url"] = manifest_url
            result["signature_url"] = signature_url
            return result
        except (OSError, TimeoutError, ValueError, ManifestVerificationError) as exc:
            last_error = exc
            if attempt == 0:
                sleep_fn(0.25)
    raise last_error


def download_package(
    update: dict,
    destination: str | Path | None = None,
    *,
    current_version: str,
    trusted_keys: dict | None = None,
    progress=None,
    cancel_event=None,
    opener=urlopen,
) -> dict:
    """Download an authenticated package into an isolated transaction."""
    if not isinstance(update, dict) or update.get("state") != "available":
        raise ValueError("Aucune mise à jour disponible")
    if update.get("manifest_trusted") is not True:
        raise ValueError("Manifeste de mise à jour non authentifié")
    manifest_bytes = update.get("manifest_bytes")
    signature_bytes = update.get("signature_envelope_bytes")
    if not isinstance(manifest_bytes, bytes) or not isinstance(signature_bytes, bytes):
        raise ValueError("Preuves du manifeste manquantes")
    verified = verify_signed_manifest(
        manifest_bytes,
        signature_bytes,
        current_version=current_version,
        trusted_keys=TRUSTED_UPDATE_KEYS if trusted_keys is None else trusted_keys,
        allowed_channel=update.get("channel", DEFAULT_UPDATE_CHANNEL),
    )
    if verified.get("state") != "available" or verified["version"] != update.get("version"):
        raise ValueError("La mise à jour n’est plus disponible")
    package_url = _https(verified["url"])
    filename = Path(urlsplit(package_url).path).name
    if UPDATE_FILENAME.fullmatch(filename) is None:
        raise ValueError("Nom de package non autorisé")
    root = Path(destination) if destination is not None else staging_dir()
    root.mkdir(parents=True, exist_ok=True)
    transaction_id, transaction_root = new_transaction_directory(staging_root=root)
    temporary = transaction_root / ".package.partial"
    target = transaction_root / filename
    received = 0
    try:
        if cancel_event is not None and cancel_event.is_set():
            raise ValueError("download cancel")
        request = Request(package_url, headers={"User-Agent": USER_AGENT, "Accept": "application/octet-stream"})
        with opener(request, timeout=60) as response:
            if _https(response.geturl()) != package_url:
                raise ValueError("Redirection du package refusée")
            with temporary.open("wb") as stream:
                while True:
                    if cancel_event is not None and cancel_event.is_set():
                        raise ValueError("download cancel")
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    received += len(chunk)
                    if received > MAX_PACKAGE_BYTES or received > verified["size_bytes"]:
                        raise ValueError("Package de mise à jour trop volumineux")
                    stream.write(chunk)
                    if progress:
                        progress(received, verified["size_bytes"])
        if received != verified["size_bytes"]:
            raise ValueError("Taille du package inattendue")
        temporary.replace(target)
        verify_package(target, expected_sha256=verified["sha256"], expected_size=verified["size_bytes"])
        record = write_transaction(
            transaction_id,
            staging_root=root,
            package_name=filename,
            update={**verified, "manifest_bytes": manifest_bytes, "signature_bytes": signature_bytes},
        )
        return {
            "transaction_id": transaction_id,
            "filename": filename,
            "version": verified["version"],
            "sha256": verified["sha256"],
            "size_bytes": verified["size_bytes"],
            "key_id": verified["key_id"],
            "staging_root": str(root),
            "record": record,
        }
    except BaseException:
        temporary.unlink(missing_ok=True)
        target.unlink(missing_ok=True)
        try:
            transaction_root.rmdir()
        except OSError:
            pass
        raise


def verify_authenticode(path: str | Path, *, expected_subject: str) -> bool:
    """Optional diagnostic only; SafeUpdate V1 does not require Authenticode."""
    if os.name != "nt" or not expected_subject:
        return False
    escaped = str(Path(path)).replace("'", "''")
    command = (
        f"$s=Get-AuthenticodeSignature -LiteralPath '{escaped}'; "
        "if ($s.Status -ne 'Valid' -or -not $s.SignerCertificate) { exit 1 }; "
        "$s.SignerCertificate.Subject"
    )
    result = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", command],
        capture_output=True, text=True, timeout=15, check=False,
    )
    if result.returncode != 0:
        return False
    subject = re.sub(r"\s+", " ", result.stdout.strip()).casefold()
    expected = re.sub(r"\s+", " ", expected_subject.strip()).casefold()
    return subject in {expected, f"cn={expected}"}
