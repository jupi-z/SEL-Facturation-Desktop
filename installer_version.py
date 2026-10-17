"""Version and registry discovery shared by the installer preflight tools."""

from __future__ import annotations

import os
from pathlib import Path
import re
from typing import Any


LEGACY_UNINSTALL_KEY = (
    "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"
    "{7B258C72-AE0B-4D4E-84DE-725C08F20EA1}_is1"
)
BUNDLE_UPGRADE_CODE = "{C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF}"
SEL_DISPLAY_NAME = "SEL Facturation"
SEL_PUBLISHER = "Société d'Énergie du Lualaba"
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
            installations.append({"kind": "burn", "version": values.get("BundleVersion") or values.get("DisplayVersion"), "key": key_name})
            continue
        if (
            values.get("DisplayName") == SEL_DISPLAY_NAME
            and values.get("Publisher") == SEL_PUBLISHER
            and "msiexec" in str(values.get("UninstallString") or "").lower()
        ):
            installations.append({"kind": "msi", "version": values.get("DisplayVersion"), "key": key_name})
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
    if any(comparison == 0 for _, comparison in comparisons):
        return "repair"
    return "upgrade"
