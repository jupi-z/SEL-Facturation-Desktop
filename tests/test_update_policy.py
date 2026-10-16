import pytest

from update_policy import evaluate_manifest


def manifest(**overrides):
    value = {
        "version": "0.4.0",
        "url": "https://sel.shuleops.com/releases/SEL-Facturation-Setup-0.4.0.exe",
        "sha256": "a" * 64,
        "package_type": "bootstrapper",
    }
    value.update(overrides)
    return value


def test_accepts_higher_https_bootstrapper():
    result = evaluate_manifest(manifest(), current_version="0.3.0")

    assert result == {"state": "available", "version": "0.4.0", "url": manifest()["url"]}


@pytest.mark.parametrize(
    "changes,reason",
    [
        ({"url": "http://example.test/setup.exe"}, "https_required"),
        ({"sha256": "bad"}, "invalid_sha256"),
        ({"package_type": "exe"}, "unsupported_package"),
        ({"version": "0.2.0"}, "downgrade_blocked"),
        ({"version": "0.3.0"}, "already_current"),
    ],
)
def test_rejects_unsafe_or_non_upgrades(changes, reason):
    result = evaluate_manifest(manifest(**changes), current_version="0.3.0")

    assert result["state"] == "rejected" if reason in {"https_required", "invalid_sha256", "unsupported_package", "downgrade_blocked"} else result["state"] == "current"
    assert result["reason"] == reason
