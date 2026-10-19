import re

import db
from app import create_app


def _token(client, path):
    html = client.get(path).get_data(as_text=True)
    match = re.search(r'name="_csrf" value="([^"]+)"', html)
    assert match
    return match.group(1)


def _authenticated_client(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    client = create_app().test_client()
    client.post("/setup", data={
        "_csrf": _token(client, "/setup"),
        "password": "TestPassword2026!",
        "confirm": "TestPassword2026!",
    })
    client.post("/login", data={
        "_csrf": _token(client, "/login"),
        "password": "TestPassword2026!",
    })
    return client


def _create_client(client, name="Client CRUD"):
    response = client.post("/clients", data={
        "_csrf": _token(client, "/clients"),
        "name": name,
        "address": "Kolwezi",
        "customer_type": "RESIDENTIAL",
        "billing_frequency": "MONTHLY",
        "contract_number": "CTR-001",
        "meter_number": "",
        "coefficient": "1",
        "center": "KOLWEZI",
        "service_point": "KOLWEZI VILLE",
        "nif": "",
        "phone": "",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        return connection.execute("SELECT id FROM clients WHERE name=?", (name,)).fetchone()[0]


def test_client_edit_persists_contract_and_all_visible_business_fields(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _create_client(client)

    html = client.get("/clients/1/edit").get_data(as_text=True)
    assert 'name="contract_number"' in html
    assert 'name="customer_type"' in html
    assert 'name="center"' in html
    assert 'name="service_point"' in html
    assert 'name="phone"' in html

    response = client.post("/clients/1/edit", data={
        "_csrf": _token(client, "/clients/1/edit"),
        "name": "Client modifié",
        "address": "Nouvelle adresse",
        "customer_type": "COMMERCIAL",
        "contract_number": "CTR-999",
        "center": "LIKASI",
        "service_point": "LIKASI CENTRE",
        "nif": "NIF-1",
        "phone": "+243900000000",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        row = connection.execute("SELECT * FROM clients WHERE id=1").fetchone()
    assert row["name"] == "Client modifié"
    assert row["contract_number"] == "CTR-999"
    assert row["customer_type"] == "COMMERCIAL"
    assert row["center"] == "LIKASI"
    assert row["service_point"] == "LIKASI CENTRE"


def test_unbilled_client_can_be_deleted_and_billed_client_can_only_be_archived(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _create_client(client, "Client à supprimer")

    response = client.post("/clients/1/delete", data={"_csrf": _token(client, "/clients/1/edit")})
    assert response.status_code == 302
    with db.connect() as connection:
        assert connection.execute("SELECT 1 FROM clients WHERE id=1").fetchone() is None

    client_id = _create_client(client, "Client à archiver")
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.commit()
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"),
        "client_id": str(client_id), "billing_period": "2026-10", "consumption_kwh": "100",
    })
    confirm = client.post("/invoices/new/confirm", data={
        "_csrf": re.search(r'name="_csrf" value="([^"]+)"', preview.get_data(as_text=True)).group(1),
    })
    assert confirm.status_code == 302

    archived = client.post(f"/clients/{client_id}/archive", data={
        "_csrf": _token(client, f"/clients/{client_id}/edit"), "reason": "Fin de contrat",
    })
    assert archived.status_code == 302
    with db.connect() as connection:
        assert connection.execute("SELECT status FROM clients WHERE id=?", (client_id,)).fetchone()[0] == "ARCHIVED"
    assert f'value="{client_id}"' not in client.get("/invoices/new").get_data(as_text=True)

    restored = client.post(f"/clients/{client_id}/restore", data={"_csrf": _token(client, "/clients")})
    assert restored.status_code == 302
    with db.connect() as connection:
        assert connection.execute("SELECT status FROM clients WHERE id=?", (client_id,)).fetchone()[0] == "ACTIVE"


def test_settings_can_update_customer_type_and_manage_currency_payment_method_and_history(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)

    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "customer_type_edit",
        "code": "COMMERCIAL", "label": "Professionnel", "active": "yes", "sort_order": "5",
    })
    assert response.status_code == 302

    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "currency",
        "code": "EUR", "label": "Euro", "decimals": "2", "active": "yes", "sort_order": "30",
    })
    assert response.status_code == 302

    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "payment_method",
        "code": "CHECK", "label": "Chèque", "reference_required": "yes", "active": "yes", "sort_order": "50",
    })
    assert response.status_code == 302

    with db.connect() as connection:
        customer_type = connection.execute("SELECT label,sort_order FROM customer_types WHERE code='COMMERCIAL'").fetchone()
        currency = connection.execute("SELECT label FROM currency_catalog WHERE code='EUR'").fetchone()
        method = connection.execute("SELECT label,reference_required FROM payment_methods WHERE code='CHECK'").fetchone()
    assert (customer_type["label"], customer_type["sort_order"]) == ("Professionnel", 5)
    assert currency["label"] == "Euro"
    assert (method["label"], method["reference_required"]) == ("Chèque", 1)

    history = client.get("/settings?tab=history").get_data(as_text=True)
    assert 'id="settings-tab-history"' in history
    assert 'id="settings-tab-history" class="settings-tab" type="button" role="tab" data-settings-tab="history" aria-controls="settings-panel-history" aria-selected="true"' in history
    assert "Professionnel" in history
    assert "Euro" in history
    assert "Chèque" in history

    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tax_profile",
        "code": "SEL_REDUCED", "label": "TVA réduite", "vat_rate": "8",
        "vat_base": "ENERGY", "local_rate": "0", "effective_from": "2027-01-01",
        "active": "yes",
    })
    assert response.status_code == 302
    with db.connect() as connection:
        profile = connection.execute("SELECT label,vat_rate,effective_from FROM tax_profiles WHERE code='SEL_REDUCED'").fetchone()
    assert (profile["label"], profile["vat_rate"], profile["effective_from"]) == ("TVA réduite", "8", "2027-01-01")
    assert "TVA réduite" in client.get("/settings?tab=catalogs").get_data(as_text=True)


def test_settings_can_create_dated_tariff_and_exchange_rate_without_rewriting_existing_history(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "tariff",
        "customer_type": "RESIDENTIAL", "unit_price": "0.10", "currency": "USD",
        "vat_rate": "16", "effective_from": "2027-01-01",
    })
    assert response.status_code == 302

    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"), "section": "exchange_rate",
        "base_currency": "USD", "quote_currency": "CDF", "rate": "2500",
        "effective_on": "2027-01-01", "source": "SEL",
    })
    assert response.status_code == 302

    with db.connect() as connection:
        tariffs = connection.execute("SELECT unit_price FROM tariff_plans WHERE customer_type='RESIDENTIAL' ORDER BY effective_from").fetchall()
        rates = connection.execute("SELECT rate FROM exchange_rates WHERE base_currency='USD' AND quote_currency='CDF' ORDER BY effective_on").fetchall()
    assert [row[0] for row in tariffs] == ["0.095", "0.10"]
    assert [row[0] for row in rates] == ["2360", "2500"]
