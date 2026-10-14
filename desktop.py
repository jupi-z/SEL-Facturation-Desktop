"""Fenêtre Windows native pour SEL Facturation (Flask reste strictement local)."""
from __future__ import annotations

import base64
import ctypes
import logging
import os
from logging.handlers import RotatingFileHandler
from pathlib import Path
import re
import threading
import time
from uuid import uuid4

from werkzeug.serving import make_server
import db


DOCUMENT_NAME = re.compile(r"SEL-\d{4}-\d{6}\.pdf\Z", re.IGNORECASE)
MAX_PDF_BASE64 = 8 * 1024 * 1024
TEMPORARY_PDF_RETENTION_SECONDS = 7 * 24 * 60 * 60


class DesktopDocuments:
    """Pont natif pour les documents PDF obtenus via une route Flask authentifiée.

    Le navigateur intégré télécharge d'abord le PDF via fetch (avec le cookie
    de session), puis transmet uniquement les octets au pont Python.
    Ce pont ne possède aucune méthode de lecture directe de la base métier.
    """

    def __init__(self):
        self.window = None

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
            selected = self.window.create_file_dialog(
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


def main():
    db.home().mkdir(parents=True, exist_ok=True)
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
        bridge.window = webview.create_window(
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
    except Exception:
        logging.exception("Échec du démarrage de SEL Facturation")
        if os.name == "nt":
            ctypes.windll.user32.MessageBoxW(
                None,
                "Le démarrage a échoué. Consultez application.log dans le dossier de données SEL.\n"
                "Vérifiez que Microsoft Edge WebView2 Runtime est installé.",
                "SEL Facturation",
                0x10,
            )
        raise
    finally:
        if server is not None:
            server.shutdown()
            server.server_close()
        if os.name == "nt" and mutex not in (None, False):
            ctypes.windll.kernel32.CloseHandle(mutex)


if __name__ == "__main__":
    main()
