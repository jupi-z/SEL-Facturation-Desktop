CREATE TABLE catalog_reference_versions (
    id UUID PRIMARY KEY,
    reference_type TEXT NOT NULL CHECK (reference_type IN ('TAX_PROFILE', 'FX_RATE')),
    natural_key TEXT NOT NULL,
    payload JSONB NOT NULL,
    effective_from DATE NOT NULL,
    effective_to DATE,
    status TEXT NOT NULL CHECK (status IN ('DRAFT', 'APPROVED', 'REVOKED')),
    approval_ref TEXT,
    approved_by UUID REFERENCES users(id),
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (effective_to IS NULL OR effective_to >= effective_from)
);

CREATE INDEX catalog_reference_history_idx
    ON catalog_reference_versions(reference_type, natural_key, effective_from DESC, id);

INSERT INTO catalog_reference_versions
    (id, reference_type, natural_key, payload, effective_from, effective_to,
     status, approval_ref, approved_by, created_by)
SELECT id,
       'TAX_PROFILE',
       upper(service_code) || '|' || upper(jurisdiction) || '|' || upper(base),
       jsonb_build_object(
           'service_code', service_code,
           'jurisdiction', jurisdiction,
           'base', base,
           'rate', rate::text,
           'source', source
       ),
       effective_from, effective_to, status, approval_ref, approved_by, NULL
FROM tax_profile_versions;

INSERT INTO catalog_reference_versions
    (id, reference_type, natural_key, payload, effective_from, effective_to,
     status, approval_ref, approved_by, created_by)
SELECT id,
       'FX_RATE',
       upper(from_currency) || '|' || upper(to_currency),
       jsonb_build_object(
           'from_currency', from_currency,
           'to_currency', to_currency,
           'value', value::text,
           'source', source
       ),
       effective_from, effective_to,
       CASE WHEN approved THEN status ELSE 'DRAFT' END,
       approval_ref, approved_by, NULL
FROM fx_rate_versions;
