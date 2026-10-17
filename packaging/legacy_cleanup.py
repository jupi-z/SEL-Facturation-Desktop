"""Safely migrate the legacy Inno Setup installation before MSI installation.

This helper is intentionally narrow: it only knows the historical SEL
uninstall key and only executes an uninstaller located inside the registered
legacy installation directory.  It never removes the application's data
directory.
"""

from __future__ import annotations

import argparse
import ctypes
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Any


LEGACY_UNINSTALL_KEY = (
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"
    "{7B258C72-AE0B-4D4E-84DE-725C08F20EA1}_is1"
)
DATA_DIRECTORY_NAME = "SEL Facturation"
VERSION_PATTERN = re.compile(
    r"^\s*v?(?P<major>\d+)(?:\.(?P<minor>\d+))?"
    r"(?:\.(?P<patch>\d+))?(?:[-+.]?(?P<suffix>[0-9A-Za-z.-]+))?\s*$"
)


def _version_key(value: str) -> tuple[tuple[int, int, int], tuple[tuple[int, str], ...]]:
    match = VERSION_PATTERN.fullmatch(str(value))
    if not match:
        raise ValueError(f"Version non reconnue: {value!r}")

    core = tuple(int(match.group(name) or 0) for name in ("major", "minor", "patch"))
    suffix = match.group("suffix")
    if not suffix:
        return core, ((1, ""),)

    tokens: list[tuple[int, str]] = []
    for token in suffix.split("."):
        if token.isdigit():
            tokens.append((0, f"{int(token):012d}"))
        else:
            tokens.append((0, token.lower()))
    return core, ((0, ""), *tokens)


def compare_versions(left: str, right: str) -> int:
    """Compare two installer versions using numeric semantic components."""

    left_key = _version_key(left)
    right_key = _version_key(right)
    return (left_key > right_key) - (left_key < right_key)


def choose_action(legacy_version: str | None, current_version: str) -> str:
    """Return ``none``, ``cleanup`` or ``block`` for the installer matrix."""

    if legacy_version is None:
        return "none"
    try:
        return "cleanup" if compare_versions(legacy_version, current_version) < 0 else "block"
    except ValueError:
        return "block"


def is_safe_uninstaller(uninstaller: Path, install_root: Path) -> bool:
    """Ensure only an Inno uninstaller below the registered install root runs."""

    try:
        uninstaller_path = uninstaller.resolve(strict=False)
        root_path = install_root.resolve(strict=False)
        inside_root = uninstaller_path.is_relative_to(root_path)
    except (OSError, ValueError):
        return False
    return inside_root and uninstaller_path.suffix.lower() == ".exe" and uninstaller_path.name.lower().startswith("unins")


def _read_legacy_installation() -> dict[str, Any] | None:
    if os.name != "nt":
        return None

    import winreg

    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, LEGACY_UNINSTALL_KEY)
    except FileNotFoundError:
        return None

    with key:
        values: dict[str, Any] = {}
        for name in ("DisplayVersion", "InstallLocation", "UninstallString"):
            try:
                values[name] = winreg.QueryValueEx(key, name)[0]
            except FileNotFoundError:
                values[name] = None
        return values


def _extract_executable(command: str | None) -> Path | None:
    if not command:
        return None
    text = os.path.expandvars(str(command).strip())
    if not text:
        return None
    if text.startswith('"'):
        closing_quote = text.find('"', 1)
        if closing_quote <= 1:
            return None
        return Path(text[1:closing_quote])
    return Path(text.split(None, 1)[0])


def _data_directory() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        raise RuntimeError("LOCALAPPDATA est indisponible; migration annulée.")
    return Path(local_app_data) / DATA_DIRECTORY_NAME


def _notify(message: str) -> None:
    """Show a native diagnostic because the bundled helper has no console."""

    if os.name != "nt":
        return
    try:
        ctypes.windll.user32.MessageBoxW(0, message, "SEL Facturation", 0x10)
    except (AttributeError, OSError):
        return


def migrate_legacy(current_version: str) -> int:
    """Migrate a lower legacy version, or return a safe failure code."""

    installation = _read_legacy_installation()
    if installation is None:
        print("Aucune installation héritée détectée.")
        return 0

    legacy_version = installation.get("DisplayVersion")
    action = choose_action(legacy_version, current_version)
    if action == "block":
        message = (
            f"Une version héritée de SEL Facturation ({legacy_version or 'inconnue'}) "
            f"est déjà installée. La version {current_version} ne sera pas installée "
            "pour éviter un doublon ou une perte de données."
        )
        print(message)
        _notify(message)
        return 20

    install_location = installation.get("InstallLocation")
    uninstaller = _extract_executable(installation.get("UninstallString"))
    if not install_location or uninstaller is None:
        message = "Migration bloquée : les informations de désinstallation héritées sont incomplètes."
        print(message)
        _notify(message)
        return 21

    install_root = Path(os.path.expandvars(str(install_location))).resolve(strict=False)
    uninstaller = uninstaller.expanduser().resolve(strict=False)
    if not is_safe_uninstaller(uninstaller, install_root) or not uninstaller.is_file():
        message = "Migration bloquée : le désinstalleur hérité est introuvable ou non sûr."
        print(message)
        _notify(message)
        return 22

    data_directory = _data_directory()
    data_existed = data_directory.exists()
    command = [str(uninstaller), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]
    try:
        result = subprocess.run(command, check=False, timeout=180, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    except (OSError, subprocess.TimeoutExpired) as exc:
        message = f"Migration interrompue : désinstallation héritée impossible ({exc})."
        print(message)
        _notify(message)
        return 23

    if result.returncode not in (0, 3010):
        message = f"Migration interrompue : l'ancien désinstalleur a renvoyé {result.returncode}."
        print(message)
        _notify(message)
        return 24
    if _read_legacy_installation() is not None:
        message = "Migration interrompue : l'installation héritée est toujours enregistrée."
        print(message)
        _notify(message)
        return 25
    if data_existed and not data_directory.exists():
        message = "Migration interrompue : le dossier de données n'est plus présent."
        print(message)
        _notify(message)
        return 26

    print(f"Installation héritée {legacy_version} retirée; données conservées.")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Migration SEL Facturation héritée")
    parser.add_argument("--cleanup-legacy", action="store_true")
    parser.add_argument("--current-version", required=True)
    arguments = parser.parse_args(argv)
    if not arguments.cleanup_legacy:
        parser.error("--cleanup-legacy est requis")
    return migrate_legacy(arguments.current_version)


if __name__ == "__main__":
    sys.exit(main())
