import pytest

from update_policy import evaluate_manifest, verify_sha256


def manifest(**overrides):
    value = {
        "version": "0.4.0",
        "product": "SEL-Facturation",
        "url": "https://sel.shuleops.com/releases/SEL-Facturation-Setup-0.4.0.exe",
        "sha256": "a" * 64,
        "package_type": "bootstrapper",
    }
    value.update(overrides)
    return value


def test_accepts_higher_https_bootstrapper():
    result = evaluate_manifest(manifest(), current_version="0.3.0")

    assert result == {
        "state": "available",
        "version": "0.4.0",
        "url": manifest()["url"],
        "sha256": "a" * 64,
        "product": "SEL-Facturation",
    }


@pytest.mark.parametrize(
    "changes,reason",
    [
        ({"url": "http://example.test/setup.exe"}, "https_required"),
        ({"sha256": "bad"}, "invalid_sha256"),
        ({"package_type": "exe"}, "unsupported_package"),
        ({"product": "other-app"}, "unsupported_product"),
        ({"version": "0.2.0"}, "downgrade_blocked"),
        ({"version": "0.3.0"}, "already_current"),
    ],
)
def test_rejects_unsafe_or_non_upgrades(changes, reason):
    result = evaluate_manifest(manifest(**changes), current_version="0.3.0")

    expected_state = "rejected" if reason in {"https_required", "invalid_sha256", "unsupported_package", "unsupported_product", "downgrade_blocked"} else "current"
    assert result["state"] == expected_state
    assert result["reason"] == reason


def test_semantic_prerelease_order_is_not_lexicographic():
    result = evaluate_manifest(manifest(version="1.0.0-alpha.10"), current_version="1.0.0-alpha.2")

    assert result["state"] == "available"


def test_preserves_signed_channel_metadata_for_the_update_ui():
    result = evaluate_manifest(
        manifest(version="0.4.0", channel="preview", signed=False),
        current_version="0.3.0",
    )

    assert result["channel"] == "preview"
    assert result["signed"] is False


def test_verifies_downloaded_package_bytes(tmp_path):
    package = tmp_path / "setup.exe"
    package.write_bytes(b"SEL package")
    import hashlib
    digest = hashlib.sha256(b"SEL package").hexdigest()

    assert verify_sha256(package, digest)
    assert not verify_sha256(package, "0" * 64)
