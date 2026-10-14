from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_authenticated_layout_uses_available_desktop_width():
    css = (ROOT / "static" / "app.css").read_text(encoding="utf-8")

    assert "main{width:100%;max-width:none;" in css
    assert "padding-inline:clamp(20px,2.2vw,40px)" in css


def test_layout_keeps_dense_forms_and_tables_responsive():
    css = (ROOT / "static" / "app.css").read_text(encoding="utf-8")

    assert ".fields{min-width:0}" in css
    assert ".tablewrap{width:100%;max-width:100%;overflow-x:auto}" in css
    assert "@media(max-width:760px)" in css
