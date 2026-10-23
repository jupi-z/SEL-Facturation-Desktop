"""Single-worker SafeUpdate V1 coordinator.

The coordinator owns update state outside the Flask session.  Its public
snapshot is JSON-safe and intentionally omits manifest/signature bytes.
"""
from __future__ import annotations

from datetime import datetime, timezone
import logging
from pathlib import Path
from threading import Event, Lock, Thread
from urllib.error import HTTPError, URLError

import db
from safeupdate_keys import TRUSTED_UPDATE_KEYS
from update_client import download_package, fetch_manifest, staging_dir
from update_transaction import mark_transaction, verify_transaction


LOGGER = logging.getLogger(__name__)
TERMINAL = {"idle", "current", "available", "ready", "failed", "offline", "unavailable", "invalid", "cancelled", "recovery_required"}
BUSY = {"checking", "downloading", "verifying", "backup", "closing", "installing", "postcheck"}


class SafeUpdateCoordinator:
    def __init__(self, *, manifest_url: str, current_version: str, trusted_keys=None,
                 fetcher=fetch_manifest, downloader=download_package, backupper=db.pre_update_backup,
                 staging_root=None):
        self.manifest_url = manifest_url
        self.current_version = current_version
        self.trusted_keys = TRUSTED_UPDATE_KEYS if trusted_keys is None else trusted_keys
        self.fetcher = fetcher
        self.downloader = downloader
        self.backupper = backupper
        self.staging_root = Path(staging_root) if staging_root is not None else staging_dir()
        self._lock = Lock()
        self._worker: Thread | None = None
        self._cancel = Event()
        self._authenticated_update: dict | None = None
        self._state = {
            "state": "idle", "version": current_version, "phase": "idle",
            "message": "Aucune vérification lancée.", "received_bytes": 0,
            "total_bytes": 0, "updated_at": self._now(),
        }

    @staticmethod
    def _now():
        return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

    def snapshot(self) -> dict:
        with self._lock:
            return dict(self._state)

    def _set(self, **changes):
        with self._lock:
            self._state.update(changes, updated_at=self._now())

    def _start(self, target, *, initial_state=None):
        with self._lock:
            if self._worker and self._worker.is_alive():
                return False
            if initial_state:
                self._state.update(initial_state, updated_at=self._now())
            self._cancel = Event()
            self._worker = Thread(target=target, name="sel-safeupdate", daemon=True)
            self._worker.start()
            return True

    def start_background_check(self) -> bool:
        return self.start_check(background=True)

    def start_check(self, *, background=False) -> bool:
        def work():
            try:
                update = self.fetcher(
                    self.manifest_url, current_version=self.current_version,
                    trusted_keys=self.trusted_keys,
                )
                with self._lock:
                    self._authenticated_update = dict(update) if update.get("state") == "available" else None
                safe = {key: value for key, value in update.items() if key not in {"manifest_bytes", "signature_envelope_bytes", "manifest"}}
                safe.update(state=update["state"], phase=update["state"], version=update.get("version", self.current_version))
                if update["state"] == "available":
                    safe["message"] = f"La version {update['version']} est disponible."
                elif update["state"] == "current":
                    safe["message"] = f"Vous utilisez déjà la dernière version disponible ({safe['version']})."
                else:
                    safe["message"] = "La recherche de mise à jour est terminée."
                self._set(**safe)
            except HTTPError as exc:
                with self._lock:
                    self._authenticated_update = None
                LOGGER.warning("SafeUpdate endpoint returned HTTP %s", exc.code)
                if exc.code == 404:
                    message = "Le canal de mise à jour n’est pas encore configuré. SEL Facturation reste disponible."
                else:
                    message = f"Le serveur de mise à jour est momentanément indisponible (HTTP {exc.code})."
                self._set(state="unavailable", phase="failed", message=message)
            except (URLError, ConnectionError, TimeoutError, OSError) as exc:
                with self._lock:
                    self._authenticated_update = None
                LOGGER.info("SafeUpdate hors ligne: %s", exc)
                self._set(state="offline", phase="failed", message="Le serveur de mise à jour est actuellement injoignable.")
            except Exception as exc:
                with self._lock:
                    self._authenticated_update = None
                LOGGER.warning("SafeUpdate manifest refused", exc_info=True)
                self._set(state="invalid", phase="failed", message="Le canal de mise à jour a été refusé après vérification cryptographique.")
        return self._start(work, initial_state={
            "state": "checking", "phase": "checking", "message": "Recherche d’une mise à jour…",
        })

    def start_download(self) -> bool:
        with self._lock:
            update = dict(self._state)
            authenticated_update = dict(self._authenticated_update or {})
        if update.get("state") != "available":
            return False
        update = authenticated_update

        def work():
            self._set(state="downloading", phase="downloading", message="Téléchargement sécurisé…", received_bytes=0, total_bytes=update.get("size_bytes", 0))
            try:
                result = self.downloader(
                    update, self.staging_root, current_version=self.current_version,
                    trusted_keys=self.trusted_keys,
                    cancel_event=self._cancel,
                    progress=lambda received, total: self._set(received_bytes=received, total_bytes=total),
                )
                self._set(state="verifying", phase="verifying", message="Vérification du package…", transaction_id=result["transaction_id"])
                verified = verify_transaction(
                    result["transaction_id"], staging_root=self.staging_root,
                    current_version=self.current_version, trusted_keys=self.trusted_keys,
                )
                self._set(state="backup", phase="backup", message="Sauvegarde vérifiée des données locales…")
                backup = self.backupper()
                if db.db_path().exists() and (backup is None or not Path(backup).is_file() or Path(backup).stat().st_size <= 0):
                    raise OSError("Sauvegarde SQLite indisponible")
                mark_transaction(result["transaction_id"], "ready", staging_root=self.staging_root,
                                 backup_path=str(backup) if backup else "")
                self._set(
                    state="ready", phase="ready", message="Mise à jour vérifiée et prête à être installée.",
                    transaction_id=result["transaction_id"], filename=result["filename"],
                    version=verified["version"], sha256=result["sha256"],
                    backup_name=Path(backup).name if backup else "",
                    backup_path=str(backup) if backup else "",
                    received_bytes=result["size_bytes"], total_bytes=result["size_bytes"],
                )
            except ValueError as exc:
                if self._cancel.is_set():
                    self._set(state="cancelled", phase="cancelled", message="Téléchargement annulé.")
                else:
                    self._set(state="failed", phase="failed", message=f"Préparation refusée : {exc}")
            except Exception as exc:
                LOGGER.warning("SafeUpdate preparation failed", exc_info=True)
                self._set(state="recovery_required", phase="recovery_required", message="La mise à jour est bloquée : la sauvegarde vérifiée n’a pas pu être confirmée.")
        return self._start(work)

    def cancel(self) -> bool:
        with self._lock:
            if not self._worker or not self._worker.is_alive() or self._state.get("state") not in BUSY:
                return False
            self._cancel.set()
            return True
