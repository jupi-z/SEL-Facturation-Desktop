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
