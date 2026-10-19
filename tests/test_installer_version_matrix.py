from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
NS = {
    "wix": "http://wixtoolset.org/schemas/v4/wxs",
    "bal": "http://wixtoolset.org/schemas/v4/wxs/bal",
    "util": "http://wixtoolset.org/schemas/v4/wxs/util",
}


def _bundle_root():
    return ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()


def test_msi_has_one_upgrade_identity_and_reinstalls_same_version():
    root = ET.parse(ROOT / "packaging" / "wix" / "Product.wxs").getroot()
    package = root.find("wix:Package", NS)
    major_upgrade = package.find("wix:MajorUpgrade", NS)

    assert package.attrib["UpgradeCode"] == "7B258C72-AE0B-4D4E-84DE-725C08F20EA1"
    assert major_upgrade.attrib["AllowSameVersionUpgrades"] == "yes"
    assert "DowngradeErrorMessage" in major_upgrade.attrib


def test_bundle_detects_legacy_inno_installation_and_runs_safe_migration():
    root = _bundle_root()
    bundle = root.find("wix:Bundle", NS)
    searches = {node.attrib["Variable"]: node for node in bundle.findall("util:RegistrySearch", NS)}
    legacy_key = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{7B258C72-AE0B-4D4E-84DE-725C08F20EA1}_is1"

    assert searches["LegacyInnoDetected"].attrib == {
        "Id": "LegacyInnoDetectedSearch",
        "Variable": "LegacyInnoDetected",
        "Root": "HKCU",
        "Key": legacy_key,
        "Result": "exists",
    }
    assert searches["LegacyInnoVersion"].attrib["Value"] == "DisplayVersion"
    assert searches["LegacyInnoVersion"].attrib["Root"] == "HKCU"

    chain = bundle.find("wix:Chain", NS)
    cleanup = next(package for package in chain.findall("wix:ExePackage", NS) if package.attrib["Id"] == "LegacyInnoCleanup")
    assert cleanup.attrib["SourceFile"] == "$(var.LegacyCleanupPath)"
    assert cleanup.attrib["DetectCondition"] == "NOT LegacyInnoDetected"
    assert cleanup.attrib["Permanent"] == "yes"
    assert cleanup.attrib["Vital"] == "yes"
    assert "--cleanup-legacy" in cleanup.attrib["InstallArguments"]
    assert "[WixBundleVersion]" in cleanup.attrib["InstallArguments"]


def test_bundle_presents_version_and_blocks_unsafe_legacy_states():
    root = _bundle_root()
    bundle = root.find("wix:Bundle", NS)
    standard_ba = root.find(".//bal:WixStandardBootstrapperApplication", NS)
    condition = bundle.find("bal:Condition", NS)

    assert bundle.attrib["UpgradeCode"] == "C84DF0E9-3C3B-4C54-8A31-B2B3C28B94CF"
    assert standard_ba.attrib["ShowVersion"] == "yes"
    assert condition is not None
    assert "installation héritée" in condition.attrib["Message"]
    assert "LegacyInnoCleanup" in condition.attrib["Message"]


def test_builds_compile_the_migration_helper_and_pass_it_to_wix():
    script = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "windows.yml").read_text(encoding="utf-8")

    for source in (script, workflow):
        assert "legacy_cleanup.py" in source
        assert "SEL-LegacyCleanup" in source
        assert "LegacyCleanupPath" in source

