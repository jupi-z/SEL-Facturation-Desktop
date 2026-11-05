CREATE TABLE credit_notes (
    id UUID PRIMARY KEY,
    receivable_id UUID NOT NULL REFERENCES receivables(id),
    amount NUMERIC(20,6) NOT NULL CHECK (amount > 0),
    reason TEXT NOT NULL,
    initiated_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX credit_notes_receivable_idx ON credit_notes(receivable_id, created_at);
