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


def test_sel_palette_uses_the_official_logo_color_anchors():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert "--sel-navy: #003078" in css
    assert "--sel-blue: #1080f8" in css
    assert "--sel-yellow: #f8d800" in css


def test_authentication_surface_is_inverted_with_a_blue_form_panel():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-brand" in css
    assert ".auth-content" in css
    assert "background: var(--sel-blue)" in css
    assert ".auth-content h2" in css
    assert ".auth-content label" in css


def test_auth_logo_uses_a_deterministic_width_without_clipping():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-identity { display: block; width: min(100%, 400px);" in css
    assert ".auth-identity .official-logo { display: block; width: 100%;" in css
    assert ".auth-brand {" in css
    assert "padding-inline: clamp(24px, 4vw, 56px);" in css


def test_authentication_hint_remains_readable_on_the_blue_panel():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-content .auth-hint" in css
    assert "color: #fff" in css
    assert "background: rgba(255, 255, 255, .12)" in css
