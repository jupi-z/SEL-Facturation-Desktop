CREATE TABLE invoice_voids (
    id TEXT PRIMARY KEY,
    invoice_id TEXT NOT NULL UNIQUE REFERENCES invoices(id),
    reason TEXT NOT NULL,
    actor_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL
);

CREATE INDEX invoice_voids_invoice_idx ON invoice_voids(invoice_id);
