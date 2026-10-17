"""Native first-run preflight that routes the user to the Burn bootstrapper."""

from __future__ import annotations

import ctypes
import os
from pathlib import Path
import re
import subprocess
import sys

from installer_version import choose_preflight_action, discover_installations


def _version_file() -> Path:
    if getattr(sys, "frozen", False):
        return Path(getattr(sys, "_MEIPASS")) / "version.py"
    return Path(__file__).resolve().parents[1] / "version.py"


def current_version() -> str:
    text = _version_file().read_text(encoding="utf-8")
    match = re.search(r"APP_VERSION\s*=\s*[\"']([^\"']+)[\"']", text)
    if not match:
        raise RuntimeError("APP_VERSION introuvable dans le paquet.")
    return match.group(1)


def status_message(action: str, installations: list[dict], version: str) -> str:
    installed = ", ".join(
        f"{item['kind']} {item.get('version') or 'inconnue'}" for item in installations
    ) or "aucune installation"
    if action == "install":
        return f"Vérification terminée : {installed}. L'installation SEL Facturation {version} va démarrer."
    if action == "migrate":
        return f"Vérification terminée : {installed}. L'ancienne installation sera remplacée par la version {version}; les données seront conservées."
    if action == "repair":
        return f"Vérification terminée : {installed}. La version {version} est déjà présente; le mode maintenance va s'ouvrir."
    if action == "upgrade":
        return f"Vérification terminée : {installed}. La mise à niveau vers la version {version} va démarrer."
    return f"Installation refusée : {installed} est plus récente ou incompatible avec la version {version}."


def _notify(message: str, error: bool = False) -> None:
    if os.name != "nt":
        return
    try:
        ctypes.windll.user32.MessageBoxW(0, message, "SEL Facturation", 0x10 if error else 0x40)
    except (AttributeError, OSError):
        return


def _core_path() -> Path:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
    return base / "SEL-Facturation-Setup-core.exe"


def main(argv: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    core = _core_path()
    if not core.is_file():
        _notify("Le moteur d'installation SEL est introuvable.", error=True)
        return 30

    # Layout/help/uninstall are Burn operations and must not be blocked by a local preflight.
    if any(argument.lower() in {"/layout", "/uninstall", "/repair", "/help", "/?"} for argument in arguments):
        return subprocess.run([str(core), *arguments], check=False).returncode

    version = current_version()
    installations = discover_installations()
    action = choose_preflight_action(installations, version)
    message = status_message(action, installations, version)
    if action == "block":
        _notify(message, error=True)
        return 20
    if not any(argument.lower() in {"/quiet", "/passive"} for argument in arguments):
        _notify(message)
    return subprocess.run([str(core), *arguments], check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
