import json
import re
from datetime import date

import db
from app import create_app
from domain import build_combined_snapshot


def _token(client, path):
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


def _combined_snapshot():
    electricity = {
        "code": "ELECTRICITY", "mode": "DIRECT_KWH", "quantity": "100", "unit": "kWh",
        "unit_price_excl_tax": "0.095",
        "lines": [{"label": "Consommation", "quantity": "100", "unit_price": "0.095", "amount": "9.50"}],
        "taxes": [{"code": "VAT", "rate": "16", "basis": "9.50"}],
    }
    water = {
        "code": "WATER", "mode": "DIRECT_M3", "quantity": "10", "unit": "m3",
        "unit_price_excl_tax": "0.40",
        "lines": [{"label": "Consommation d'eau", "quantity": "10", "unit_price": "0.40", "amount": "4.00"}],
        "taxes": [],
    }
    return build_combined_snapshot(electricity, water, "USD")


def _prepared_client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariffs",
        "residential_rate": "0.095", "commercial_rate": "0.12",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client test combiné",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-COMB",
        "meter_number": "", "coefficient": "1",
    })
    return client


def _insert_combined_invoice():
    snapshot = _combined_snapshot()
    with db.connect() as connection:
        client = dict(connection.execute("SELECT * FROM clients WHERE id=1").fetchone())
        settings = dict(db.settings(connection))
        cursor = connection.execute(
            """INSERT INTO invoices(number,client_id,billing_period,issue_date,due_date,
               client_snapshot,company_snapshot,calc_snapshot,currency,note)
               VALUES(?,?,?,?,?,?,?,?,?,?)""",
            (
                "SEL-2026-000700", 1, "2026-10", date.today().isoformat(), "2026-10-31",
                json.dumps(client, ensure_ascii=False), json.dumps(settings, ensure_ascii=False),
                json.dumps(snapshot, ensure_ascii=False), "USD", "",
            ),
        )
        connection.commit()
        return cursor.lastrowid


def test_combined_mode_is_visible_but_real_emission_is_guarded(tmp_path, monkeypatch):
    client = _prepared_client(tmp_path, monkeypatch)
    page = client.get("/invoices/new").get_data(as_text=True)
    assert 'name="billing_mode"' in page
    assert "Eau + électricité" in page
    assert "Le mode eau + électricité sera activé" in page

    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "billing_mode": "COMBINED",
        "client_id": "1", "billing_period": "2026-10", "consumption_kwh": "100",
        "due_date": "2026-10-31",
    })
    assert response.status_code == 200
    assert "attend la validation" in response.get_data(as_text=True)
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0


def test_combined_detail_pdf_and_csv_use_additive_snapshot(tmp_path, monkeypatch):
    client = _prepared_client(tmp_path, monkeypatch)
    invoice_id = _insert_combined_invoice()

    detail = client.get(f"/invoices/{invoice_id}")
    html = detail.get_data(as_text=True)
    assert detail.status_code == 200
    assert "ÉLECTRICITÉ" in html
    assert "EAU" in html
    assert "Sous-total électricité" in html
    assert "Sous-total eau" in html
    assert "15,02" in html

    pdf = client.get(f"/invoices/{invoice_id}/pdf")
    assert pdf.status_code == 200 and pdf.data.startswith(b"%PDF-")
    cdf = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF&rate=2360")
    assert cdf.status_code == 200 and cdf.data.startswith(b"%PDF-")

    exported = client.get("/export/invoices.csv")
    assert exported.status_code == 200
    csv_text = exported.get_data(as_text=True)
    assert "COMBINED_WATER_ELECTRICITY" in csv_text
    assert "15.02" in csv_text

    portable = json.loads(client.get("/export/portable.json").data)
    assert portable["schema_version"] == 1
    assert portable["data"]["invoices"][0]["calc_snapshot"]["services"]
