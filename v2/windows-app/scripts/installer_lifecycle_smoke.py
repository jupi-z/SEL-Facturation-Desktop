"""Run the NSIS install/upgrade path against an isolated GitHub Actions profile."""

from __future__ import annotations

import hashlib
import os
import sqlite3
import subprocess
import sys
import uuid
from pathlib import Path


APP_IDENTIFIER = "cd.sel.facturation.desktop"


def tree_hashes(root: Path) -> dict[str, str]:
    return {
        str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def run_setup(setup: Path, install_dir: Path, operation: str) -> None:
    result = subprocess.run(
        [str(setup), "/S", f"/D={install_dir}"],
        check=False,
        timeout=300,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"NSIS {operation} using {setup.name} returned {result.returncode}"
        )
    installed_executables = list(install_dir.rglob("*.exe"))
    if not installed_executables:
        raise RuntimeError(f"NSIS installer produced no executable under {install_dir}")


def main() -> None:
    if os.name != "nt":
        raise RuntimeError("This smoke test must run on a Windows CI runner")
    if len(sys.argv) != 3:
        raise SystemExit("usage: installer_lifecycle_smoke.py <previous-setup.exe> <current-setup.exe>")

    previous_setup, current_setup = (Path(arg).resolve() for arg in sys.argv[1:])
    for setup in (previous_setup, current_setup):
        if not setup.is_file():
            raise FileNotFoundError(setup)
    if previous_setup == current_setup:
        raise RuntimeError("The previous and current installers must be distinct files")

    # Tauri's app_local_data_dir() resolves under LOCALAPPDATA on Windows; this
    # is the exact root used by the desktop shell for SQLite and device config.
    appdata = Path(os.environ["LOCALAPPDATA"]) / APP_IDENTIFIER
    install_dir = Path(os.environ["RUNNER_TEMP"]) / f"sel-v2-upgrade-{uuid.uuid4()}" / "app"
    if appdata.exists() or install_dir.exists():
        raise RuntimeError("Isolated smoke-test paths already exist; refusing to overwrite them")

    appdata.mkdir(parents=True)
    install_dir.parent.mkdir(parents=True)
    database = appdata / "sel-local-v2.sqlite"
    with sqlite3.connect(database) as connection:
        connection.execute(
            "CREATE TABLE continuity_probe (record_key TEXT PRIMARY KEY, value TEXT NOT NULL)"
        )
        connection.executemany(
            "INSERT INTO continuity_probe(record_key, value) VALUES (?, ?)",
            [
                ("identity", "synthetic-device-identity"),
                ("outbox", "synthetic-pending-event"),
                ("invoice", "synthetic-historical-snapshot"),
            ],
        )
    (appdata / "device.json").write_text(
        '{"organization_id":"synthetic","device_id":"synthetic"}',
        encoding="utf-8",
    )
    documents = appdata / "documents"
    documents.mkdir()
    (documents / "synthetic-historical-document.marker").write_text(
        "synthetic local document marker", encoding="utf-8"
    )
    expected = tree_hashes(appdata)

    run_setup(previous_setup, install_dir, "initial install")
    if tree_hashes(appdata) != expected:
        raise RuntimeError("Previous-version install modified the synthetic user profile")

    run_setup(current_setup, install_dir, "upgrade")
    actual = tree_hashes(appdata)
    if actual != expected:
        raise RuntimeError("Upgrade modified the synthetic database, identity, outbox, or document marker")

    run_setup(current_setup, install_dir, "repair/reinstall")
    if tree_hashes(appdata) != expected:
        raise RuntimeError("Repair/reinstall modified the synthetic user profile")

    with sqlite3.connect(database) as connection:
        rows = dict(connection.execute("SELECT record_key, value FROM continuity_probe"))
    if rows.get("outbox") != "synthetic-pending-event":
        raise RuntimeError("Synthetic pending outbox record did not survive the upgrade")

    print(
        "PASS: current-user NSIS initial install, version upgrade, and repair/reinstall "
        "preserved synthetic SQLite, device configuration, outbox marker, and local document marker."
    )


if __name__ == "__main__":
    main()
