import re

import db
from app import create_app


def _token(client, path):
    html = client.get(path).get_data(as_text=True)
    return re.search(r'name="_csrf" value="([^"]+)"', html).group(1)


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


def _enable_billing():
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.execute("UPDATE tariff_plans SET confirmed=1,source='TEST fixture approval'")
        connection.execute("UPDATE tax_profiles SET confirmed=1,source='TEST fixture approval'")
        connection.commit()


def _create_client(client, name="Client test"):
    response = client.post("/clients", data={
        "_csrf": _token(client, "/clients"),
        "name": name,
        "address": "Kolwezi",
        "customer_type": "RESIDENTIAL",
        "billing_frequency": "MONTHLY",
        "contract_number": "C-TEST",
        "meter_number": "",
        "coefficient": "1",
        "center": "KOLWEZI",
        "service_point": "KOLWEZI VILLE",
        "nif": "",
        "phone": "",
    })
    assert response.status_code == 302


def _create_invoice(client):
    preview = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"),
        "client_id": "1",
        "billing_period": "2026-10",
        "consumption_kwh": "100",
        "water_m3": "0",
        "water_zero_confirmed": "yes",
        "due_date": "2026-10-31",
        "note": "Essai",
    })
    assert preview.status_code == 200
    created = client.post("/invoices/new/confirm", data={"_csrf": _token(client, "/invoices/new")})
    assert created.status_code == 302
    return 1


def test_new_invoice_preserves_invalid_submission_after_refresh(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _enable_billing()
    _create_client(client)

    response = client.post("/invoices/new", data={
        "_csrf": _token(client, "/invoices/new"),
        "client_id": "1",
        "billing_period": "2026-11",
        "consumption_kwh": "pas-un-nombre",
        "water_m3": "0",
        "water_zero_confirmed": "yes",
        "due_date": "2026-11-30",
        "note": "Brouillon conservé",
    })

    assert response.status_code == 200
    refreshed = client.get("/invoices/new").get_data(as_text=True)
    assert 'value="2026-11"' in refreshed
    assert 'value="pas-un-nombre"' in refreshed
    assert 'value="2026-11-30"' in refreshed
    assert 'value="Brouillon conservé"' in refreshed
    assert 'value="1" data-water-enabled="true" selected' in refreshed


def test_settings_preserves_invalid_identity_after_refresh_and_keeps_tab(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    response = client.post("/settings", data={
        "_csrf": _token(client, "/settings"),
        "section": "identity",
        "return_tab": "identity",
        "company_name": "SEL BROUILLON",
        "tagline": "Version de test",
        "address": "",
        "phone": "+243 000",
        "email": "test@sel.cd",
        "website": "",
        "rccm": "RCCM-TEST",
        "nif": "NIF-TEST",
        "id_nat": "",
    })

    assert response.status_code == 200
    refreshed = client.get("/settings").get_data(as_text=True)
    assert 'value="SEL BROUILLON"' in refreshed
    assert 'value="Version de test"' in refreshed
    assert 'id="settings-tab-identity"' in refreshed and 'aria-selected="true"' in refreshed


def test_client_create_reopens_preserved_draft_after_refresh(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    response = client.post("/clients", data={
        "_csrf": _token(client, "/clients"),
        "name": "Abonné brouillon",
        "address": "",
        "customer_type": "RESIDENTIAL",
        "billing_frequency": "MONTHLY",
        "center": "KOLWEZI",
        "service_point": "KOLWEZI VILLE",
    })

    assert response.status_code == 200
    refreshed = client.get("/clients").get_data(as_text=True)
    assert 'data-modal-autopen="true"' in refreshed
    assert 'value="Abonné brouillon"' in refreshed


def test_client_edit_preserves_invalid_submission_after_refresh(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _create_client(client, name="Ancien nom")
    response = client.post("/clients/1/edit", data={
        "_csrf": _token(client, "/clients/1/edit"),
        "name": "Nouveau nom brouillon",
        "address": "",
        "category": "",
        "client_type": "",
        "customer_type": "RESIDENTIAL",
        "billing_frequency": "MONTHLY",
        "contract_number": "C-EDIT",
        "meter_number": "",
        "coefficient": "1",
        "center": "KOLWEZI",
        "service_point": "KOLWEZI VILLE",
        "nif": "",
        "phone": "",
    })

    assert response.status_code == 200
    refreshed = client.get("/clients/1/edit").get_data(as_text=True)
    assert 'value="Nouveau nom brouillon"' in refreshed
    assert 'value="C-EDIT"' in refreshed


def test_payment_and_void_forms_preserve_invalid_submissions_after_refresh(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _enable_billing()
    _create_client(client)
    invoice_id = _create_invoice(client)

    payment = client.post(f"/invoices/{invoice_id}/payments", data={
        "_csrf": _token(client, f"/invoices/{invoice_id}"),
        "amount": "9999",
        "method": "Espèces",
        "paid_on": "2026-10-20",
        "reference": "Référence conservée",
    })
    assert payment.status_code == 302
    detail = client.get(f"/invoices/{invoice_id}").get_data(as_text=True)
    assert 'value="9999"' in detail
    assert 'value="2026-10-20"' in detail
    assert 'value="Référence conservée"' in detail

    void = client.post(f"/invoices/{invoice_id}/void", data={
        "_csrf": _token(client, f"/invoices/{invoice_id}"),
        "reason": "x",
    })
    assert void.status_code == 302
    detail = client.get(f"/invoices/{invoice_id}").get_data(as_text=True)
    assert 'value="x"' in detail


def test_all_business_forms_have_scoped_draft_keys_without_password_drafts(tmp_path, monkeypatch):
    client = _authenticated_client(tmp_path, monkeypatch)
    _enable_billing()
    _create_client(client)
    invoice_id = _create_invoice(client)

    new_invoice = client.get('/invoices/new').get_data(as_text=True)
    clients = client.get('/clients').get_data(as_text=True)
    edit = client.get('/clients/1/edit').get_data(as_text=True)
    settings = client.get('/settings').get_data(as_text=True)
    detail = client.get(f'/invoices/{invoice_id}').get_data(as_text=True)

    assert 'data-draft-key="invoice-new"' in new_invoice
    assert 'data-draft-key="client-create"' in clients
    assert 'data-draft-key="client-edit-1"' in edit
    assert 'data-draft-key="settings-identity"' in settings
    assert 'data-draft-key="settings-tariffs"' in settings
    assert 'data-draft-key="settings-customer-type"' in settings
    assert 'data-draft-key="settings-operations"' in settings
    assert f'data-draft-key="payment-{invoice_id}"' in detail
    assert f'data-draft-key="void-{invoice_id}"' in detail

    login = client.get('/logout', follow_redirects=False)
    assert login.status_code == 405
    auth_page = create_app().test_client().get('/login').get_data(as_text=True)
    assert 'data-draft-key' not in auth_page


def test_draft_script_uses_session_storage_and_excludes_passwords():
    script = open('static/desktop.js', encoding='utf-8').read()
    assert 'window.sessionStorage' in script
    assert "new Set(['password'" in script
    assert "field.name !== '_csrf'" in script
    assert "type !== 'hidden'" in script
    assert "form.dataset.draftServer === 'true'" in script
