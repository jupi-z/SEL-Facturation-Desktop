-- Additive only: historical references are retained.
ALTER TABLE payments ADD COLUMN document_reference TEXT;
ALTER TABLE payments ADD COLUMN issued_at TIMESTAMPTZ;
ALTER TABLE sales ADD COLUMN document_reference TEXT;
CREATE UNIQUE INDEX payments_document_reference_unique ON payments(document_reference) WHERE document_reference IS NOT NULL;
CREATE UNIQUE INDEX sales_document_reference_unique ON sales(document_reference) WHERE document_reference IS NOT NULL;
