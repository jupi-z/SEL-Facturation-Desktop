"""Same-user SafeUpdate supervisor.

It accepts only an opaque transaction id, waits for the parent application,
revalidates the transaction, and invokes the fixed WiX bootstrapper argument
list without a shell or elevation helper.
"""
from __future__ import annotations

import logging
import os
from pathlib import Path
import subprocess
import sys
import time

from installer_version import canonical_installation_executable
from safeupdate_keys import TRUSTED_UPDATE_KEYS
from update_client import staging_dir
from update_transaction import mark_transaction, release_transaction_lock, verify_transaction
from version import APP_VERSION


LOGGER = logging.getLogger(__name__)


def _pid_exists(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name == "nt":
        import ctypes
        SYNCHRONIZE = 0x00100000
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        handle = kernel32.OpenProcess(SYNCHRONIZE, False, pid)
        if not handle:
            return False
        try:
            return kernel32.WaitForSingleObject(handle, 0) == 0x00000102
        finally:
            kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def wait_for_parent(pid: int, timeout: float = 120.0) -> bool:
    deadline = time.monotonic() + timeout
    while _pid_exists(pid) and time.monotonic() < deadline:
        time.sleep(0.25)
    return not _pid_exists(pid)


def _supervisor_command(transaction_id: str, parent_pid: int) -> list[str]:
    if getattr(sys, "frozen", False):
        return [sys.executable, "--sel-update-supervisor", transaction_id, str(parent_pid)]
    return [sys.executable, str(Path(__file__).resolve()), "--sel-update-supervisor", transaction_id, str(parent_pid)]


def spawn(transaction_id: str, parent_pid: int) -> None:
    """Spawn a detached same-user supervisor with no user-controlled command."""
    subprocess.Popen(
        _supervisor_command(transaction_id, parent_pid),
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        close_fds=True, shell=False,
        creationflags=getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )


def run(transaction_id: str, parent_pid: int, *, staging_root: Path | None = None,
        installer_runner=subprocess.run, relauncher=subprocess.Popen) -> int:
    root = Path(staging_root) if staging_root is not None else staging_dir()
    try:
        if not isinstance(parent_pid, int) or parent_pid <= 0:
            raise ValueError("PID parent invalide")
        if not wait_for_parent(parent_pid):
            raise RuntimeError("Le logiciel SEL ne s’est pas fermé à temps")
        verified = verify_transaction(
            transaction_id, staging_root=root, current_version=APP_VERSION,
            trusted_keys=TRUSTED_UPDATE_KEYS,
        )
        record = verified
        backup_path = record.get("backup_path")
        if backup_path and not Path(backup_path).is_file():
            raise RuntimeError("La sauvegarde vérifiée est introuvable")
        mark_transaction(transaction_id, "installing", staging_root=root)
        result = installer_runner(
            [str(verified["package_path"]), "/norestart"],
            shell=False, check=False,
        )
        code = int(result.returncode)
        if code != 0:
            mark_transaction(transaction_id, "recovery_required", staging_root=root, exit_code=code)
            return code
        mark_transaction(transaction_id, "completed", staging_root=root, exit_code=code)
        executable = canonical_installation_executable()
        if executable.is_file():
            relauncher([str(executable)], shell=False, close_fds=True)
        return 0
    except Exception:
        LOGGER.exception("SafeUpdate supervisor failed")
        try:
            mark_transaction(transaction_id, "recovery_required", staging_root=root)
        except Exception:
            LOGGER.exception("Unable to persist SafeUpdate recovery state")
        return 1
    finally:
        try:
            release_transaction_lock(transaction_id, staging_root=root)
        except Exception:
            pass


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) != 3 or args[0] != "--sel-update-supervisor":
        return 2
    transaction_id = args[1]
    try:
        parent_pid = int(args[2])
    except ValueError:
        return 2
    return run(transaction_id, parent_pid)
