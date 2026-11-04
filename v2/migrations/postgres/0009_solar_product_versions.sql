ALTER TABLE products DROP CONSTRAINT products_sku_key;

ALTER TABLE products
    ADD COLUMN effective_from DATE NOT NULL DEFAULT DATE '1970-01-01',
    ADD COLUMN effective_to DATE,
    ADD COLUMN status TEXT NOT NULL DEFAULT 'APPROVED'
        CHECK (status IN ('DRAFT', 'APPROVED', 'REVOKED')),
    ADD COLUMN approval_ref TEXT,
    ADD COLUMN approved_by UUID REFERENCES users(id);

UPDATE products SET status = CASE WHEN active THEN 'APPROVED' ELSE 'REVOKED' END;

CREATE UNIQUE INDEX products_sku_effective_from_unique
    ON products(sku, effective_from);
CREATE INDEX products_active_period_idx
    ON products(product_type, currency, status, active, effective_from, effective_to);
