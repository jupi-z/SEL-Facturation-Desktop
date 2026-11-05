CREATE TABLE invoice_voids (
    id UUID PRIMARY KEY,
    invoice_id UUID NOT NULL UNIQUE REFERENCES invoices(id),
    reason TEXT NOT NULL,
    actor_id UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX invoice_voids_invoice_idx ON invoice_voids(invoice_id);
