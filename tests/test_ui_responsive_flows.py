from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_settings_tabs_wrap_instead_of_creating_a_clipped_horizontal_strip():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".settings-tabs{flex-wrap:wrap;overflow:visible" in css
    assert ".settings-tab{flex:1 1 auto" in css


def test_long_modal_forms_keep_one_scroll_region_and_visible_actions():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".modal>form{overscroll-behavior:contain" in css
    assert ".modal-actions{position:sticky;bottom:0" in css


def test_settings_tabs_keep_the_accessible_tab_contract():
    html = (ROOT / "templates" / "settings.html").read_text(encoding="utf-8")

    assert 'role="tablist"' in html
    assert 'role="tab"' in html
    assert 'aria-controls=' in html
    assert 'role="tabpanel"' in html
