CREATE TABLE invoice_meter_readings (
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    meter_reading_id UUID NOT NULL REFERENCES meter_readings(id),
    service_code TEXT NOT NULL CHECK (service_code IN ('ELECTRICITY', 'WATER')),
    PRIMARY KEY (invoice_id, service_code),
    UNIQUE (meter_reading_id)
);

CREATE INDEX invoice_meter_readings_invoice_idx
    ON invoice_meter_readings(invoice_id);
