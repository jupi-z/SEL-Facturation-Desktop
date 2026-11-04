CREATE TABLE meters (
    id TEXT PRIMARY KEY,
    service_account_id TEXT NOT NULL REFERENCES service_accounts(id),
    serial TEXT NOT NULL,
    unit TEXT NOT NULL CHECK (unit IN ('KWH', 'M3')),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    UNIQUE (service_account_id, serial)
);

CREATE TABLE meter_readings (
    id TEXT PRIMARY KEY,
    meter_id TEXT NOT NULL REFERENCES meters(id),
    period TEXT NOT NULL,
    previous TEXT NOT NULL,
    current TEXT NOT NULL,
    measured_quantity TEXT NOT NULL,
    source TEXT NOT NULL,
    corrected_from_id TEXT REFERENCES meter_readings(id),
    correction_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (CAST(previous AS NUMERIC) >= 0),
    CHECK (CAST(current AS NUMERIC) >= CAST(previous AS NUMERIC)),
    CHECK (CAST(measured_quantity AS NUMERIC) >= 0)
);

CREATE UNIQUE INDEX one_original_meter_reading_per_period
    ON meter_readings(meter_id, period)
    WHERE corrected_from_id IS NULL;

CREATE INDEX meter_readings_history_idx
    ON meter_readings(meter_id, period, created_at, id);
