CREATE TABLE sale_lines (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(id),
    product_ref TEXT NOT NULL,
    quantity TEXT NOT NULL,
    unit_price TEXT NOT NULL,
    tax_snapshot_json TEXT NOT NULL,
    total TEXT NOT NULL,
    UNIQUE(sale_id, product_ref)
);
