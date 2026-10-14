from pathlib import Path
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
WIX_NS = {"wix": "http://wixtoolset.org/schemas/v4/wxs"}


def _root(path: Path):
    return ET.parse(path).getroot()


def test_msi_declares_stable_upgrade_and_preserves_user_data():
    root = _root(ROOT / "packaging" / "wix" / "Product.wxs")

    package = root.find("wix:Package", WIX_NS)
    assert package is not None
    assert package.attrib["UpgradeCode"] == "7B258C72-AE0B-4D4E-84DE-725C08F20EA1"

    source = (ROOT / "packaging" / "wix" / "Product.wxs").read_text(encoding="utf-8")
    assert "%LOCALAPPDATA%\\SEL Facturation" not in source
    assert "$(var.ExePath)" in source


def test_bundle_chains_msi_and_webview2():
    root = _root(ROOT / "packaging" / "bundle" / "Bundle.wxs")
    bundle = root.find("wix:Bundle", WIX_NS)
    assert bundle is not None
    chain = bundle.find("wix:Chain", WIX_NS)
    assert chain is not None
    assert chain.find("wix:ExePackage", WIX_NS) is not None
    assert chain.find("wix:MsiPackage", WIX_NS) is not None

