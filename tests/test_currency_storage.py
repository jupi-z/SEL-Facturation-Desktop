from datetime import date

import db


def test_initialize_seeds_supported_currency_catalog_without_changing_legacy_settings(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()

    with db.connect() as connection:
        currencies = db.currency_catalog(connection)
        assert [row["code"] for row in currencies] == ["CDF", "USD"]
        assert db.settings(connection)["currency"] == "CDF"


def test_exchange_rate_history_returns_latest_rate_on_or_before_issue_date(monkeypatch, tmp_path):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()

    with db.connect() as connection:
        db.save_exchange_rate(connection, "CDF", "USD", "2850", "2026-10-01", "SEL")
        db.save_exchange_rate(connection, "CDF", "USD", "2900", "2026-10-15", "SEL")
        current = db.exchange_rate_for(connection, "CDF", "USD", date.fromisoformat("2026-10-15"))
        previous = db.exchange_rate_for(connection, "CDF", "USD", date.fromisoformat("2026-10-10"))

    assert current["rate"] == "2900"
    assert previous["rate"] == "2850"
