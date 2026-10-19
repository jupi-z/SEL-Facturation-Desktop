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


def test_settings_sections_are_not_rendered_as_nested_cards_and_modals_remain_distinct():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".settings-cards{gap:0;border-top:1px solid var(--border);border-bottom:1px solid var(--border)}" in css
    assert ".settings-cards article{border:0;border-radius:0;background:transparent;" in css
    assert ".modal {" in css and "box-shadow:" in css
