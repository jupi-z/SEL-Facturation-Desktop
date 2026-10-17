import db
from config_service import active_customer_types, billing_configuration


def test_new_database_seeds_official_sel_tariffs_and_settings(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        settings = db.settings(connection)
        plans = {row["customer_type"]: dict(row) for row in connection.execute("SELECT * FROM tariff_plans")}
    assert settings["vat_rate"] == "16"
    assert settings["local_rate"] == "0"
    assert settings["currency"] == "USD"
    assert settings["exchange_rate_usd_cdf"] == "2360"
    assert settings["company_name"] == "SOCIÉTÉ D’ÉNERGIE DU LUALABA"
    assert settings["tagline"] == "S.E.L. SASU"
    assert settings["address"] == "225, Boulevard LD Kabila, C. Manika, Kolwezi, Lualaba – RDC"
    assert settings["phone"] == "+243 902 190 779"
    assert settings["rccm"] == "CD/KVZ/RCCM/25-B-00736"
    assert settings["nif"] == "A2620672Z"
    assert settings["email"] == ""
    assert settings["website"] == ""
    assert settings["id_nat"] == ""
    assert plans["RESIDENTIAL"]["unit_price"] == "0.095"
    assert plans["COMMERCIAL"]["unit_price"] == "0.12"


def test_new_database_seeds_typed_configuration_catalogs(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        types = active_customer_types(connection)
        config = billing_configuration(connection, "2026-10-01")

    assert {item["code"] for item in types} == {"RESIDENTIAL", "COMMERCIAL"}
    assert config["currency"]["code"] == "USD"
    assert config["tax"]["code"] == "SEL_STANDARD"
    assert config["tax"]["vat_rate"] == "16"
    assert config["tax"]["local_rate"] == "0"
    assert config["exchange_rate"]["rate"] == "2360"


def test_new_customer_type_can_receive_a_tariff_without_code_change(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO customer_types(code,label,active,sort_order) VALUES(?,?,1,?)",
            ("INDUSTRIAL", "Industriel", 3),
        )
        connection.execute(
            "INSERT INTO tariff_plans(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active) "
            "VALUES(?,?,?,?,?,?,?,1)",
            ("INDUSTRIAL-2026", "INDUSTRIAL", "KWH", "0.15", "USD", "16", "2026-01-01"),
        )
        connection.commit()
        plan = db.tariff_plan_for(connection, "INDUSTRIAL", "2026-10-01")

    assert plan["unit_price"] == "0.15"


def test_tariff_lookup_rejects_unknown_type(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        try:
            db.tariff_plan_for(connection, "UNKNOWN", "2026-10-01")
        except ValueError as exc:
            assert "type" in str(exc).lower()
        else:
            raise AssertionError("unknown customer type was accepted")


def test_tariff_lookup_error_identifies_type_and_date(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        try:
            db.tariff_plan_for(connection, "RESIDENTIAL", "2025-12-01")
        except ValueError as exc:
            message = str(exc)
        else:
            raise AssertionError("missing tariff was accepted")

    assert "RESIDENTIAL" in message
    assert "2025-12-01" in message


def test_tariff_lookup_keeps_an_effective_historical_version_queryable(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute("UPDATE tariff_plans SET active=0 WHERE customer_type='RESIDENTIAL'")
        connection.commit()
        plan = db.tariff_plan_for(connection, "RESIDENTIAL", "2026-10-01")

    assert plan["unit_price"] == "0.095"


def test_tariff_settings_can_be_saved_without_resubmitting_identity(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute("UPDATE settings SET value='SEL' WHERE key='company_name'")
        connection.execute("UPDATE settings SET value='Kolwezi' WHERE key='address'")
        connection.commit()
        # This mirrors the modal: tariff fields only, no identity fields.
        connection.execute("UPDATE settings SET value='1' WHERE key='configured'")
        connection.commit()
    with db.connect() as connection:
        assert db.settings(connection)["company_name"] == "SEL"
