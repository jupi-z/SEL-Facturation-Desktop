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
    assert "UninstallArguments" not in exe.attrib
    assert exe.find("wix:InstallCommand", NS) is None
    assert exe.find("wix:UninstallCommand", NS) is None
    assert "DisplayInternalUI" not in msi.attrib


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

