from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_invoice_cancellation_is_marked_for_confirmation():
    html = (ROOT / "templates" / "invoice_detail.html").read_text(encoding="utf-8")

    assert 'data-confirm="' in html


def test_pdf_actions_expose_busy_state_and_confirmation_script():
    script = (ROOT / "static" / "desktop.js").read_text(encoding="utf-8")

    assert "aria-busy" in script
    assert "data-confirm" in script
