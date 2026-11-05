ALTER TABLE invoices
    ADD COLUMN document_snapshot_json TEXT NOT NULL DEFAULT '{}';
