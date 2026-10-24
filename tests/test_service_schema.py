import db
from config_service import tax_profile_for
import pytest


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


def test_water_connection_is_preserved_and_separate_from_invoice_service_requirement(
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
        connection_statuses = [row[0] for row in connection.execute(
            "SELECT water_connection_status FROM clients ORDER BY code"
        )]
        required_triggers = connection.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name IN "
            "('clients_water_required_insert','clients_water_required_update')"
        ).fetchone()[0]

    assert "water_enabled" in columns
    assert "water_connection_status" in columns
    assert water_enabled == new_water_enabled == 0
    assert connection_statuses == ["UNKNOWN", "UNKNOWN"]
    assert required_triggers == 0


def test_upgrade_from_water_coercion_triggers_keeps_connection_unknown_when_history_was_lost(
    tmp_path, monkeypatch
):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO clients(code,name,address,meter_number,water_enabled) VALUES('SEL-C-LEGACY','Legacy','Kolwezi','',0)"
        )
        connection.executescript("""
          CREATE TRIGGER clients_water_required_insert AFTER INSERT ON clients
          WHEN NEW.water_enabled<>1 BEGIN UPDATE clients SET water_enabled=1 WHERE id=NEW.id; END;
          CREATE TRIGGER clients_water_required_update AFTER UPDATE OF water_enabled ON clients
          WHEN NEW.water_enabled<>1 BEGIN UPDATE clients SET water_enabled=1 WHERE id=NEW.id; END;
        """)
        connection.execute("UPDATE clients SET water_enabled=0 WHERE code='SEL-C-LEGACY'")
        connection.execute("ALTER TABLE clients DROP COLUMN water_connection_status")
        connection.commit()

    db.initialize()
    with db.connect() as connection:
        row=connection.execute("SELECT water_enabled,water_connection_status FROM clients WHERE code='SEL-C-LEGACY'").fetchone()
        trigger_count=connection.execute("SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'clients_water_required_%'").fetchone()[0]
    assert row['water_enabled']==1  # irrecoverable legacy value is not rewritten
    assert row['water_connection_status']=='UNKNOWN'
    assert trigger_count==0


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


def test_latest_inactive_tariff_blocks_billing_instead_of_selecting_it_or_falling_back(
    tmp_path, monkeypatch
):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "UPDATE tariff_plans SET confirmed=1 WHERE code='RESIDENTIAL-2026'"
        )
        connection.execute(
            "INSERT INTO tariff_plans "
            "(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('RESIDENTIAL-NEW-TEST','RESIDENTIAL','KWH','0.20','USD','16','2026-10-01',0,'ELECTRICITY','TEST disabled',1)"
        )
        connection.commit()

        with pytest.raises(ValueError, match="désactivé"):
            db.tariff_plan_for(
                connection, "RESIDENTIAL", "2026-10-01", require_confirmed=True
            )


def test_latest_unconfirmed_tariff_blocks_billing_without_falling_back(
    tmp_path, monkeypatch
):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "UPDATE tariff_plans SET confirmed=1 WHERE code='RESIDENTIAL-2026'"
        )
        connection.execute(
            "INSERT INTO tariff_plans "
            "(code,customer_type,unit,unit_price,currency,vat_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('RESIDENTIAL-PENDING-TEST','RESIDENTIAL','KWH','0.20','USD','16','2026-10-01',1,'ELECTRICITY','TEST pending',0)"
        )
        connection.commit()

        with pytest.raises(ValueError, match="confirmé"):
            db.tariff_plan_for(
                connection, "RESIDENTIAL", "2026-10-01", require_confirmed=True
            )


def test_newest_inactive_tax_profile_blocks_without_fallback(tmp_path, monkeypatch):
    monkeypatch.setenv("SEL_DATA_DIR", str(tmp_path))
    db.initialize()
    with db.connect() as connection:
        connection.execute(
            "INSERT INTO tax_profiles "
            "(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('ELECTRICITY-TAX-OLD-TEST','Ancien test','16','ENERGY','0','2026-01-01',1,'ELECTRICITY','TEST',1)"
        )
        connection.execute(
            "INSERT INTO tax_profiles "
            "(code,label,vat_rate,vat_base,local_rate,effective_from,active,service_code,source,confirmed) "
            "VALUES('ELECTRICITY-TAX-NEW-TEST','Nouveau inactif test','17','ENERGY','0','2026-10-01',0,'ELECTRICITY','TEST',1)"
        )
        connection.commit()

        with pytest.raises(ValueError, match="désactivé"):
            tax_profile_for(
                connection, "2026-10-01", code=None,
                service_code="ELECTRICITY", require_confirmed=True,
            )
