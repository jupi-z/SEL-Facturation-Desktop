ALTER TABLE service_accounts
    ADD COLUMN revision BIGINT NOT NULL DEFAULT 1 CHECK (revision >= 1);
