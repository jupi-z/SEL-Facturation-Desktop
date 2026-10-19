import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "packaging" / "legacy_cleanup.py"
SPEC = importlib.util.spec_from_file_location("sel_legacy_cleanup", MODULE_PATH)
legacy_cleanup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(legacy_cleanup)


def test_version_comparison_handles_numeric_components():
    assert legacy_cleanup.compare_versions("0.3.2", "0.3.3") < 0
    assert legacy_cleanup.compare_versions("0.3.3", "0.3.3") == 0
    assert legacy_cleanup.compare_versions("0.3.10", "0.3.9") > 0


def test_legacy_action_matrix():
    assert legacy_cleanup.choose_action(None, "0.3.3") == "none"
    assert legacy_cleanup.choose_action("0.3.2", "0.3.3") == "cleanup"
    assert legacy_cleanup.choose_action("0.3.3", "0.3.3") == "block"
    assert legacy_cleanup.choose_action("0.4.0", "0.3.3") == "block"


def test_unknown_legacy_version_is_blocked():
    assert legacy_cleanup.choose_action("not-a-version", "0.3.3") == "block"


def test_uninstaller_must_be_inside_legacy_install_directory(tmp_path):
    install_root = tmp_path / "legacy"
    install_root.mkdir()
    safe = install_root / "unins000.exe"
    outside = tmp_path / "unins000.exe"

    assert legacy_cleanup.is_safe_uninstaller(safe, install_root)
    assert not legacy_cleanup.is_safe_uninstaller(outside, install_root)
    assert not legacy_cleanup.is_safe_uninstaller(install_root / "sel-facturation.exe", install_root)
