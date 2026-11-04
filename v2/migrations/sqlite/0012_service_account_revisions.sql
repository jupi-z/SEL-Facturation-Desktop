ALTER TABLE service_accounts
    ADD COLUMN revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1);
