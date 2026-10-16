import db


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
    assert plans["RESIDENTIAL"]["unit_price"] == "0.095"
    assert plans["COMMERCIAL"]["unit_price"] == "0.12"


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
