from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
NS = {"wix": "http://wixtoolset.org/schemas/v4/wxs"}


def test_bundle_uses_wix4_exe_arguments():
    root = ET.parse(ROOT / "packaging" / "bundle" / "Bundle.wxs").getroot()
    bundle = root.find("wix:Bundle", NS)
    chain = bundle.find("wix:Chain", NS)
    exe = chain.find("wix:ExePackage", NS)
    msi = chain.find("wix:MsiPackage", NS)
    assert exe.attrib["InstallArguments"] == "/silent /install"
    assert exe.attrib["UninstallArguments"] == "/uninstall /silent"
    assert exe.find("wix:InstallCommand", NS) is None
    assert exe.find("wix:UninstallCommand", NS) is None
    assert "DisplayInternalUI" not in msi.attrib

