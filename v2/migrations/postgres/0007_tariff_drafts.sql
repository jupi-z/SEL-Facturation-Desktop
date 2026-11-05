ALTER TABLE tariff_versions ALTER COLUMN approval_ref DROP NOT NULL;
ALTER TABLE tariff_versions
    ADD CONSTRAINT tariff_approved_requires_approval_ref
    CHECK (status <> 'APPROVED' OR length(trim(approval_ref)) > 0) NOT VALID;
