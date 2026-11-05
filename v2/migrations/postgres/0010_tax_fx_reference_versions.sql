ALTER TABLE tax_profile_versions
    ADD COLUMN approval_ref TEXT,
    ADD COLUMN approved_by UUID REFERENCES users(id);

ALTER TABLE fx_rate_versions
    ADD COLUMN effective_to DATE,
    ADD COLUMN approval_ref TEXT,
    ADD COLUMN approved_by UUID REFERENCES users(id),
    ADD COLUMN status TEXT NOT NULL DEFAULT 'DRAFT'
        CHECK (status IN ('DRAFT', 'APPROVED', 'REVOKED'));

UPDATE fx_rate_versions
SET status = CASE WHEN approved THEN 'APPROVED' ELSE 'DRAFT' END;

CREATE INDEX tax_profiles_period_idx
    ON tax_profile_versions(service_code, jurisdiction, base, status, effective_from, effective_to);
CREATE INDEX fx_rates_period_idx
    ON fx_rate_versions(from_currency, to_currency, status, effective_from, effective_to);
