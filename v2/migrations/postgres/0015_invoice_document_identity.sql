ALTER TABLE invoices
    ADD COLUMN document_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb;
