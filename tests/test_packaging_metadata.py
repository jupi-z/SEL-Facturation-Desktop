from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
NS = {
    "wix": "http://wixtoolset.org/schemas/v4/wxs",
    "bal": "http://wixtoolset.org/schemas/v4/wxs/bal",
    "util": "http://wixtoolset.org/schemas/v4/wxs/util",
}


def test_bundle_uses_wix4_exe_arguments():
    root = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()
    bundle = root.find("wix:Bundle", NS)
    chain = bundle.find("wix:Chain", NS)
    packages = chain.findall("wix:ExePackage", NS)
    exe = next(node for node in packages if node.attrib["Id"] == "WebView2EvergreenBootstrapper")
    legacy = next(node for node in packages if node.attrib["Id"] == "LegacyInnoCleanup")
    msi = chain.find("wix:MsiPackage", NS)
    assert exe.attrib["SourceFile"] == "$(var.WebView2Installer)"
    # The application MSI is per-user; keeping WebView2 per-user avoids
    # requiring an elevated token just to secure C:\\ProgramData\\Package Cache.
    assert exe.attrib["PerMachine"] == "no"
    assert exe.attrib["InstallArguments"] == "/silent /install"
    assert exe.attrib["Vital"] == "yes"
    ordered_packages = list(chain)
    assert ordered_packages.index(exe) < ordered_packages.index(legacy) < ordered_packages.index(chain.find("wix:MsiPackage", NS))
    assert "UninstallArguments" not in exe.attrib
    assert exe.find("wix:InstallCommand", NS) is None
    assert exe.find("wix:UninstallCommand", NS) is None
    assert "DisplayInternalUI" not in msi.attrib


def test_bundle_uses_the_sel_application_icon():
    root = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()

    assert root.find("wix:Bundle", NS).get("IconSourceFile") == "$(var.IconPath)"


def test_msi_keeps_program_files_separate_from_local_data():
    root = ET.parse(ROOT / "packaging" / "wix" / "Product.wxs").getroot()
    ns = {"wix": "http://wixtoolset.org/schemas/v4/wxs"}
    install = root.find(".//wix:Directory[@Id='INSTALLFOLDER']", ns)
    assert install is not None
    assert install.attrib["Name"] == "SEL Facturation"
    parent = next(parent for parent in root.iter() if install in list(parent))
    assert parent.attrib == {"Id": "ProgramsFolder", "Name": "Programs"}


def test_windows_builds_use_sel_logo_icon():
    icon = ROOT / "packaging" / "sel-facturation.ico"
    assert icon.is_file() and icon.stat().st_size > 1000
    assert "--icon" in (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    assert "--icon packaging\\sel-facturation.ico" in workflow


def test_windows_build_embeds_small_signed_webview2_bootstrapper_and_separates_publication_modes():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    build_script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    downloader = (ROOT / "packaging" / "download-webview2-bootstrapper.ps1").read_text(encoding="utf-8")

    assert "MicrosoftEdgeWebview2Setup.exe" in workflow
    assert "MicrosoftEdgeWebview2Setup.exe" in build_script
    assert "2124703" in downloader
    assert "Get-AuthenticodeSignature" in downloader
    assert "Microsoft Corporation" in downloader
    assert "100MB" not in downloader
    assert "512KB" in downloader and "10MB" in downloader
    assert "for ($attempt = 1; $attempt -le 3; $attempt++)" in downloader
    assert "$partialPath" in downloader and "Move-Item" in downloader
    assert "download-webview2-bootstrapper.ps1" in workflow
    assert "download-webview2-bootstrapper.ps1" in build_script
    assert "SEL_CODESIGN_PFX_BASE64" in workflow
    assert "Get-AuthenticodeSignature" in workflow
    assert "wix extension add -g WixToolset.Util.wixext/4.0.5" in workflow
    assert "publish_unsigned_preview" in workflow
    assert "PUBLISH_UNSIGNED_PREVIEW" in workflow
    assert "aperçu non signé" in workflow
    assert "Signature Authenticode absente" in workflow
    assert "if: github.event_name == 'workflow_dispatch' && inputs.publish_release == true" in workflow
    assert "La publication doit partir de main après fusion et CI réussie." in workflow
    assert "cle publique Ed25519 SEL n'est pas epinglee" in workflow
    publish_block = workflow.split("Publish GitHub prerelease without overwriting", 1)[1].split("Upload unsigned acceptance packages", 1)[0]
    assert "’" not in publish_block


def test_windows_packaging_collects_reportlab_dynamic_modules_and_smokes_the_frozen_app():
    build_script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    spec = (ROOT / "SEL-Facturation.spec").read_text(encoding="utf-8")
    readme = (ROOT / "README.md").read_text(encoding="utf-8")

    assert "--collect-all reportlab" in build_script
    assert "--collect-all reportlab" in workflow
    assert "collect_all('reportlab')" in spec
    assert "--collect-all reportlab" in readme
    assert "--sel-packaging-smoke" in workflow


def test_webview2_detection_covers_machine_and_user_runtime_locations():
    bundle = (ROOT / "packaging" / "bundle" / "Bundle.wxs").read_text(encoding="utf-8")

    assert "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" in bundle
    assert "Root=\"HKLM\"" in bundle
    assert "Root=\"HKCU\"" in bundle
    assert '&lt;&gt; &quot;0.0.0.0&quot;' in bundle
    assert "WebView2RuntimeVersionMachine AND" in bundle
    assert "WebView2RuntimeVersionUser AND" in bundle


def test_bundle_blocks_unsupported_architecture_and_missing_dotnet_framework():
    root = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()
    bundle = root.find("wix:Bundle", NS)
    conditions = bundle.findall("bal:Condition", NS)
    searches = bundle.findall("util:RegistrySearch", NS)

    assert any(condition.attrib["Condition"] == "VersionNT64" for condition in conditions)
    assert any(condition.attrib["Condition"] == "NetFrameworkRelease AND NetFrameworkRelease >= 394802" for condition in conditions)
    netfx_search = next(search for search in searches if search.attrib["Id"] == "NetFrameworkReleaseSearch")
    assert netfx_search.attrib["Value"] == "Release"
    assert netfx_search.attrib["Bitness"] == "always64"


def test_signing_order_protects_embedded_payloads():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    assert workflow.index("Sign application payloads before MSI") < workflow.index("Download and validate WebView2 bootstrapper, then compile MSI")
    assert workflow.index("Sign MSI before Burn") < workflow.index("wix build packaging\\bundle\\Bundle.wxs")
    assert workflow.index("Sign Burn core before preflight wrapper") < workflow.index("preflight_launcher.py")
    assert workflow.index("Sign final setup launcher") > workflow.index("preflight_launcher.py")


def test_release_workflow_has_one_supported_installer_path():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    build_script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")

    assert "Install Inno Setup" not in workflow
    assert "Compile Windows installer" not in workflow
    assert "Inno Setup absent" not in build_script
    assert "bootstrapper WiX" in build_script
    assert "wix build packaging\\wix\\Product.wxs" in build_script
    assert "MicrosoftEdgeWebview2Setup.exe" in build_script


def test_wix_build_resolves_paths_before_preprocessing():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    build_script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    for source in (workflow, build_script):
        assert "$exePath" in source
        assert "$msiPath" in source
        assert "ExePath=$((Resolve-Path" not in source
        assert "MsiPath=$((Resolve-Path" not in source
    assert '"IconPath=$iconPath"' in workflow


def test_release_tag_is_derived_from_the_application_version():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    assert "$tag = \"v$version\"" in workflow
    assert "v0.3.0-rc5" not in workflow
    assert 'SEL Facturation Desktop v$version - installateur Windows' in workflow


def test_wix_workflow_installs_all_extensions_used_by_bundle_build():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    assert "wix extension add -g WixToolset.Bal.wixext/4.0.5" in workflow
    assert "wix extension add -g WixToolset.Util.wixext/4.0.5" in workflow
    assert "-ext WixToolset.Bal.wixext -ext WixToolset.Util.wixext" in workflow

