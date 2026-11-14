-- One centrally approved VAT policy is shared by utility and solar sales.
-- Legacy service-specific tax rows remain as history but are no longer read
-- by commercial calculation paths or accepted for new drafts.
ALTER TABLE tax_profile_versions
    DROP CONSTRAINT tax_profile_versions_service_code_check;

ALTER TABLE tax_profile_versions
    ADD CONSTRAINT tax_profile_versions_service_code_check
    CHECK (service_code IN ('ELECTRICITY', 'WATER', 'SOLAR_KIT', 'VAT'));

-- Append-only trace tying an erroneous solar sale to its accounting
-- correction. The old sale snapshots and payment records are never rewritten.
CREATE TABLE solar_sale_corrections (
    id UUID PRIMARY KEY,
    old_sale_id UUID NOT NULL UNIQUE REFERENCES sales(id),
    replacement_sale_id UUID NOT NULL UNIQUE REFERENCES sales(id),
    old_receivable_id UUID NOT NULL UNIQUE REFERENCES receivables(id),
    replacement_receivable_id UUID NOT NULL UNIQUE REFERENCES receivables(id),
    actor_id UUID NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    corrected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    correction_snapshot JSONB NOT NULL
);

CREATE INDEX solar_sale_corrections_replacement_idx
    ON solar_sale_corrections(replacement_sale_id);
