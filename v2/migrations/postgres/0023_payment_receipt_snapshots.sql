-- Keep the financial context printed on a payment receipt stable when it is reprinted.
ALTER TABLE payments ADD COLUMN receipt_snapshot JSONB;
