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
    assert "background: var(--sel-primary)" in css
    assert ".auth-content h2" in css
    assert ".auth-content label" in css


def test_sel_navy_is_the_dominant_surface_color_not_the_light_blue_accent():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert "--sel-primary: #003078" in css
    assert "background: var(--sel-primary);" in css
    assert ".auth-content { max-width: none; margin: 0; background: var(--sel-primary);" in css
    assert ".statusbar { border-left-color: var(--sel-primary);" in css


def test_auth_logo_uses_a_deterministic_width_without_clipping():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-identity { display: block; width: min(100%, 340px);" in css
    assert ".auth-identity .official-logo { display: block; width: 100%;" in css
    assert ".auth-brand {" in css
    assert "padding-inline: clamp(24px, 4vw, 56px);" in css


def test_auth_layout_keeps_brand_and_form_balanced_on_wide_windows():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-shell { grid-template-columns: minmax(420px, 44%) minmax(520px, 1fr);" in css
    assert ".auth-brand { min-height: 100vh; overflow: hidden;" in css
    assert ".auth-identity { display: block; width: min(100%, 340px);" in css
    assert "padding: clamp(32px, 5vw, 72px) clamp(32px, 6vw, 96px);" in css
    assert ".auth-stage { width: min(100%, 460px);" in css
    assert ".auth-form { width: 100%; max-width: none;" in css
    assert "@media (min-width: 761px) and (max-width: 1100px)" in css
    assert ".auth-shell { grid-template-columns: minmax(360px, 40%) minmax(0, 1fr);" in css
    assert "@media (max-width: 760px)" in css
    assert ".auth-brand { min-height: 250px;" in css


def test_authentication_hint_remains_readable_on_the_blue_panel():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".auth-content .auth-hint" in css
    assert "color: #fff" in css
    assert "background: rgba(255, 255, 255, .12)" in css


def test_auth_stage_centers_the_complete_login_group_and_keeps_footer_links_visible():
    base = (ROOT / "templates" / "base.html").read_text(encoding="utf-8")
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert '<div class="auth-stage">' in base
    assert ".auth-stage { width: min(100%, 460px);" in css
    assert ".auth-footer { width: 100%;" in css
    assert "text-align: center;" in css
    assert ".auth-content .auth-footer a { color: #fff;" in css


def test_about_surface_keeps_sel_primary_and_developer_marks_secondary():
    base = (ROOT / "templates" / "base.html").read_text(encoding="utf-8")
    about = ROOT / "templates" / "about.html"
    assert "url_for('about')" in base
    assert about.is_file()
    about_text = about.read_text(encoding="utf-8")
    assert "monarque-group-horizontal.png" in about_text
    assert "ltex.png" in about_text
    assert "SOCIÉTÉ D’ÉNERGIE DU LUALABA" in about_text


def test_updates_surface_centers_a_compact_content_panel():
    template = (ROOT / "templates" / "updates.html").read_text(encoding="utf-8")
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert '<section class="panel updates-panel">' in template
    assert ".updates-main{display:flex;align-items:center;" in css
    assert ".updates-panel{width:min(100%,760px);" in css
    assert "margin:0 auto;" in css
