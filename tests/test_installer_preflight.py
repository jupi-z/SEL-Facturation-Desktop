import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace


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


def test_registry_uninstall_command_extracts_cached_bundle_path():
    command = '"C:\\Users\\Test User\\AppData\\Local\\Package Cache\\setup.exe" /burn.clean.room /uninstall'

    assert installer_version._command_executable(command) == (
        "C:\\Users\\Test User\\AppData\\Local\\Package Cache\\setup.exe"
    )


def test_preflight_exposes_a_native_status_message_and_preserves_arguments():
    assert preflight.status_message("install", [], "0.3.3")
    assert preflight.status_message("migrate", [{"kind": "legacy", "version": "0.3.2"}], "0.3.3")
    assert preflight.status_message("repair", [{"kind": "burn", "version": "0.3.3"}], "0.3.3")
    assert preflight.status_message("block", [{"kind": "burn", "version": "0.4.0"}], "0.3.3")


def test_preflight_finds_versioned_embedded_burn_engine(tmp_path):
    embedded = tmp_path / "SEL-Facturation-Setup-0.3.3-core.exe"
    embedded.write_bytes(b"burn")

    assert preflight.resolve_core_path(tmp_path) == embedded


def test_preflight_reuses_cached_bundle_for_same_version_repair(tmp_path):
    cached = tmp_path / "SEL-Facturation-Setup-0.3.3.exe"
    cached.write_bytes(b"burn")

    assert preflight.resolve_repair_path(
        [{"kind": "burn", "bundle_path": str(cached)}]
    ) == cached


def test_preflight_repair_requires_cached_bundle_when_missing(tmp_path):
    assert preflight.resolve_repair_path(
        [{"kind": "burn", "bundle_path": str(tmp_path / "missing.exe")}]
    ) is None


def test_preflight_installs_over_same_version_portable_copy_without_repair(monkeypatch, tmp_path):
    fallback = tmp_path / "new-setup.exe"
    fallback.write_bytes(b"new")
    calls = []

    monkeypatch.setattr(preflight, "_core_path", lambda: fallback)
    monkeypatch.setattr(preflight, "current_version", lambda: "0.3.8")
    monkeypatch.setattr(
        preflight,
        "discover_installations",
        lambda: [{"kind": "portable", "version": "0.3.8", "executable_path": "C:/portable/SEL-Facturation.exe"}],
    )
    monkeypatch.setattr(preflight, "_notify", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        preflight.subprocess,
        "run",
        lambda arguments, check: (calls.append(arguments) or SimpleNamespace(returncode=0)),
    )

    assert preflight.main([]) == 0
    assert calls == [[str(fallback)]]


def test_preflight_launches_cached_bundle_in_repair_mode(monkeypatch, tmp_path):
    fallback = tmp_path / "new-setup.exe"
    cached = tmp_path / "cached-setup.exe"
    fallback.write_bytes(b"new")
    cached.write_bytes(b"cached")
    calls = []

    monkeypatch.setattr(preflight, "_core_path", lambda: fallback)
    monkeypatch.setattr(preflight, "current_version", lambda: "0.3.3")
    monkeypatch.setattr(
        preflight,
        "discover_installations",
        lambda: [{"kind": "burn", "version": "0.3.3", "bundle_path": str(cached)}],
    )
    monkeypatch.setattr(preflight, "_notify", lambda *args, **kwargs: None)
    monkeypatch.setattr(
        preflight.subprocess,
        "run",
        lambda arguments, check: (calls.append(arguments) or SimpleNamespace(returncode=0)),
    )

    assert preflight.main([]) == 0
    assert calls == [[str(cached), "/repair"]]


def test_setup_build_wraps_the_wix_bundle_with_preflight():
    script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "legacy" / "windows-v1-local-archive.yml").read_text(encoding="utf-8")

    for source in (script, workflow):
        assert "preflight_launcher.py" in source
        assert "-core.exe" in source
        assert "--add-binary" in source
        assert "version.py;." in source


def test_installation_marker_records_only_portable_identity_metadata(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    executable = tmp_path / "portable" / "SEL-Facturation.exe"

    marker = installer_version.write_installation_marker(executable, "0.3.8")

    assert marker == tmp_path / installer_version.INSTALLATION_MARKER_NAME
    payload = json.loads(marker.read_text(encoding="utf-8"))
    assert payload == {
        "schema_version": 1,
        "kind": "portable",
        "version": "0.3.8",
        "executable_path": str(executable.resolve()),
    }
    assert "password" not in marker.read_text(encoding="utf-8").lower()

