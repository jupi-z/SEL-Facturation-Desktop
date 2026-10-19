import hashlib
import io
import json

import pytest

from update_client import download_package, fetch_manifest, verify_authenticode


class FakeResponse:
    def __init__(self, payload, url="https://sel.shuleops.com/updates/latest.json"):
        self.payload = payload
        self._url = url

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self, size=-1):
        return self.payload.read(size)

    def geturl(self):
        return self._url


def test_fetch_manifest_rejects_http_before_network():
    with pytest.raises(ValueError, match="https"):
        fetch_manifest("http://example.test/manifest.json", current_version="0.3.0", opener=lambda *_: pytest.fail("network called"))


def test_fetch_manifest_parses_json_and_validates_policy():
    manifest = {"product": "SEL-Facturation", "version": "0.4.0", "url": "https://sel.shuleops.com/setup.exe", "sha256": "a" * 64, "package_type": "bootstrapper"}
    response = FakeResponse(io.BytesIO(json.dumps(manifest).encode()))

    result = fetch_manifest("https://sel.shuleops.com/updates/latest.json", current_version="0.3.0", opener=lambda *_args, **_kwargs: response)

    assert result["state"] == "available"
    assert result["sha256"] == "a" * 64


def test_download_package_verifies_hash_before_publish(tmp_path):
    payload = b"trusted setup bytes"
    manifest = {"product": "SEL-Facturation", "version": "0.4.0", "url": "https://sel.shuleops.com/setup.exe", "sha256": hashlib.sha256(payload).hexdigest(), "package_type": "bootstrapper"}
    response = FakeResponse(io.BytesIO(payload), "https://sel.shuleops.com/setup.exe")

    result = download_package(
        manifest, tmp_path, opener=lambda *_args, **_kwargs: response,
        current_version="0.3.0", expected_publisher="Test Publisher",
        signature_verifier=lambda *_args, **_kwargs: True,
    )

    assert result.read_bytes() == payload
    assert result.parent == tmp_path


def test_download_requires_publisher_signature(tmp_path):
    manifest = {"product": "SEL-Facturation", "version": "0.4.0", "url": "https://sel.shuleops.com/setup.exe", "sha256": "a" * 64, "package_type": "bootstrapper"}

    with pytest.raises(ValueError, match="signature"):
        download_package(manifest, tmp_path, current_version="0.3.0", opener=lambda *_args, **_kwargs: pytest.fail("network called"))


def test_download_rejects_an_explicitly_unsigned_preview_before_network(tmp_path):
    manifest = {
        "product": "SEL-Facturation",
        "version": "0.4.0",
        "url": "https://sel.shuleops.com/setup.exe",
        "sha256": "a" * 64,
        "package_type": "bootstrapper",
        "channel": "preview",
        "signed": False,
    }

    with pytest.raises(ValueError, match="non signé"):
        download_package(
            manifest,
            tmp_path,
            current_version="0.3.0",
            expected_publisher="SEL",
            opener=lambda *_args, **_kwargs: pytest.fail("network called"),
        )


def test_authenticode_publisher_match_is_not_a_substring(monkeypatch, tmp_path):
    package = tmp_path / "setup.exe"
    package.write_bytes(b"setup")

    class Result:
        returncode = 0
        stdout = "CN=MonarQue Group S.A.S. (unrelated publisher)"

    monkeypatch.setattr("update_client.subprocess.run", lambda *args, **kwargs: Result())

    assert not verify_authenticode(package, expected_subject="MonarQue Group S.A.S.")
