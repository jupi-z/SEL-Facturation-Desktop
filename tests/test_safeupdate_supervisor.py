from pathlib import Path

import safeupdate_supervisor as supervisor


def test_supervisor_revalidates_transaction_and_uses_fixed_installer_args(monkeypatch, tmp_path):
    transaction_id = "c" * 32
    backup = tmp_path / "backup.sqlite3"
    backup.write_bytes(b"snapshot")
    package = tmp_path / "SEL-Facturation-Setup-0.4.0.exe"
    package.write_bytes(b"package")
    calls = []
    monkeypatch.setattr(supervisor, "wait_for_parent", lambda pid: True)
    monkeypatch.setattr(supervisor, "verify_transaction", lambda *args, **kwargs: {"package_path": package, "backup_path": str(backup)})
    monkeypatch.setattr(supervisor, "mark_transaction", lambda *args, **kwargs: {})
    def runner(args, **kwargs):
        calls.append((args, kwargs))
        return type("Result", (), {"returncode": 0})()
    assert supervisor.run(transaction_id, 123, staging_root=tmp_path, installer_runner=runner, relauncher=lambda *args, **kwargs: None) == 0
    assert calls == [([str(package), "/norestart"], {"shell": False, "check": False})]
