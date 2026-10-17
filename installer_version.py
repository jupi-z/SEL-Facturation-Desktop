"""Version and registry discovery shared by the installer preflight tools."""

from __future__ import annotations

import os
from pathlib import Path
import re
import json
import tempfile
from typing import Any


LEGACY_UNINSTALL_KEY = (
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"
    "{7B258C72-AE0B-4D4E-84DE-725C08F20EA1}_is1"
)
BUNDLE_UPGRADE_CODE = "{C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF}"
SEL_DISPLAY_NAME = "SEL Facturation"
SEL_PUBLISHER = "Société d'Énergie du Lualaba"
DATA_DIRECTORY_NAME = "SEL Facturation"
INSTALLATION_MARKER_NAME = "installation.json"
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
        tokens.append((0, f"{int(token):012d}" if token.isdigit() else token.lower()))
    return core, ((0, ""), *tokens)


def compare_versions(left: str, right: str) -> int:
    left_key = _version_key(left)
    right_key = _version_key(right)
    return (left_key > right_key) - (left_key < right_key)


def choose_action(legacy_version: str | None, current_version: str) -> str:
    if legacy_version is None:
        return "none"
    try:
        return "cleanup" if compare_versions(legacy_version, current_version) < 0 else "block"
    except ValueError:
        return "block"


def is_safe_uninstaller(uninstaller: Path, install_root: Path) -> bool:
    try:
        uninstaller_path = uninstaller.resolve(strict=False)
        root_path = install_root.resolve(strict=False)
        inside_root = uninstaller_path.is_relative_to(root_path)
    except (OSError, ValueError):
        return False
    return inside_root and uninstaller_path.suffix.lower() == ".exe" and uninstaller_path.name.lower().startswith("unins")


def _normalise_guid(value: Any) -> str:
    if isinstance(value, (list, tuple)):
        value = "".join(str(item) for item in value)
    return str(value or "").replace("{", "").replace("}", "").upper()


def _command_executable(command: Any) -> str | None:
    """Extract the executable path from a Windows uninstall command."""

    text = str(command or "").strip()
    if not text:
        return None
    if text.startswith('"'):
        end = text.find('"', 1)
        return text[1:end] if end > 1 else None
    match = re.match(r"(.+?\.exe)(?:\s|$)", text, flags=re.IGNORECASE)
    return match.group(1) if match else None


def _data_root() -> Path:
    configured = os.environ.get("SEL_DATA_DIR", "").strip()
    if configured:
        return Path(configured)
    local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
    if local_app_data:
        return Path(local_app_data) / DATA_DIRECTORY_NAME
    return Path.home() / "AppData" / "Local" / DATA_DIRECTORY_NAME


def installation_marker_path(data_root: Path | None = None) -> Path:
    return (Path(data_root) if data_root is not None else _data_root()) / INSTALLATION_MARKER_NAME


def canonical_installation_executable() -> Path:
    """Return the executable path owned by the official per-user installer."""

    local_app_data = os.environ.get("LOCALAPPDATA", "").strip()
    root = Path(local_app_data) if local_app_data else Path.home() / "AppData" / "Local"
    return root / "Programs" / DATA_DIRECTORY_NAME / "SEL-Facturation.exe"


def is_canonical_installation_executable(executable_path: Path) -> bool:
    try:
        return Path(executable_path).resolve() == canonical_installation_executable().resolve()
    except OSError:
        return False


def write_installation_marker(executable_path: Path, version: str, data_root: Path | None = None) -> Path:
    """Record a portable copy that has actually been launched.

    The marker contains no credentials or business data. It is intentionally
    limited to the executable identity needed by the preflight checker.
    """

    executable = Path(executable_path).resolve()
    if not executable.name.lower().endswith(".exe"):
        raise ValueError("L'exécutable SEL doit être un fichier .exe.")
    if not VERSION_PATTERN.fullmatch(str(version)):
        raise ValueError("Version SEL invalide.")
    destination = installation_marker_path(data_root)
    destination.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema_version": 1,
        "kind": "portable",
        "version": str(version),
        "executable_path": str(executable),
    }
    temporary_name = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w", encoding="utf-8", dir=destination.parent,
            prefix=f".{destination.name}.", suffix=".tmp", delete=False,
        ) as temporary:
            temporary_name = temporary.name
            json.dump(payload, temporary, ensure_ascii=False, separators=(",", ":"))
            temporary.flush()
            os.fsync(temporary.fileno())
        os.replace(temporary_name, destination)
    except Exception:
        if temporary_name:
            Path(temporary_name).unlink(missing_ok=True)
        raise
    return destination


def read_installation_marker(data_root: Path | None = None) -> dict[str, str] | None:
    marker = installation_marker_path(data_root)
    try:
        payload = json.loads(marker.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        return None
    if payload.get("kind") != "portable":
        return None
    version = payload.get("version")
    executable = payload.get("executable_path")
    if not isinstance(version, str) or not VERSION_PATTERN.fullmatch(version):
        return None
    if not isinstance(executable, str) or not executable.lower().endswith(".exe"):
        return None
    return {"kind": "portable", "version": version, "executable_path": executable}


def _query_values(hive: Any, key_path: str, access: int = 0) -> dict[str, Any] | None:
    if os.name != "nt":
        return None
    import winreg

    try:
        key = winreg.OpenKey(hive, key_path, 0, winreg.KEY_READ | access)
    except FileNotFoundError:
        return None

    values: dict[str, Any] = {}
    with key:
        index = 0
        while True:
            try:
                name, value, _ = winreg.EnumValue(key, index)
            except OSError:
                break
            values[name] = value
            index += 1
    return values


def _uninstall_entries() -> list[tuple[str, str, dict[str, Any]]]:
    if os.name != "nt":
        return []
    import winreg

    entries: list[tuple[str, str, dict[str, Any]]] = []
    seen: set[tuple[str, str]] = set()
    roots = (
        ("HKCU", winreg.HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ("HKLM", winreg.HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    )
    access_modes = (0, getattr(winreg, "KEY_WOW64_32KEY", 0), getattr(winreg, "KEY_WOW64_64KEY", 0))
    for root_name, hive, uninstall_root in roots:
        for access in access_modes:
            try:
                root_key = winreg.OpenKey(hive, uninstall_root, 0, winreg.KEY_READ | access)
            except FileNotFoundError:
                continue
            with root_key:
                index = 0
                while True:
                    try:
                        child = winreg.EnumKey(root_key, index)
                    except OSError:
                        break
                    index += 1
                    identity = (root_name, child)
                    if identity in seen:
                        continue
                    seen.add(identity)
                    values = _query_values(hive, f"{uninstall_root}\\{child}", access)
                    if values is not None:
                        entries.append((root_name, child, values))
    return entries


def discover_installations() -> list[dict[str, Any]]:
    """Discover only SEL-owned legacy, Burn, and MSI registrations."""

    if os.name != "nt":
        return []

    installations: list[dict[str, Any]] = []
    legacy = _query_values(__import__("winreg").HKEY_CURRENT_USER, LEGACY_UNINSTALL_KEY)
    if legacy is not None:
        installations.append({"kind": "legacy", "version": legacy.get("DisplayVersion")})

    for root_name, key_name, values in _uninstall_entries():
        if _normalise_guid(values.get("BundleUpgradeCode")) == _normalise_guid(BUNDLE_UPGRADE_CODE):
            installations.append(
                {
                    "kind": "burn",
                    "version": values.get("BundleVersion") or values.get("DisplayVersion"),
                    "key": key_name,
                    "bundle_path": _command_executable(values.get("UninstallString")),
                }
            )
            continue
        if (
            values.get("DisplayName") == SEL_DISPLAY_NAME
            and values.get("Publisher") == SEL_PUBLISHER
            and "msiexec" in str(values.get("UninstallString") or "").lower()
        ):
            installations.append({"kind": "msi", "version": values.get("DisplayVersion"), "key": key_name})
    marker = read_installation_marker()
    if marker:
        installations.append(marker)
    return installations


def choose_preflight_action(installations: list[dict[str, Any]], current_version: str) -> str:
    if not installations:
        return "install"

    comparisons: list[tuple[dict[str, Any], int]] = []
    for installation in installations:
        version = installation.get("version")
        if not version:
            return "block"
        try:
            comparisons.append((installation, compare_versions(str(version), current_version)))
        except ValueError:
            return "block"

    if any(comparison > 0 for _, comparison in comparisons):
        return "block"
    if any(installation["kind"] == "legacy" and comparison == 0 for installation, comparison in comparisons):
        return "block"
    if any(installation["kind"] == "legacy" and comparison < 0 for installation, comparison in comparisons):
        return "migrate"
    # A registered Burn bundle has a reliable cached maintenance engine. A
    # portable copy (or an MSI-only registration) does not; the current
    # bootstrapper must be allowed to install/repair it instead of failing
    # with a missing cached-bundle error or creating a second repair path.
    if any(installation["kind"] == "burn" and comparison == 0 for installation, comparison in comparisons):
        return "repair"
    if any(comparison == 0 for _, comparison in comparisons):
        return "upgrade"
    return "upgrade"
