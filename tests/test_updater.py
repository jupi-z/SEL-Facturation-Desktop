import hashlib
import json
import sqlite3

import pytest

import db
from updater import ManifestError, UpdateManifest, check_for_update, download_update, verify_sha256


def manifest_payload(**overrides):
    payload = {
        "schema": 1,
        "version": "0.3.2",
        "url": "https://example.test/SEL-Facturation-Setup-0.3.2.exe",
        "sha256": "a" * 64,
        "size": 4,
        "published_at": "2026-10-15T12:00:00Z",
        "release_notes": "Correctifs de fiabilité.",
    }
    payload.update(overrides)
    return json.dumps(payload).encode("utf-8")


def test_manifest_accepts_valid_https_release():
    manifest = UpdateManifest.from_payload(manifest_payload(), "https://example.test/update.json")

    assert manifest.version == "0.3.2"
    assert manifest.is_newer_than("0.3.1")
    assert not manifest.is_newer_than("0.3.2")
    assert not manifest.is_newer_than("0.4.0")


@pytest.mark.parametrize(
    "overrides",
    [
        {"url": "http://example.test/update.exe"},
        {"url": "https://example.test/update.msi"},
        {"sha256": "not-a-hash"},
        {"size": 0},
        {"version": "latest"},
    ],
)
def test_manifest_rejects_unsafe_or_invalid_values(overrides):
    with pytest.raises(ManifestError):
        UpdateManifest.from_payload(manifest_payload(**overrides), "https://example.test/update.json")


def test_manifest_source_must_be_https():
    with pytest.raises(ManifestError):
        UpdateManifest.from_payload(manifest_payload(), "http://example.test/update.json")


def test_check_for_update_returns_none_for_current_or_older_release(monkeypatch):
    class Response:
        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def read(self, size=-1):
            return manifest_payload(version="0.3.1")

        def geturl(self):
            return "https://example.test/update.json"

    monkeypatch.setattr("updater.urlopen", lambda request, timeout: Response())

    result = check_for_update("https://example.test/update.json", "0.3.1")

    assert result.manifest is None
    assert result.status == "current"


def test_download_update_verifies_hash_and_renames_atomically(tmp_path, monkeypatch):
    content = b"test-installer"
    digest = hashlib.sha256(content).hexdigest()
    manifest = UpdateManifest.from_payload(
        manifest_payload(sha256=digest, size=len(content)),
        "https://example.test/update.json",
    )

    class Response:
        def __init__(self):
            self.remaining = content

        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def read(self, size=-1):
            chunk, self.remaining = self.remaining[:size], self.remaining[size:]
            return chunk

        def geturl(self):
            return "https://example.test/SEL-Facturation-Setup-0.3.2.exe"

    monkeypatch.setattr("updater.urlopen", lambda request, timeout: Response())

    target = download_update(manifest, tmp_path)

    assert target.name == "SEL-Facturation-Setup-0.3.2.exe"
    assert target.read_bytes() == content
    assert not list(tmp_path.glob("*.part"))
    verify_sha256(target, digest)


def test_download_update_removes_partial_file_when_hash_is_wrong(tmp_path, monkeypatch):
    content = b"unexpected"
    manifest = UpdateManifest.from_payload(
        manifest_payload(sha256="b" * 64, size=len(content)),
        "https://example.test/update.json",
    )

    class Response:
        def __init__(self):
            self.remaining = content

        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def read(self, size=-1):
            chunk, self.remaining = self.remaining[:size], self.remaining[size:]
            return chunk

        def geturl(self):
            return "https://example.test/SEL-Facturation-Setup-0.3.2.exe"

    monkeypatch.setattr("updater.urlopen", lambda request, timeout: Response())

    with pytest.raises(ManifestError):
        download_update(manifest, tmp_path)

    assert not list(tmp_path.iterdir())


def test_create_pre_update_backup_is_valid_and_keeps_live_database(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    live = db.db_path().read_bytes()

    from updater import create_pre_update_backup

    backup = create_pre_update_backup()

    assert backup.parent == tmp_path / "sauvegardes"
    assert backup.exists()
    check_connection = sqlite3.connect(backup)
    try:
        assert check_connection.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    finally:
        check_connection.close()
    assert db.db_path().read_bytes() == live
