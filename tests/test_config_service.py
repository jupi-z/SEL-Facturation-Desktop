import db

from config_service import active_customer_types, active_payment_methods, billing_configuration


def test_billing_configuration_is_resolved_by_effective_date(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO exchange_rates(base_currency,quote_currency,rate,effective_on,source) VALUES(?,?,?,?,?)",
            ("USD", "CDF", "2500", "2027-01-01", "SEL"),
        )
        connection.commit()
        before = billing_configuration(connection, "2026-12-01")
        after = billing_configuration(connection, "2027-01-01")

    assert before["exchange_rate"]["rate"] == "2360"
    assert after["exchange_rate"]["rate"] == "2500"


def test_inactive_customer_type_is_not_offered(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute("UPDATE customer_types SET active=0 WHERE code='COMMERCIAL'")
        connection.commit()
        types = active_customer_types(connection)

    assert [item["code"] for item in types] == ["RESIDENTIAL"]


def test_exchange_rate_saved_in_settings_is_available_from_effective_rate_catalog(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO exchange_rates(base_currency,quote_currency,rate,effective_on,source) VALUES(?,?,?,?,?)",
            ("USD", "CDF", "2450", "2026-11-01", "SEL"),
        )
        connection.commit()
        config = billing_configuration(connection, "2026-11-01")

    assert config["exchange_rate"]["rate"] == "2450"


def test_payment_methods_are_read_from_configuration(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute("UPDATE payment_methods SET active=0 WHERE code='OTHER'")
        connection.commit()
        methods = active_payment_methods(connection)

    assert "OTHER" not in {item["code"] for item in methods}
    assert "CASH" in {item["code"] for item in methods}
