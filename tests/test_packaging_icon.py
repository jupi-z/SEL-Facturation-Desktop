from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[1]


def _ico_sizes(path: Path) -> set[tuple[int, int]]:
    data = path.read_bytes()
    reserved, image_type, count = struct.unpack_from("<HHH", data, 0)
    assert (reserved, image_type) == (0, 1)
    sizes = set()
    for index in range(count):
        width, height = struct.unpack_from("<BB", data, 6 + index * 16)
        sizes.add((width or 256, height or 256))
    return sizes


def test_windows_icon_is_sel_brand_and_multiresolution():
    source = ROOT / "packaging" / "sel-icon-source.svg"
    icon = ROOT / "packaging" / "sel-facturation.ico"
    assert source.exists()
    assert "data:image/png;base64," in source.read_text(encoding="utf-8")
    assert icon.exists()
    assert _ico_sizes(icon) == {(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)}


def test_windows_packaging_uses_the_shared_sel_icon():
    build = (ROOT / "build-windows.ps1").read_text(encoding="utf-8")
    spec = (ROOT / "SEL-Facturation.spec").read_text(encoding="utf-8")
    bundle = (ROOT / "packaging" / "bundle" / "Bundle.wxs").read_text(encoding="utf-8")
    assert "packaging\\prepare-icon.py" in build
    assert "--icon 'packaging\\sel-facturation.ico'" in build
    assert "icon=['packaging/sel-facturation.ico']" in spec
    assert "IconSourceFile=\"$(var.IconPath)\"" in bundle
