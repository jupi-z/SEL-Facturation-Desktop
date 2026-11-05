ALTER TABLE tax_profile_versions ADD COLUMN approval_ref TEXT;
ALTER TABLE tax_profile_versions ADD COLUMN approved_by TEXT;
CREATE INDEX tax_profiles_local_period_idx
    ON tax_profile_versions(service_code, jurisdiction, base, status, effective_from, effective_to);

ALTER TABLE fx_rate_versions ADD COLUMN effective_to TEXT;
ALTER TABLE fx_rate_versions ADD COLUMN approval_ref TEXT;
ALTER TABLE fx_rate_versions ADD COLUMN approved_by TEXT;
ALTER TABLE fx_rate_versions ADD COLUMN status TEXT NOT NULL DEFAULT 'DRAFT';
UPDATE fx_rate_versions
SET status = CASE WHEN approved = 1 THEN 'APPROVED' ELSE 'DRAFT' END;
CREATE INDEX fx_rates_local_period_idx
    ON fx_rate_versions(from_currency, to_currency, status, effective_from, effective_to);
