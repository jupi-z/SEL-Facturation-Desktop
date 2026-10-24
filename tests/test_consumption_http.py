import html
import json
import re
import csv
from io import StringIO
from datetime import date, timedelta
from io import BytesIO

from pypdf import PdfReader

import app as app_module
from app import create_app
import db
from invoice_pdf import internal_qr_payload


def _token(client, path):
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


def _approve_default_electricity():
    """Test fixture: explicitly approve the seeded values; never production data."""
    with db.connect() as connection:
        connection.execute("UPDATE tariff_plans SET confirmed=1,source='TEST fixture approval' WHERE service_code='ELECTRICITY'")
        connection.execute("UPDATE tax_profiles SET confirmed=1,source='TEST fixture approval' WHERE service_code='ELECTRICITY'")
        connection.commit()


def test_monthly_direct_consumption_invoice_without_meter(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("UPDATE settings SET value='CDF' WHERE key='currency'")
        connection.execute("UPDATE clients SET customer_type='RESIDENTIAL' WHERE 1=0")
        connection.commit()
    response = client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client Résidentiel",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-1",
        "meter_number": "", "coefficient": "1",
    })
    assert response.status_code == 302
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes", "due_date": "2026-10-31",
    })
    assert preview.status_code == 200
    assert "11,02" in preview.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        row = connection.execute("SELECT id,currency FROM invoices").fetchone()
        invoice_id = row[0]
        assert row[1] == "USD"
    with db.connect() as connection:
        connection.execute("UPDATE exchange_rates SET rate='2500' WHERE base_currency='USD' AND quote_currency='CDF'")
        connection.commit()
    variant = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF")
    assert variant.status_code == 200 and variant.data.startswith(b"%PDF-")
    text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(variant.data)).pages)
    assert "2500" in text

    captured = {}
    def capture_render(invoice, payments, qr_reference=None):
        captured["currency"] = invoice["currency"]
        captured["qr_reference"] = qr_reference
        return b"%PDF-test"
    monkeypatch.setattr(app_module, "render_invoice", capture_render)
    converted = client.get(f"/invoices/{invoice_id}/pdf?currency=CDF")
    assert converted.status_code == 200
    assert captured["currency"] == "CDF"
    qr_invoice, qr_calc = captured["qr_reference"]
    assert qr_invoice["currency"] == "USD"
    assert '"currency":"USD"' in internal_qr_payload(qr_invoice, qr_calc)


def test_current_month_invoice_still_works_after_mid_month_tariff_save(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})

    settings = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariffs",
        "residential_rate": "0.095", "commercial_rate": "0.12",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    assert settings.status_code == 302

    client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client du mois",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-2",
        "meter_number": "", "coefficient": "1",
    })
    period = date.today().strftime("%Y-%m")
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": period, "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes",
        "due_date": date.today().replace(day=28).isoformat(),
    })

    assert preview.status_code == 200
    assert "11,02" in preview.get_data(as_text=True)


def test_invoice_accepts_a_past_due_date_for_late_billing(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariffs",
        "residential_rate": "0.095", "commercial_rate": "0.12",
        "exchange_rate_usd_cdf": "2360", "vat_rate": "16", "confirmed": "yes",
    })
    client.post("/clients", data={
        "_csrf": _token(client, "/clients"), "name": "Client échéance passée",
        "address": "Kolwezi", "customer_type": "RESIDENTIAL", "contract_number": "C-PAST",
        "meter_number": "", "coefficient": "1",
    })
    past_due = (date.today() - timedelta(days=1)).isoformat()
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": date.today().strftime("%Y-%m"), "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes", "due_date": past_due,
    })

    assert preview.status_code == 200
    assert f"Échéance : {past_due}" in preview.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        assert connection.execute("SELECT due_date FROM invoices").fetchone()[0] == past_due


def test_water_zero_is_included_in_combined_invoice_when_customer_is_connected(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_enabled) VALUES('SEL-C-WATER','Client eau','Kolwezi','','RESIDENTIAL',1)")
        connection.execute(
            "INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('WATER_TEST','RESIDENTIAL','M3','0.40','USD','0','2026-01-01',1,'WATER','Fixture de test synthétique',1)"
        )
        connection.execute(
            "INSERT INTO tax_profiles(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('WATER_TAX_TEST','Règle eau test','0','NONE','0','2026-01-01',1,'WATER','Fixture de test synthétique',1)"
        )
        connection.commit()

    incomplete = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "invalide", "water_m3": "0",
        "due_date": "2026-10-31",
    })
    form_html = incomplete.get_data(as_text=True)
    water_input = re.search(r'<input[^>]+name="water_m3"[^>]*>', form_html).group(0)
    assert "required" in water_input and "disabled" not in water_input

    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes",
        "due_date": "2026-10-31",
    })
    assert response.status_code == 200
    response_html = response.get_data(as_text=True)
    assert "Consommation d’eau" in response_html
    assert "0,00" in response.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', response.get_data(as_text=True)).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    detail = client.get(created.location).get_data(as_text=True)
    assert "Consommation d’eau" in detail
    assert "0 M3" in detail
    with db.connect() as connection:
        snapshot = json.loads(connection.execute("SELECT calc_snapshot FROM invoices").fetchone()[0])
    assert snapshot["total"] == "11.02"
    assert [service["service_code"] for service in snapshot["services"]] == ["ELECTRICITY", "WATER"]
    assert snapshot["services"][1]["quantity"] == "0"
    assert snapshot["services"][1]["lines"][0]["amount"] == "0.00"
    exported = list(csv.DictReader(StringIO(client.get("/export/invoices.csv").get_data(as_text=True).lstrip("\ufeff")), delimiter=";"))
    assert exported[0]["water_m3"] == "0"
    assert exported[0]["water_total"] == "0.00"


def test_preexisting_subscriber_always_has_water_and_zero_is_invoiceable_without_fake_tariff(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.commit()

    with db.connect() as connection:
        cursor = connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,customer_type,water_enabled) VALUES('SEL-C-ELECTRIC','Client électricité','Kolwezi','','RESIDENTIAL',0)"
        )
        connection.execute("UPDATE clients SET water_connection_status='CONNECTED' WHERE id=?",(cursor.lastrowid,))
        client_id = cursor.lastrowid
        connection.commit()
    form = client.get("/invoices/new").get_data(as_text=True)
    assert 'id="water-consumption-field" class="field-stack water-field"' in form
    assert re.search(r'<input id="water_m3"[^>]*name="water_m3"[^>]*value="0"[^>]*required', form)

    missing_water = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": str(client_id),
        "billing_period": "2026-10", "consumption_kwh": "100",
    })
    assert "Émission interrompue" in html.unescape(missing_water.get_data(as_text=True))
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0

    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": str(client_id),
        "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "0", "water_zero_confirmed": "yes",
        "due_date": "2026-10-31",
    })

    assert response.status_code == 200
    preview = response.get_data(as_text=True)
    assert "Émission interrompue" not in preview
    assert "Consommation d’eau" in preview
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        invoice = connection.execute("SELECT id,calc_snapshot FROM invoices").fetchone()
        snapshot = json.loads(invoice["calc_snapshot"])
    assert [service["service_code"] for service in snapshot["services"]] == ["ELECTRICITY", "WATER"]
    assert snapshot["services"][1]["quantity"] == "0"
    assert snapshot["services"][1]["subtotal"] == "0.00"
    assert snapshot["services"][1]["lines"][0]["unit_price"] == ""
    pdf = client.get(f"/invoices/{invoice['id']}/pdf")
    pdf_text = "\n".join(page.extract_text() or "" for page in PdfReader(BytesIO(pdf.data)).pages)
    assert "0 M3" in pdf_text and "Eau" in pdf_text
    assert "TVA eau" not in pdf_text


def test_positive_water_requires_real_confirmed_tariff_for_any_subscriber(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        cursor = connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,customer_type,water_enabled) VALUES('SEL-C-ELECTRIC','Client électricité','Kolwezi','','RESIDENTIAL',0)"
        )
        connection.execute("UPDATE clients SET water_connection_status='CONNECTED' WHERE id=?", (cursor.lastrowid,))
        client_id = cursor.lastrowid
        connection.commit()

    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": str(client_id),
        "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "2",
        "due_date": "2026-10-31",
    })

    assert response.status_code == 200
    response_html = html.unescape(response.get_data(as_text=True))
    assert "tarif confirmé" in response_html
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0


def test_water_invoice_is_blocked_when_water_tariff_or_tax_is_not_confirmed(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    _approve_default_electricity()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_enabled) VALUES('SEL-C-WATER','Client eau','Kolwezi','','RESIDENTIAL',1)")
        connection.execute("UPDATE clients SET water_connection_status='CONNECTED' WHERE code='SEL-C-WATER'")
        connection.commit()
    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "100", "water_m3": "5",
    })

    assert response.status_code == 200
    assert "tarif confirmé" in response.get_data(as_text=True)
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0] == 0


def test_zero_water_requires_explicit_ack_and_changed_pricing_invalidates_preview(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client=create_app().test_client()
    _approve_default_electricity()
    client.post("/setup",data={"_csrf":_token(client,"/setup"),"password":"TestPassword2026!","confirm":"TestPassword2026!"})
    client.post("/login",data={"_csrf":_token(client,"/login"),"password":"TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_connection_status) VALUES('SEL-C-ZERO-ACK','Client test','Kolwezi','','RESIDENTIAL','UNKNOWN')")
        connection.commit()
    form={"client_id":"1","billing_period":"2026-10","consumption_kwh":"100","water_m3":"0","due_date":"2026-10-31"}
    rejected=client.post("/invoices/new",data={"_csrf":_token(client,"/invoices/new"),**form})
    assert "Confirmez explicitement" in html.unescape(rejected.get_data(as_text=True))
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0]==0

    preview=client.post("/invoices/new",data={"_csrf":_token(client,"/invoices/new"),**form,"water_zero_confirmed":"yes"})
    assert "TOTAL DE LA FACTURE" in preview.get_data(as_text=True)
    with db.connect() as connection:
        connection.execute("UPDATE tariff_plans SET unit_price='0.10' WHERE code='RESIDENTIAL-2026'")
        connection.commit()
    confirm=client.post("/invoices/new/confirm",data={"_csrf":_token(client,"/invoices/new")})
    assert confirm.status_code==302
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0]==0
        assert connection.execute("SELECT COUNT(*) FROM sequences").fetchone()[0]==0


def test_positive_water_requires_known_physical_connection_even_with_approved_tariff(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR",str(tmp_path))
    client=create_app().test_client()
    _approve_default_electricity()
    client.post("/setup",data={"_csrf":_token(client,"/setup"),"password":"TestPassword2026!","confirm":"TestPassword2026!"})
    client.post("/login",data={"_csrf":_token(client,"/login"),"password":"TestPassword2026!"})
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("INSERT INTO clients(code,name,address,meter_number,customer_type,water_connection_status) VALUES('SEL-C-UNKNOWN','Client test','Kolwezi','','RESIDENTIAL','UNKNOWN')")
        connection.execute("INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) VALUES('WATER-OK','RESIDENTIAL','M3','0.40','USD','0','2026-01-01',1,'WATER','SEL-confirmed test fixture',1)")
        connection.execute("INSERT INTO tax_profiles(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) VALUES('WATER-TAX-OK','Eau test','0','NONE','0','2026-01-01',1,'WATER','SEL-confirmed test fixture',1)")
        connection.commit()
    response=client.post("/invoices/new",data={"_csrf":_token(client,"/invoices/new"),"client_id":"1","billing_period":"2026-10","consumption_kwh":"100","water_m3":"2"})
    assert "raccordement physique" in html.unescape(response.get_data(as_text=True))
    with db.connect() as connection:
        assert connection.execute("SELECT COUNT(*) FROM invoices").fetchone()[0]==0


def test_cdf_pdf_rejects_unapproved_rate_override(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR",str(tmp_path))
    client=create_app().test_client()
    client.post("/setup",data={"_csrf":_token(client,"/setup"),"password":"TestPassword2026!","confirm":"TestPassword2026!"})
    client.post("/login",data={"_csrf":_token(client,"/login"),"password":"TestPassword2026!"})
    invoice=_create_synthetic_combined_invoice(client,'0')
    response=client.get(f"/invoices/{invoice['id']}/pdf?currency=CDF&rate=9999")
    assert response.status_code==400
    assert "paramètre libre" in html.unescape(response.get_data(as_text=True))


def _create_synthetic_combined_invoice(client, water_m3, *, water_tariff=False):
    _approve_default_electricity()
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,customer_type,water_enabled,water_connection_status) "
            "VALUES('SEL-C-FX-TEST','Client FX TEST','Kolwezi','','RESIDENTIAL',1,?)",
            ("CONNECTED" if float(water_m3) > 0 else "UNKNOWN",)
        )
        if water_tariff:
            connection.execute(
                "INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) "
                "VALUES('WATER-FX-TEST','RESIDENTIAL','M3','0.40','USD','0','2026-01-01',1,'WATER','TEST fixture only',1)"
            )
            connection.execute(
                "INSERT INTO tax_profiles(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
                "VALUES('WATER-TAX-FX-TEST','Taxe eau TEST','0','NONE','0','2026-01-01',1,'WATER','TEST fixture only',1)"
            )
        connection.commit()
    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"), "client_id": "1",
        "billing_period": "2026-10", "consumption_kwh": "100",
        "water_m3": str(water_m3), "water_zero_confirmed": "yes", "due_date": "2026-10-31",
    })
    assert response.status_code == 200
    preview = response.get_data(as_text=True)
    confirm_token = re.search(r'name="_csrf" value="([^"]+)"', preview).group(1)
    created = client.post("/invoices/new/confirm", data={"_csrf": confirm_token})
    assert created.status_code == 302
    with db.connect() as connection:
        return connection.execute("SELECT id,calc_snapshot FROM invoices").fetchone()


def test_cdf_pdf_route_converts_each_combined_service_and_unit_price_once(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    invoice = _create_synthetic_combined_invoice(client, "0")
    original_snapshot = invoice["calc_snapshot"]

    pdf = client.get(f"/invoices/{invoice['id']}/pdf?currency=CDF")
    text = " ".join((page.extract_text() or "") for page in PdfReader(BytesIO(pdf.data)).pages)
    normalized = re.sub(r"\s+", " ", text)

    assert pdf.status_code == 200 and pdf.data.startswith(b"%PDF-")
    assert re.search(r"22\s*420,00 CDF", normalized)  # electricity subtotal
    assert re.search(r"3\s*587,20 CDF", normalized)  # electricity VAT
    assert re.search(r"26\s*007,20 CDF", normalized)  # invoice and service total
    assert re.search(r"224,20 CDF/KWH", normalized)  # converted unit price
    assert "0 M3" in normalized and "0,00 CDF" in normalized
    assert "TVA eau" not in normalized and "0,40 USD/M3" not in normalized
    with db.connect() as connection:
        persisted = connection.execute("SELECT calc_snapshot FROM invoices WHERE id=?", (invoice["id"],)).fetchone()[0]
    assert persisted == original_snapshot


def test_cdf_pdf_route_reconciles_water_positive_and_net_payment(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={"_csrf": _token(client, "/setup"), "password": "TestPassword2026!", "confirm": "TestPassword2026!"})
    client.post("/login", data={"_csrf": _token(client, "/login"), "password": "TestPassword2026!"})
    invoice = _create_synthetic_combined_invoice(client, "2", water_tariff=True)
    payment_token = _token(client, f"/invoices/{invoice['id']}")
    paid = client.post(f"/invoices/{invoice['id']}/payments", data={
        "_csrf": payment_token, "amount": "1.00", "method": "Espèces", "paid_on": "2026-10-20",
    })
    assert paid.status_code == 302
    reverse_token = _token(client, f"/invoices/{invoice['id']}")
    reversed_payment = client.post(f"/invoices/{invoice['id']}/payments/1/reverse", data={
        "_csrf": reverse_token, "amount": "0.10", "reason": "Correction test FX", "reversed_on": "2026-10-21",
    })
    assert reversed_payment.status_code == 302

    pdf = client.get(f"/invoices/{invoice['id']}/pdf?currency=CDF")
    text = " ".join((page.extract_text() or "") for page in PdfReader(BytesIO(pdf.data)).pages)
    normalized = re.sub(r"\s+", " ", text)

    assert re.search(r"1\s*888,00 CDF", normalized)  # water subtotal and total
    assert re.search(r"27\s*895,20 CDF", normalized)  # combined document total
    assert re.search(r"2\s*124,00 CDF", normalized)  # net payment after reversal
    assert re.search(r"25\s*771,20 CDF", normalized)  # reconciled balance
