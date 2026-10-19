import db
import sqlite3

from config_service import active_customer_types, active_payment_methods, billing_configuration


def test_initialize_migrates_currency_catalog_from_previous_schema(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    database = tmp_path / "sel.sqlite3"
    with sqlite3.connect(database) as connection:
        connection.executescript(
            """
            CREATE TABLE currency_catalog (
                code TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                symbol TEXT NOT NULL,
                decimal_digits INTEGER NOT NULL DEFAULT 2,
                active INTEGER NOT NULL DEFAULT 1,
                display_order INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE currency_rates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                base_currency TEXT NOT NULL,
                quote_currency TEXT NOT NULL,
                rate TEXT NOT NULL,
                effective_on TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(base_currency, quote_currency, effective_on)
            );
            INSERT INTO currency_catalog(code,name,symbol,decimal_digits,active,display_order)
            VALUES ('USD','Dollar américain','$',2,1,20),
                   ('CDF','Franc congolais','CDF',2,1,10);
            INSERT INTO currency_rates(base_currency,quote_currency,rate,effective_on,source)
            VALUES ('USD','CDF','2500','2026-10-01','ancienne configuration');
            """
        )

    db.initialize()
    with db.connect() as connection:
        columns = {row["name"] for row in connection.execute("PRAGMA table_info(currency_catalog)")}
        currencies = connection.execute(
            "SELECT code,label,decimals,sort_order FROM currency_catalog ORDER BY sort_order"
        ).fetchall()
        rate = connection.execute(
            "SELECT rate,source FROM exchange_rates WHERE effective_on='2026-10-01'"
        ).fetchone()

    assert {"label", "decimals", "sort_order"}.issubset(columns)
    assert [(row["code"], row["label"]) for row in currencies] == [
        ("CDF", "Franc congolais"),
        ("USD", "Dollar américain"),
    ]
    assert (rate["rate"], rate["source"]) == ("2500", "ancienne configuration")


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
