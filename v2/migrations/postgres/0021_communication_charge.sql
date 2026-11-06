ALTER TABLE invoice_lines
    DROP CONSTRAINT IF EXISTS invoice_lines_service_code_check;

ALTER TABLE invoice_lines
    ADD CONSTRAINT invoice_lines_service_code_check
    CHECK (service_code IS NULL OR service_code IN ('ELECTRICITY', 'WATER', 'COMMUNICATION'));

ALTER TABLE catalog_reference_versions
    DROP CONSTRAINT IF EXISTS catalog_reference_versions_reference_type_check;

ALTER TABLE catalog_reference_versions
    ADD CONSTRAINT catalog_reference_versions_reference_type_check
    CHECK (reference_type IN ('TAX_PROFILE', 'FX_RATE', 'COMMUNICATION_CHARGE'));

INSERT INTO catalog_reference_versions
    (id, reference_type, natural_key, payload, effective_from, effective_to,
     status, approval_ref)
VALUES
    ('21c00100-0000-4000-8000-000000000001', 'COMMUNICATION_CHARGE', 'USD',
     '{"label":"Ligne de communication","amount":"10","currency":"USD","taxable":true}'::jsonb,
     CURRENT_DATE, NULL, 'APPROVED', 'SEL-OWNER-CONFIRMED-COMMUNICATION-10-USD')
ON CONFLICT (id) DO NOTHING;
