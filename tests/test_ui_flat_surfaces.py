from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_content_panels_use_flat_surface_language():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".panel:not(.modal){background:transparent;border:0;border-radius:0;box-shadow:none" in css
    assert ".dashboard-kpis{border-top:1px solid var(--border);border-bottom:1px solid var(--border);gap:0}" in css
    assert ".dashboard-kpis .metric{border:0;border-right:1px solid var(--border);" in css


def test_status_and_alert_surfaces_are_flat_but_semantically_distinct():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".statusbar{border:0;border-top:1px solid var(--border);border-bottom:1px solid var(--border);" in css
    assert ".notice-warning,.alert{border:0;border-left:3px solid" in css
    assert ".sum .total{border-top:2px solid var(--sel-primary);border-bottom:1px solid var(--border);background:transparent;" in css


def test_settings_uses_flat_tabs_and_modals_remain_available_elsewhere():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")
    html = (ROOT / "templates" / "settings.html").read_text(encoding="utf-8")

    assert ".settings-tabs{display:flex;" in css
    assert ".settings-tab[aria-selected=\"true\"]" in css
    assert ".settings-summary{display:grid;" in css
    assert "data-settings-tab" in html
    assert "settings-cards" not in html
    assert ".modal {" in css and "box-shadow:" in css


def test_settings_catalog_rows_keep_summary_and_edit_controls_compact():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")
    html = (ROOT / "templates" / "settings.html").read_text(encoding="utf-8")

    assert 'class="settings-type-summary"' in html
    assert ".settings-type-list>div{display:grid;grid-template-columns:minmax(220px,1fr) minmax(500px,1.4fr);" in css
    assert ".settings-type-list>div{display:grid;grid-template-columns:minmax(220px,1fr) minmax(500px,1.4fr);align-items:center;gap:24px;padding:14px 0;background:transparent;border:0;" in css
    assert ".settings-type-summary{display:flex;align-items:center;" in css
    assert ".settings-catalog-grid section{display:grid;align-content:start;" in css
    assert ".settings-type-list>div{grid-template-columns:1fr;gap:10px}" in css
