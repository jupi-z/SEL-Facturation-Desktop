from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_official_sel_logo_is_packaged_for_the_user_interface():
    logo = ROOT / "static" / "sel-official-logo.png"

    assert logo.is_file()
    assert logo.stat().st_size > 10_000


def test_sel_brand_tokens_are_centralized_in_the_stylesheet():
    css = (ROOT / "static" / "app.css").read_text(encoding="utf-8")

    assert "--sel-navy:" in css
    assert "--sel-blue:" in css
    assert "--sel-yellow:" in css
    assert "--focus-ring:" in css
    brand_css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")
    assert "--green:" not in brand_css


def test_templates_reference_the_official_logo_for_ui_surfaces():
    base = (ROOT / "templates" / "base.html").read_text(encoding="utf-8")

    assert "filename='sel-official-logo.png'" in base
    assert "filename='sel-logo.png'" not in base


def test_official_logo_has_contrast_support_on_the_auth_panel():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-identity .official-logo" in css
    assert "background: #fff" in css
