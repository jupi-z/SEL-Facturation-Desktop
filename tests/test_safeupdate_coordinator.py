import time
from pathlib import Path
from threading import Event
from urllib.error import HTTPError

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
    coordinator._set(state="downloading", phase="downloading", message="Téléchargement en cours.")
    assert not coordinator.start_check()
    assert coordinator.snapshot()["state"] == "downloading"
    release.set()
    assert wait_for(coordinator, "current")["state"] == "current"
    assert calls == [True]


def test_current_state_confirms_installed_version_is_latest_available():
    coordinator = SafeUpdateCoordinator(
        manifest_url="https://updates.example.test/latest.json", current_version="0.3.27",
        fetcher=lambda *args, **kwargs: {"state": "current", "version": "0.3.27"},
    )

    assert coordinator.start_check()
    current = wait_for(coordinator, "current")
    assert current["message"] == "Vous utilisez déjà la dernière version disponible (0.3.27)."


def test_missing_update_channel_is_reported_without_claiming_current():
    def missing_channel(*args, **kwargs):
        raise HTTPError("https://updates.example.test/latest.sig.json", 404, "Not Found", None, None)

    coordinator = SafeUpdateCoordinator(
        manifest_url="https://updates.example.test/latest.json", current_version="0.3.27",
        fetcher=missing_channel,
    )

    assert coordinator.start_check()
    result = wait_for(coordinator, "unavailable")
    assert result["message"] == "Le canal de mise à jour n’est pas encore configuré. SEL Facturation reste disponible."


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
