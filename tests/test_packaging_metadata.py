from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
NS = {"wix": "http://wixtoolset.org/schemas/v4/wxs"}
MSI_UPGRADE_CODE = "7B258C72-AE0B-4D4E-84DE-725C08F20EA1"
BUNDLE_UPGRADE_CODE = "C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF"


def test_bundle_uses_wix4_exe_arguments():
    root = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()
    bundle = root.find("wix:Bundle", NS)
    chain = bundle.find("wix:Chain", NS)
    exe = chain.find("wix:ExePackage", NS)
    msi = chain.find("wix:MsiPackage", NS)
    assert exe.attrib["SourceFile"] == "$(var.WebView2Installer)"
    assert exe.attrib["PerMachine"] == "yes"
    assert exe.attrib["InstallArguments"] == "/silent /install"
    assert "UninstallArguments" not in exe.attrib
    assert exe.find("wix:InstallCommand", NS) is None
    assert exe.find("wix:UninstallCommand", NS) is None
    assert "DisplayInternalUI" not in msi.attrib
    assert msi.attrib["Visible"] == "no"
    assert msi.find("wix:MsiProperty", NS).attrib == {"Name": "ARPSYSTEMCOMPONENT", "Value": "1"}
    condition = root.find("wix:Bundle", NS).find("bal:Condition", {**NS, "bal": "http://wixtoolset.org/schemas/v4/wxs/bal"})
    assert condition is not None
    assert condition.attrib["Condition"] == "NOT LegacyInnoSetupInstalled"


def test_msi_keeps_program_files_separate_from_local_data():
    root = ET.parse(ROOT / "packaging" / "wix" / "Product.wxs").getroot()
    ns = {"wix": "http://wixtoolset.org/schemas/v4/wxs"}
    install = root.find(".//wix:Directory[@Id='INSTALLFOLDER']", ns)
    assert install is not None
    assert install.attrib["Name"] == "SEL Facturation"
    parent = next(parent for parent in root.iter() if install in list(parent))
    assert parent.attrib == {"Id": "ProgramFiles64Folder"}


def test_msi_upgrade_contract_is_stable_and_never_downgrades():
    root = ET.parse(ROOT / "packaging" / "wix" / "Product.wxs").getroot()
    package = root.find("wix:Package", NS)
    major_upgrade = root.find("wix:Package/wix:MajorUpgrade", NS)

    assert package is not None
    assert package.attrib["UpgradeCode"] == MSI_UPGRADE_CODE
    assert package.attrib["Scope"] == "perMachine"
    assert package.find("wix:Property[@Id='ARPSYSTEMCOMPONENT']", NS).attrib["Value"] == "1"
    assert major_upgrade is not None
    assert major_upgrade.attrib["AllowDowngrades"] == "no"
    assert major_upgrade.attrib["AllowSameVersionUpgrades"] == "no"


def test_bundle_blocks_legacy_inno_and_uses_the_same_program_directory():
    bundle = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot().find("wix:Bundle", NS)
    inno = (ROOT / "packaging" / "SEL-Facturation.iss").read_text(encoding="utf-8")
    bundle_source = (ROOT / "packaging" / "bundle" / "Bundle.wxs").read_text(encoding="utf-8")

    assert bundle is not None
    assert bundle.attrib["UpgradeCode"] == BUNDLE_UPGRADE_CODE
    assert "LegacyInnoSetupInstalled" in bundle_source
    assert "Désinstallez-la avant" in bundle_source
    assert "DefaultDirName={localappdata}\\Programs\\SEL Facturation" in inno


def test_update_contract_keeps_persistent_data_outside_install_directory():
    db_source = (ROOT / "db.py").read_text(encoding="utf-8")
    product_source = (ROOT / "packaging" / "wix" / "Product.wxs").read_text(encoding="utf-8")

    assert "LOCALAPPDATA" in db_source
    assert "SEL_DATA_DIR" in db_source
    assert "sel.sqlite3" in db_source
    assert "INSTALLFOLDER" in product_source
    assert "LOCALAPPDATA\\SEL Facturation" not in product_source
    assert "sel.sqlite3" not in product_source


def test_release_workflow_publishes_only_the_recommended_wix_installer():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    publish_block = workflow.split("Publish private GitHub prerelease", 1)[1]

    assert "gh release create $tag $exe $setup $msi $manifest" in publish_block
    assert "update.json" in publish_block
    assert "inputs.release_tag" in publish_block
    assert "same numeric version as version.py" in publish_block
    assert "same numeric version already has a release" in publish_block
    assert "--target $env:GITHUB_SHA" in publish_block
    assert "SEL-Facturation-Setup-0.3.0.exe" not in publish_block


def test_windows_builds_use_sel_logo_icon():
    icon = ROOT / "packaging" / "sel-facturation.ico"
    assert icon.is_file() and icon.stat().st_size > 1000
    assert "--icon" in (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")
    assert "--icon packaging\\sel-facturation.ico" in workflow


def test_windows_build_embeds_webview2_and_requires_signing_for_published_releases():
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    assert "MicrosoftEdgeWebView2RuntimeInstallerX64.exe" in workflow
    assert "Invoke-WebRequest" in workflow
    assert "SEL_CODESIGN_PFX_BASE64" in workflow
    assert "Get-AuthenticodeSignature" in workflow

