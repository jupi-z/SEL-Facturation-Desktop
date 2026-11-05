CREATE TABLE credit_notes (
    id TEXT PRIMARY KEY,
    receivable_id TEXT NOT NULL REFERENCES receivables(id),
    amount TEXT NOT NULL CHECK(CAST(amount AS NUMERIC) > 0),
    reason TEXT NOT NULL,
    -- The actor may belong to another authorized workstation and may not be
    -- cached in this offline database yet; the signed sync payload remains the
    -- source of audit identity.
    initiated_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX credit_notes_receivable_idx ON credit_notes(receivable_id, created_at);
