import time
from pathlib import Path
from threading import Event

from safeupdate_coordinator import SafeUpdateCoordinator


def wait_for(coordinator, state):
    for _ in range(100):
        if coordinator.snapshot()["state"] == state:
            return coordinator.snapshot()
        time.sleep(0.005)
    return coordinator.snapshot()


def test_check_is_nonblocking_and_serialized():
    calls = []
    release = Event()
    def fetch(*args, **kwargs):
        calls.append(True)
        release.wait(1)
        return {"state": "current", "version": "0.3.24"}
    coordinator = SafeUpdateCoordinator(
        manifest_url="https://updates.example.test/latest.json", current_version="0.3.24",
        fetcher=fetch,
    )
    assert coordinator.start_check()
    assert not coordinator.start_check()
    release.set()
    assert wait_for(coordinator, "current")["state"] == "current"
    assert calls == [True]


def test_download_reports_real_progress_and_requires_backup(monkeypatch, tmp_path):
    coordinator = SafeUpdateCoordinator(
        manifest_url="https://updates.example.test/latest.json", current_version="0.3.24",
        fetcher=lambda *args, **kwargs: {"state": "available", "version": "0.4.0", "manifest_trusted": True, "manifest_bytes": b"m", "signature_envelope_bytes": b"s", "size_bytes": 4},
        downloader=lambda update, root, **kwargs: kwargs["progress"](4, 4) or {"transaction_id": "a" * 32, "filename": "SEL-Facturation-Setup-0.4.0.exe", "version": "0.4.0", "sha256": "a" * 64, "size_bytes": 4},
        backupper=lambda: tmp_path / "backup.sqlite3",
        staging_root=tmp_path,
    )
    (tmp_path / "backup.sqlite3").write_bytes(b"snapshot")
    monkeypatch.setattr("safeupdate_coordinator.verify_transaction", lambda *args, **kwargs: {"version": "0.4.0"})
    monkeypatch.setattr("safeupdate_coordinator.mark_transaction", lambda *args, **kwargs: {})
    assert coordinator.start_check()
    assert wait_for(coordinator, "available")["state"] == "available"
    assert coordinator.start_download()
    ready = wait_for(coordinator, "ready")
    assert ready["received_bytes"] == 4
    assert ready["backup_name"] == "backup.sqlite3"


def test_backup_failure_blocks_installation(monkeypatch, tmp_path):
    coordinator = SafeUpdateCoordinator(
        manifest_url="https://updates.example.test/latest.json", current_version="0.3.24",
        fetcher=lambda *args, **kwargs: {"state": "available", "version": "0.4.0", "manifest_trusted": True, "manifest_bytes": b"m", "signature_envelope_bytes": b"s"},
        downloader=lambda *args, **kwargs: {"transaction_id": "b" * 32, "filename": "SEL-Facturation-Setup-0.4.0.exe", "version": "0.4.0", "sha256": "a" * 64, "size_bytes": 4},
        backupper=lambda: (_ for _ in ()).throw(OSError("disk full")), staging_root=tmp_path,
    )
    monkeypatch.setattr("safeupdate_coordinator.verify_transaction", lambda *args, **kwargs: {"version": "0.4.0"})
    monkeypatch.setattr("safeupdate_coordinator.db.db_path", lambda: tmp_path / "sel.sqlite3")
    (tmp_path / "sel.sqlite3").write_bytes(b"db")
    coordinator.start_check()
    wait_for(coordinator, "available")
    coordinator.start_download()
    assert wait_for(coordinator, "recovery_required")["state"] == "recovery_required"
