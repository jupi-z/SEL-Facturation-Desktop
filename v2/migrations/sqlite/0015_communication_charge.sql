CREATE TABLE communication_charge_versions (
    id TEXT PRIMARY KEY,
    currency TEXT NOT NULL,
    label TEXT NOT NULL,
    amount TEXT NOT NULL CHECK (CAST(amount AS NUMERIC) >= 0),
    effective_from TEXT NOT NULL,
    effective_to TEXT,
    status TEXT NOT NULL CHECK (status IN ('APPROVED', 'REVOKED')),
    approval_ref TEXT
);

CREATE INDEX communication_charge_local_period_idx
    ON communication_charge_versions(currency, status, effective_from, effective_to);
