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
        return f"Vérification terminée : {installed}. L'installation de maintenance vers la version {version} va démarrer."
    return f"Installation refusée : {installed} est plus récente ou incompatible avec la version {version}."


def _notify(message: str, error: bool = False) -> None:
    if os.name != "nt":
        return
    try:
        ctypes.windll.user32.MessageBoxW(0, message, "SEL Facturation", 0x10 if error else 0x40)
    except (AttributeError, OSError):
        return


def resolve_core_path(base: Path) -> Path | None:
    """Resolve the embedded Burn engine even when its build name is versioned."""

    exact = base / "SEL-Facturation-Setup-core.exe"
    if exact.is_file():
        return exact
    versioned = sorted(base.glob("SEL-Facturation-Setup-*-core.exe"))
    return versioned[0] if versioned else None


def resolve_repair_path(installations: list[dict]) -> Path | None:
    """Use the exact cached Burn bundle for same-version maintenance."""

    for installation in installations:
        if installation.get("kind") != "burn":
            continue
        candidate = installation.get("bundle_path")
        if not candidate:
            continue
        path = Path(candidate)
        if path.is_file() and path.suffix.lower() == ".exe":
            return path
    return None


def _has_action_argument(arguments: list[str]) -> bool:
    actions = {"/install", "/modify", "/repair", "/uninstall", "/layout", "/help", "/?"}
    return any(argument.lower().split("=", 1)[0] in actions for argument in arguments)


def _core_path() -> Path | None:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
    return resolve_core_path(base)


def main(argv: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    core = _core_path()
    if core is None:
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
    launch_core = core
    launch_arguments = list(arguments)
    if action == "repair" and not _has_action_argument(launch_arguments):
        launch_core = resolve_repair_path(installations)
        if launch_core is None:
            _notify(
                "La version SEL est déjà installée, mais son moteur de maintenance est introuvable. "
                "Aucune nouvelle installation ne sera créée pour protéger les données.",
                error=True,
            )
            return 31
        launch_arguments.append("/repair")
    return subprocess.run([str(launch_core), *launch_arguments], check=False).returncode


if __name__ == "__main__":
    sys.exit(main())
