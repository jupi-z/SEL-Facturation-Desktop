import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name, relative_path):
    path = ROOT / relative_path
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


installer_version = load_module("installer_version", "installer_version.py")
preflight = load_module("preflight_launcher", "packaging/preflight_launcher.py")


def test_preflight_matrix_routes_every_installed_state():
    assert installer_version.choose_preflight_action([], "0.3.3") == "install"
    assert installer_version.choose_preflight_action(
        [{"kind": "legacy", "version": "0.3.2"}], "0.3.3"
    ) == "migrate"
    assert installer_version.choose_preflight_action(
        [{"kind": "burn", "version": "0.3.3"}], "0.3.3"
    ) == "repair"
    assert installer_version.choose_preflight_action(
        [{"kind": "msi", "version": "0.3.2"}], "0.3.3"
    ) == "upgrade"
    assert installer_version.choose_preflight_action(
        [{"kind": "burn", "version": "0.4.0"}], "0.3.3"
    ) == "block"
    assert installer_version.choose_preflight_action(
        [{"kind": "legacy", "version": "0.3.3"}], "0.3.3"
    ) == "block"


def test_unknown_or_mixed_installations_are_safe():
    assert installer_version.choose_preflight_action(
        [{"kind": "legacy", "version": None}], "0.3.3"
    ) == "block"


def test_registry_guid_normalization_handles_windows_multisz_values():
    assert installer_version._normalise_guid(["{C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF}"]) == "C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF"
    assert installer_version.choose_preflight_action(
        [
            {"kind": "legacy", "version": "0.3.2"},
            {"kind": "burn", "version": "0.4.0"},
        ],
        "0.3.3",
    ) == "block"


def test_preflight_exposes_a_native_status_message_and_preserves_arguments():
    assert preflight.status_message("install", [], "0.3.3")
    assert preflight.status_message("migrate", [{"kind": "legacy", "version": "0.3.2"}], "0.3.3")
    assert preflight.status_message("repair", [{"kind": "burn", "version": "0.3.3"}], "0.3.3")
    assert preflight.status_message("block", [{"kind": "burn", "version": "0.4.0"}], "0.3.3")


def test_setup_build_wraps_the_wix_bundle_with_preflight():
    script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    for source in (script, workflow):
        assert "preflight_launcher.py" in source
        assert "-core.exe" in source
        assert "--add-binary" in source
        assert "version.py;." in source

