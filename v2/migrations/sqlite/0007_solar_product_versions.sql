ALTER TABLE products RENAME TO products_before_versions;

CREATE TABLE products (
    id TEXT PRIMARY KEY,
    sku TEXT NOT NULL,
    product_type TEXT NOT NULL,
    label TEXT NOT NULL,
    price_reference TEXT NOT NULL,
    currency TEXT NOT NULL,
    active INTEGER NOT NULL,
    effective_from TEXT NOT NULL DEFAULT '1970-01-01',
    effective_to TEXT,
    status TEXT NOT NULL DEFAULT 'APPROVED'
        CHECK (status IN ('DRAFT','APPROVED','REVOKED')),
    approval_ref TEXT,
    UNIQUE(sku, effective_from)
);

INSERT INTO products
    (id, sku, product_type, label, price_reference, currency, active,
     effective_from, effective_to, status, approval_ref)
SELECT id, sku, product_type, label, price_reference, currency, active,
       '1970-01-01', NULL, CASE WHEN active = 1 THEN 'APPROVED' ELSE 'REVOKED' END,
       NULL
FROM products_before_versions;

DROP TABLE products_before_versions;
CREATE INDEX products_local_active_period_idx
    ON products(product_type, currency, status, active, effective_from, effective_to);
