from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_authenticated_layout_uses_available_desktop_width():
    css = "\n".join(
        (ROOT / "static" / name).read_text(encoding="utf-8")
        for name in ("app.css", "brand.css")
    )

    assert "main{width:100%;max-width:none;" in css
    assert "padding-inline:clamp(20px,2.2vw,40px)" in css


def test_layout_keeps_dense_forms_and_tables_responsive():
    css = "\n".join(
        (ROOT / "static" / name).read_text(encoding="utf-8")
        for name in ("app.css", "brand.css")
    )

    assert ".fields{min-width:0}" in css
    assert ".tablewrap{width:100%;max-width:100%;overflow-x:auto}" in css
    assert "@media(max-width:760px)" in css


def test_dashboard_uses_five_column_desktop_density_with_safe_breakpoints():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".kpis.dashboard-kpis{grid-template-columns:repeat(5,minmax(0,1fr));" in css
    assert ".dashboard-kpis .metric{min-width:0;" in css
    assert "@media(max-width:1180px)" in css
    assert "@media(max-width:520px)" in css


def test_shared_toolbar_and_filter_layout_prevent_wasted_rows():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".toolbar-row{display:flex;" in css
    assert ".filters-row{display:flex;" in css
    assert ".toolbar-search{flex:1 1 360px;" in css


def test_modal_keyboard_navigation_is_trapped_inside_open_dialog():
    js = (ROOT / "static" / "desktop.js").read_text(encoding="utf-8")

    assert "modal.addEventListener('keydown'" in js
    assert "event.key !== 'Tab'" in js
    assert "focusable[focusable.length - 1]" in js
