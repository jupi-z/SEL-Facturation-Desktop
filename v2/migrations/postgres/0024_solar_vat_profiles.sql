-- Solar-kit sales use the versioned VAT catalogue like utility services.
-- This widens future tax configuration only; issued documents remain immutable.
ALTER TABLE tax_profile_versions
    DROP CONSTRAINT tax_profile_versions_service_code_check;

ALTER TABLE tax_profile_versions
    ADD CONSTRAINT tax_profile_versions_service_code_check
    CHECK (service_code IN ('ELECTRICITY', 'WATER', 'SOLAR_KIT'));
