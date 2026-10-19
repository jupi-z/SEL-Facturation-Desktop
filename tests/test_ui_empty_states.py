from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_empty_directories_offer_the_next_relevant_action():
    clients = (ROOT / "templates" / "clients.html").read_text(encoding="utf-8")
    invoices = (ROOT / "templates" / "invoices.html").read_text(encoding="utf-8")

    assert 'class="empty-state"' in clients
    assert 'data-modal-open="client-create-modal"' in clients
    assert 'class="empty-state"' in invoices
    assert 'href="{{ url_for(\'new_invoice\') }}"' in invoices


def test_empty_dashboard_and_payment_sections_are_explanatory():
    dashboard = (ROOT / "templates" / "dashboard.html").read_text(encoding="utf-8")
    detail = (ROOT / "templates" / "invoice_detail.html").read_text(encoding="utf-8")

    assert 'class="empty-state"' in dashboard
    assert 'class="empty-state compact"' in detail


def test_directory_and_history_tables_expose_column_headers():
    dashboard = (ROOT / "templates" / "dashboard.html").read_text(encoding="utf-8")
    clients = (ROOT / "templates" / "clients.html").read_text(encoding="utf-8")
    invoices = (ROOT / "templates" / "invoices.html").read_text(encoding="utf-8")

    assert dashboard.count('scope="col"') >= 5
    assert clients.count('scope="col"') >= 7
    assert invoices.count('scope="col"') >= 6


def test_empty_state_has_a_compact_flat_visual_treatment():
    css = (ROOT / "static" / "brand.css").read_text(encoding="utf-8")

    assert ".empty-state{" in css
    assert ".empty-state.compact{" in css
