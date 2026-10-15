"""Secure, offline-tolerant update primitives for the Windows bootstrapper."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import os
import tempfile
from datetime import datetime
from urllib.parse import urlparse
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

import db


MAX_UPDATE_SIZE = 250 * 1024 * 1024
DEFAULT_MANIFEST_URL = "https://sel.shuleops.com/stable/update.json"


def staging_dir() -> Path:
    return Path(tempfile.gettempdir()) / "SEL-Facturation-Updater"
VERSION_PATTERN = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
HASH_PATTERN = re.compile(r"^[0-9a-fA-F]{64}$")


class ManifestError(ValueError):
    """Manifest or downloaded package failed a safety check."""


def _version(value: str) -> tuple[int, int, int]:
    if not isinstance(value, str) or not VERSION_PATTERN.fullmatch(value):
        raise ManifestError("Version de mise à jour invalide.")
    return tuple(int(part) for part in value.split("."))


def _https_url(value: str, label: str) -> str:
    if not isinstance(value, str) or urlparse(value).scheme.lower() != "https":
        raise ManifestError(f"{label} HTTPS obligatoire.")
    return value


@dataclass(frozen=True)
class UpdateManifest:
    schema: int
    version: str
    url: str
    sha256: str
    size: int
    published_at: str
    release_notes: str
    source_url: str

    @classmethod
    def from_payload(cls, payload: bytes, source_url: str) -> "UpdateManifest":
        _https_url(source_url, "Source du manifeste")
        try:
            raw = json.loads(payload.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise ManifestError("Manifeste JSON invalide.") from exc
        if not isinstance(raw, dict) or raw.get("schema") != 1:
            raise ManifestError("Version de manifeste non prise en charge.")
        version = raw.get("version")
        _version(version)
        url = _https_url(raw.get("url"), "URL de mise à jour")
        if not urlparse(url).path.lower().endswith(".exe"):
            raise ManifestError("L'installateur doit être un fichier .exe.")
        sha256 = raw.get("sha256")
        if not isinstance(sha256, str) or not HASH_PATTERN.fullmatch(sha256):
            raise ManifestError("SHA-256 de mise à jour invalide.")
        size = raw.get("size")
        if isinstance(size, bool) or not isinstance(size, int) or not 0 < size <= MAX_UPDATE_SIZE:
            raise ManifestError("Taille de mise à jour invalide.")
        published_at = raw.get("published_at", "")
        release_notes = raw.get("release_notes", "")
        if not isinstance(published_at, str) or not isinstance(release_notes, str):
            raise ManifestError("Métadonnées de mise à jour invalides.")
        return cls(1, version, url, sha256.lower(), size, published_at, release_notes, source_url)

    def is_newer_than(self, current: str) -> bool:
        return _version(self.version) > _version(current)


@dataclass(frozen=True)
class UpdateCheckResult:
    status: str
    manifest: UpdateManifest | None = None
    message: str = ""


def check_for_update(manifest_url: str, current_version: str, timeout: float = 3.0) -> UpdateCheckResult:
    _https_url(manifest_url, "Source du manifeste")
    _version(current_version)
    try:
        request = Request(manifest_url, headers={"User-Agent": "SEL-Facturation-Updater/1"})
        with urlopen(request, timeout=timeout) as response:
            final_url = response.geturl()
            payload = response.read(1024 * 1024)
        manifest = UpdateManifest.from_payload(payload, final_url)
        if not manifest.is_newer_than(current_version):
            return UpdateCheckResult("current")
        return UpdateCheckResult("available", manifest)
    except HTTPError as exc:
        if exc.code == 404:
            return UpdateCheckResult(
                "not_published",
                message="Le canal de mise à jour n’est pas encore publié.",
            )
        return UpdateCheckResult(
            "unavailable",
            message="Le serveur de mise à jour est temporairement indisponible.",
        )
    except URLError:
        return UpdateCheckResult(
            "unavailable",
            message="Impossible de joindre le service de mise à jour. Vérifiez votre connexion ou réessayez.",
        )
    except TimeoutError:
        return UpdateCheckResult(
            "unavailable",
            message="Le service de mise à jour n’a pas répondu à temps. Réessayez.",
        )
    except ManifestError as exc:
        return UpdateCheckResult("invalid", message=str(exc))
    except Exception:
        return UpdateCheckResult(
            "unavailable",
            message="Le service de mise à jour est temporairement indisponible.",
        )


def verify_sha256(path: Path, expected: str) -> None:
    if not HASH_PATTERN.fullmatch(expected or ""):
        raise ManifestError("SHA-256 attendu invalide.")
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected.lower():
        raise ManifestError("L'intégrité de l'installateur n'a pas pu être vérifiée.")


def download_update(manifest: UpdateManifest, destination_dir: Path, timeout: float = 30.0) -> Path:
    destination_dir = Path(destination_dir)
    destination_dir.mkdir(parents=True, exist_ok=True)
    target = destination_dir / Path(urlparse(manifest.url).path).name
    if target.suffix.lower() != ".exe":
        raise ManifestError("Installateur inattendu.")
    fd, temporary_name = tempfile.mkstemp(prefix=f".{target.name}.", suffix=".part", dir=destination_dir)
    os.close(fd)
    temporary = Path(temporary_name)
    try:
        request = Request(manifest.url, headers={"User-Agent": "SEL-Facturation-Updater/1"})
        with temporary.open("wb") as temp:
            with urlopen(request, timeout=timeout) as response:
                if urlparse(response.geturl()).scheme.lower() != "https":
                    raise ManifestError("La redirection de téléchargement doit rester en HTTPS.")
                total = 0
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    total += len(chunk)
                    if total > manifest.size or total > MAX_UPDATE_SIZE:
                        raise ManifestError("Installateur trop volumineux.")
                    temp.write(chunk)
        if total != manifest.size:
            raise ManifestError("Taille de l'installateur incorrecte.")
        verify_sha256(temporary, manifest.sha256)
        temporary.replace(target)
        return target
    except Exception:
        temporary.unlink(missing_ok=True)
        raise


def create_pre_update_backup() -> Path:
    """Create and verify a consistent backup before an installer is launched."""
    folder = db.home() / "sauvegardes"
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    destination = folder / f"sel-pre-update-{stamp}.sqlite3"
    return db.backup(destination)
