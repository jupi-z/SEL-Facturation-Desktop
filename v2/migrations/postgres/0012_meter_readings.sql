ALTER TABLE meter_readings
    ADD COLUMN correction_reason TEXT,
    ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE UNIQUE INDEX one_original_meter_reading_per_period
    ON meter_readings(meter_id, period)
    WHERE corrected_from_id IS NULL;

CREATE UNIQUE INDEX one_correction_per_meter_reading
    ON meter_readings(corrected_from_id)
    WHERE corrected_from_id IS NOT NULL;

CREATE UNIQUE INDEX meter_serial_per_service_account
    ON meters(service_account_id, serial);

CREATE INDEX meter_readings_history_idx
    ON meter_readings(meter_id, period, created_at, id);
