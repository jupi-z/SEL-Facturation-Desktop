"""Ancien lanceur V1, conservé pour l'historique et la migration."""
from __future__ import annotations

import base64
import ctypes
import logging
import os
import sqlite3
import subprocess
import webbrowser
from logging.handlers import RotatingFileHandler
from pathlib import Path
import re
import sys
import threading
import time
from urllib.parse import urlsplit
from uuid import uuid4

from werkzeug.serving import make_server
import db
from installer_version import is_canonical_installation_executable, write_installation_marker
from safeupdate_keys import TRUSTED_UPDATE_KEYS
from safeupdate_supervisor import spawn as spawn_update_supervisor
from update_client import staging_dir
from update_transaction import claim_transaction, release_transaction_lock, verify_transaction
from version import APP_VERSION


DOCUMENT_NAME = re.compile(r"SEL-\d{4}-\d{6}\.pdf\Z", re.IGNORECASE)
MAX_PDF_BASE64 = 8 * 1024 * 1024
TEMPORARY_PDF_RETENTION_SECONDS = 7 * 24 * 60 * 60
EXTERNAL_URL_SCHEMES = frozenset(("http", "https", "mailto", "tel"))


class DesktopDocuments:
    """Pont natif pour les documents PDF obtenus via une route Flask authentifiée.

    Le navigateur intégré télécharge d'abord le PDF via fetch (avec le cookie
    de session), puis transmet uniquement les octets au pont Python.
    Ce pont ne possède aucune méthode de lecture directe de la base métier.
    """

    def __init__(self):
        # Keep the native window private. pywebview exposes public attributes
        # of the JS bridge and would recursively walk WinForms AccessibilityObject.
        self._window = None

    def _validate_pdf(self, filename: str, payload: str) -> bytes:
        if not isinstance(filename, str) or DOCUMENT_NAME.fullmatch(filename) is None:
            raise ValueError("Nom de facture non autorisé.")
        if not isinstance(payload, str) or len(payload) > MAX_PDF_BASE64:
            raise ValueError("Document PDF trop volumineux.")
        try:
            binary = base64.b64decode(payload, validate=True)
        except (ValueError, base64.binascii.Error) as exc:
            raise ValueError("Contenu PDF incorrect.") from exc
        if not binary.startswith(b"%PDF-") or len(binary) > 6 * 1024 * 1024:
            raise ValueError("Le document reçu n'est pas un PDF valide.")
        return binary

    def _temporary_pdf(self, filename: str, content: bytes) -> Path:
        folder = db.home() / "documents-temporaires"
        folder.mkdir(parents=True, exist_ok=True)
        cutoff = time.time() - TEMPORARY_PDF_RETENTION_SECONDS
        for old_file in folder.glob("*.pdf"):
            try:
                if old_file.is_file() and old_file.stat().st_mtime < cutoff:
                    old_file.unlink()
            except OSError:
                logging.warning("Impossible de purger le PDF temporaire %s", old_file)
        target = folder / (uuid4().hex + "-" + filename)
        target.write_bytes(content)
        return target

    def open_pdf(self, filename: str, payload: str) -> dict:
        """Ouvrir dans le lecteur PDF Windows, sans déplacer l'application."""
        try:
            content = self._validate_pdf(filename, payload)
            target = self._temporary_pdf(filename, content)
            os.startfile(str(target))  # Windows : application PDF associée
            return {"ok": True}
        except Exception as exc:
            logging.exception("Impossible d'ouvrir le document")
            return {"ok": False, "message": str(exc)}

    def save_pdf(self, filename: str, payload: str) -> dict:
        """Boîte de dialogue Enregistrer sous native Windows."""
        try:
            import webview
            content = self._validate_pdf(filename, payload)
            selected = self._window.create_file_dialog(
                webview.FileDialog.SAVE,
                save_filename=filename,
                file_types=("Documents PDF (*.pdf)",),
            )
            if not selected:
                return {"ok": True, "cancelled": True}
            target = Path(selected[0])
            if target.suffix.lower() != ".pdf":
                target = target.with_suffix(".pdf")
            target.write_bytes(content)
            return {"ok": True}
        except Exception as exc:
            logging.exception("Impossible d'enregistrer le document")
            return {"ok": False, "message": str(exc)}

    def print_pdf(self, filename: str, payload: str) -> dict:
        """Utiliser la commande Windows d'impression du lecteur PDF associé."""
        try:
            content = self._validate_pdf(filename, payload)
            target = self._temporary_pdf(filename, content)
            try:
                os.startfile(str(target), "print")
            except OSError as print_error:
                # The print verb is optional in Windows PDF associations. Keep
                # the document available in the associated viewer so the user
                # can print it with Ctrl+P instead of losing the action.
                os.startfile(str(target))
                return {
                    "ok": False,
                    "fallback": "open",
                    "message": "Impression automatique indisponible. Le PDF est ouvert : utilisez Ctrl+P.",
                    "detail": str(print_error),
                }
            return {"ok": True}
        except Exception as exc:
            logging.exception("Impossible d'imprimer le document")
            return {"ok": False, "message": str(exc)}

    @staticmethod
    def _validate_external_url(url: str) -> str:
        if not isinstance(url, str) or not url.strip() or len(url) > 2048:
            raise ValueError("Adresse externe invalide.")
        candidate = url.strip()
        if any(char in candidate for char in "\r\n"):
            raise ValueError("Adresse externe invalide.")
        try:
            parsed = urlsplit(candidate)
        except ValueError as exc:
            raise ValueError("Adresse externe invalide.") from exc
        scheme = parsed.scheme.lower()
        if scheme not in EXTERNAL_URL_SCHEMES:
            raise ValueError("Protocole externe non autorisé.")
        if scheme in {"http", "https"}:
            if not parsed.hostname or parsed.username or parsed.password:
                raise ValueError("Adresse web externe invalide.")
        elif scheme == "mailto":
            if parsed.netloc or not parsed.path:
                raise ValueError("Adresse e-mail externe invalide.")
        elif not parsed.path:
            raise ValueError("Adresse externe invalide.")
        return candidate

    def open_external_url(self, url: str) -> dict:
        """Ouvrir une URL autorisée dans le gestionnaire externe de Windows."""
        try:
            target = self._validate_external_url(url)
            if os.name == "nt" and hasattr(os, "startfile"):
                os.startfile(target)
            elif not webbrowser.open(target, new=2, autoraise=True):
                raise OSError("Aucun gestionnaire externe disponible.")
            return {"ok": True}
        except Exception as exc:
            logging.exception("Impossible d'ouvrir le lien externe")
            return {"ok": False, "message": str(exc)}

    def install_update(self, transaction_id: str) -> dict:
        """Hand only a fully revalidated opaque transaction to the supervisor."""
        try:
            verified = verify_transaction(
                transaction_id, staging_root=staging_dir(), current_version=APP_VERSION,
                trusted_keys=TRUSTED_UPDATE_KEYS,
            )
            backup_path = verified.get("backup_path")
            if backup_path and not Path(backup_path).is_file():
                raise ValueError("La sauvegarde vérifiée est introuvable.")
            claim_transaction(transaction_id, staging_root=staging_dir())
            try:
                spawn_update_supervisor(transaction_id, os.getpid())
            except Exception:
                release_transaction_lock(transaction_id, staging_root=staging_dir())
                raise
            if self._window is not None:
                threading.Timer(0.5, self._window.destroy).start()
            return {"ok": True, "transaction_id": transaction_id}
        except Exception as exc:
            logging.exception("Impossible de lancer la mise à jour")
            return {"ok": False, "message": str(exc)}


def _acquire_single_instance():
    """Empêcher deux fenêtres concurrentes d'ouvrir la même base sur Windows."""
    if os.name != "nt":
        return None
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateMutexW.argtypes = (ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p)
    kernel32.CreateMutexW.restype = ctypes.c_void_p
    mutex = kernel32.CreateMutexW(None, False, "Local\\SEL-Facturation-Desktop")
    if not mutex:
        raise OSError("Verrouillage de l'application impossible.")
    if ctypes.get_last_error() == 183:  # ERROR_ALREADY_EXISTS
        kernel32.CloseHandle(mutex)
        ctypes.windll.user32.MessageBoxW(
            None,
            "SEL Facturation est déjà ouvert sur cet ordinateur.",
            "SEL Facturation",
            0x40,
        )
        return False
    return mutex


def startup_error_message(error: Exception) -> str:
    """Return an actionable native startup message for the failing layer."""
    if isinstance(error, sqlite3.Error):
        return (
            "Le démarrage a échoué lors de l'ouverture ou de la mise à niveau "
            "de la base de données locale. Aucune suppression de données n'est "
            "effectuée automatiquement. Consultez application.log dans le "
            "dossier de données SEL."
        )
    if isinstance(error, ModuleNotFoundError):
        missing_module = error.name or str(error).strip()
        return (
            "L'installation de SEL Facturation est incomplète : un composant "
            f"({missing_module}) est absent. Réinstallez la dernière version "
            "depuis l'installateur officiel. Consultez application.log dans "
            "le dossier de données SEL."
        )
    detail = str(error).strip().splitlines()[0][:220] if str(error).strip() else type(error).__name__
    if "webview2" in detail.casefold() or "edgechromium" in detail.casefold():
        return (
            "Le démarrage de la fenêtre Windows a échoué avec WebView2. "
            "Réparez SEL depuis son installateur officiel et vérifiez que "
            "Microsoft Edge WebView2 Runtime peut être installé sur ce poste. "
            "Consultez application.log dans le dossier de données SEL."
        )
    return (
        f"Le démarrage de SEL Facturation a échoué ({detail}). Consultez "
        "application.log dans le dossier de données SEL."
    )


def main(argv=None):
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments == ["--sel-packaging-smoke"]:
        # CI executes the actual frozen one-file EXE in a data-free mode. This
        # imports the complete app/PDF stack, including ReportLab's dynamic
        # barcode modules, without opening a window or touching SEL data.
        try:
            import app  # noqa: F401
            from reportlab.graphics.barcode import code128  # noqa: F401
        except Exception:
            logging.exception("Échec du contrôle d'import du paquet SEL Facturation")
            return 1
        return 0
    if arguments and arguments[0] == "--sel-update-supervisor":
        from safeupdate_supervisor import main as supervisor_main
        return supervisor_main(arguments)
    print(
        "Le client local SEL V1 est retiré et ne peut plus démarrer depuis les sources. "
        "Utilisez le client Windows connecté au serveur SEL.",
        file=sys.stderr,
    )
    return 2

    db.home().mkdir(parents=True, exist_ok=True)
    # A standalone EXE is allowed for diagnostics and portable use. Record
    # only copies that have actually been launched so the setup preflight can
    # identify them later; the official installed path is registry-owned.
    executable = Path(sys.executable if getattr(sys, "frozen", False) else __file__).resolve()
    if getattr(sys, "frozen", False) and not is_canonical_installation_executable(executable):
        try:
            write_installation_marker(executable, APP_VERSION, db.home())
        except (OSError, ValueError):
            logging.warning("Impossible d'enregistrer le marqueur d'installation portable", exc_info=True)
    logging.basicConfig(
        level=logging.INFO,
        handlers=[
            RotatingFileHandler(
                db.home() / "application.log",
                maxBytes=1024 * 1024,
                backupCount=2,
                encoding="utf-8",
            )
        ],
    )
    mutex = _acquire_single_instance()
    if mutex is False:
        return
    server = None
    try:
        from app import create_app
        import webview

        application = create_app()
        application.extensions["safeupdate"].start_background_check()
        # Port dynamique réservé par Werkzeug. Jamais accessible sur le réseau.
        server = make_server("127.0.0.1", 0, application, threaded=True)
        server_thread = threading.Thread(
            target=server.serve_forever,
            name="sel-flask-local",
            daemon=True,
        )
        server_thread.start()

        webview.settings["ALLOW_DOWNLOADS"] = True
        bridge = DesktopDocuments()
        bridge._window = webview.create_window(
            "SEL Facturation — Société d'Énergie du Lualaba",
            f"http://127.0.0.1:{server.server_port}/",
            js_api=bridge,
            width=1240,
            height=790,
            min_size=(900, 600),
            background_color="#F5F9FC",
            text_select=True,
        )
        # WebView2 : l'interface ne nécessite plus de navigateur externe.
        webview.start(gui="edgechromium", debug=False)
    except Exception as error:
        logging.exception("Échec du démarrage de SEL Facturation")
        if os.name == "nt":
            ctypes.windll.user32.MessageBoxW(
                None,
                startup_error_message(error),
                "SEL Facturation",
                0x10,
            )
            return 1
        raise
    finally:
        if server is not None:
            server.shutdown()
            server.server_close()
        if os.name == "nt" and mutex not in (None, False):
            ctypes.windll.kernel32.CloseHandle(mutex)
    return 0


if __name__ == "__main__":
    sys.exit(main())
