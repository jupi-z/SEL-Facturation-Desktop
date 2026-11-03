CREATE TABLE import_batches (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    source_system TEXT NOT NULL,
    source_schema_version INTEGER NOT NULL,
    source_file_sha256 TEXT NOT NULL,
    manifest_json JSONB NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('STARTED', 'APPLIED', 'FAILED')),
    error_detail TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    applied_at TIMESTAMPTZ,
    UNIQUE (organization_id, source_system, source_file_sha256)
);

CREATE TABLE legacy_id_map (
    source_system TEXT NOT NULL,
    entity TEXT NOT NULL,
    legacy_key TEXT NOT NULL,
    target_uuid UUID NOT NULL,
    import_batch_id UUID NOT NULL REFERENCES import_batches(id),
    PRIMARY KEY (source_system, entity, legacy_key),
    UNIQUE (import_batch_id, entity, target_uuid)
);

CREATE INDEX import_batches_org_idx ON import_batches(organization_id, created_at DESC);
