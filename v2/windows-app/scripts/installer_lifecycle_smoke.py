"""Run the NSIS install/upgrade path against an isolated GitHub Actions profile."""

from __future__ import annotations

import hashlib
import os
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

    # Tauri's app_local_data_dir() resolves under LOCALAPPDATA on Windows.
    # The central-only client stores no business database, device identity or outbox.
    appdata = Path(os.environ["LOCALAPPDATA"]) / APP_IDENTIFIER
    install_dir = Path(os.environ["RUNNER_TEMP"]) / f"sel-v2-upgrade-{uuid.uuid4()}" / "app"
    if appdata.exists() or install_dir.exists():
        raise RuntimeError("Isolated smoke-test paths already exist; refusing to overwrite them")

    appdata.mkdir(parents=True)
    install_dir.parent.mkdir(parents=True)
    # Opaque synthetic marker only: installation must preserve the user's
    # protected session file but must never create local business data.
    (appdata / "central-session.dpapi").write_bytes(b"synthetic protected-session marker")
    (appdata / "window-preferences.marker").write_text("synthetic preference", encoding="utf-8")
    expected = tree_hashes(appdata)

    run_setup(previous_setup, install_dir, "initial install")
    if tree_hashes(appdata) != expected:
        raise RuntimeError("Previous-version install modified the synthetic user profile")

    run_setup(current_setup, install_dir, "upgrade")
    actual = tree_hashes(appdata)
    if actual != expected:
        raise RuntimeError("Upgrade modified the synthetic central-session or preference marker")

    run_setup(current_setup, install_dir, "repair/reinstall")
    if tree_hashes(appdata) != expected:
        raise RuntimeError("Repair/reinstall modified the synthetic user profile")

    if any(path.suffix.lower() in {".sqlite", ".sqlite3", ".db"} for path in appdata.rglob("*")):
        raise RuntimeError("A local business database was created by the installer")

    print(
        "PASS: current-user NSIS initial install, version upgrade, and repair/reinstall "
        "preserved the synthetic central-session marker and preferences without a local business database."
    )


if __name__ == "__main__":
    main()
