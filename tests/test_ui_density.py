from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def test_dashboard_marks_dense_summary_surfaces():
    html = (ROOT / "templates" / "dashboard.html").read_text(encoding="utf-8")

    assert 'class="kpis dashboard-kpis"' in html
    assert 'class="statusbar dashboard-status' in html


def test_clients_and_invoices_keep_controls_on_shared_toolbar_rows():
    clients = (ROOT / "templates" / "clients.html").read_text(encoding="utf-8")
    invoices = (ROOT / "templates" / "invoices.html").read_text(encoding="utf-8")

    assert 'class="panel toolbar-panel"' in clients
    assert 'class="toolbar-row"' in clients
    assert 'class="toolbar-row"' in invoices
    assert 'class="search toolbar-search filters-row"' in clients
    assert 'class="search toolbar-search filters-row"' in invoices
