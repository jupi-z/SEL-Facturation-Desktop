import db
from config_service import tax_profile_for


def test_service_migration_scopes_existing_electricity_and_does_not_seed_water(
    tmp_path, monkeypatch
):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    db.initialize()

    with db.connect() as connection:
        tariff_columns = {
            row["name"] for row in connection.execute("PRAGMA table_info(tariff_plans)")
        }
        tax_columns = {
            row["name"] for row in connection.execute("PRAGMA table_info(tax_profiles)")
        }
        tariffs = connection.execute("SELECT service_code, unit FROM tariff_plans").fetchall()
        taxes = connection.execute("SELECT service_code, confirmed, source FROM tax_profiles").fetchall()
        water_tariffs = connection.execute(
            "SELECT COUNT(*) FROM tariff_plans WHERE service_code='WATER'"
        ).fetchone()[0]
        water_taxes = connection.execute(
            "SELECT COUNT(*) FROM tax_profiles WHERE service_code='WATER'"
        ).fetchone()[0]

    assert {"service_code", "source", "confirmed"}.issubset(tariff_columns)
    assert {"service_code", "source", "confirmed"}.issubset(tax_columns)
    assert {(row["service_code"], row["unit"]) for row in tariffs} == {("ELECTRICITY", "KWH")}
    assert {(row["service_code"], row["confirmed"]) for row in taxes} == {("ELECTRICITY", 1)}
    assert not any(row["source"] == "" for row in taxes)
    assert water_tariffs == water_taxes == 0


def test_migration_enables_water_for_existing_clients_and_future_inserts(
    tmp_path, monkeypatch
):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        columns = {row["name"] for row in connection.execute("PRAGMA table_info(clients)")}
        connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,water_enabled) VALUES('SEL-C-OLD','Client ancien','Kolwezi','',0)"
        )
        connection.commit()
    db.initialize()
    with db.connect() as connection:
        water_enabled = connection.execute(
            "SELECT water_enabled FROM clients WHERE code='SEL-C-OLD'"
        ).fetchone()[0]
        connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,water_enabled) VALUES('SEL-C-NEW','Nouveau','Kolwezi','',0)"
        )
        connection.commit()
        new_water_enabled = connection.execute(
            "SELECT water_enabled FROM clients WHERE code='SEL-C-NEW'"
        ).fetchone()[0]

    assert "water_enabled" in columns
    assert water_enabled == new_water_enabled == 1


def test_tariff_and_tax_lookup_are_service_scoped(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO tariff_plans "
            "(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('WATER-SEL-2027','RESIDENTIAL','M3','0.40','USD','0','2027-01-01',1,'WATER','SEL approval 2026-12-20',1)"
        )
        connection.execute(
            "INSERT INTO tax_profiles "
            "(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('WATER-EXEMPT','Eau - exempté confirmé','0','NONE','0','2027-01-01',1,'WATER','SEL approval 2026-12-20',1)"
        )
        connection.commit()

        water_plan = db.tariff_plan_for(
            connection, "RESIDENTIAL", "2027-01-01", service_code="WATER"
        )
        water_tax = tax_profile_for(
            connection, "2027-01-01", code="WATER-EXEMPT",
            service_code="WATER", require_confirmed=True
        )

        try:
            db.tariff_plan_for(connection, "RESIDENTIAL", "2027-01-01", service_code="GAS")
        except ValueError as exc:
            unknown_service = str(exc)
        else:
            raise AssertionError("unknown service was accepted")

    assert water_plan["unit"] == "M3"
    assert water_tax["vat_rate"] == "0"
    assert "service" in unknown_service.lower()


def test_unconfirmed_water_tax_is_not_eligible_for_billing(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO tax_profiles "
            "(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('WATER-PENDING','Eau à confirmer','0','NONE','0','2027-01-01',1,'WATER','',0)"
        )
        connection.commit()
        try:
            tax_profile_for(
                connection, "2027-01-01", code="WATER-PENDING",
                service_code="WATER", require_confirmed=True
            )
        except ValueError as exc:
            message = str(exc)
        else:
            raise AssertionError("unconfirmed water tax was accepted")

    assert "confirm" in message.lower()
