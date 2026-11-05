CREATE TABLE offline_grant_renewals (
    id UUID PRIMARY KEY,
    grant_id UUID NOT NULL REFERENCES offline_grants(id),
    device_id UUID NOT NULL REFERENCES devices(id),
    previous_issued_at TIMESTAMPTZ NOT NULL,
    previous_expires_at TIMESTAMPTZ NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    signature TEXT NOT NULL,
    renewed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (expires_at > issued_at)
);

CREATE INDEX offline_grant_renewals_grant_idx
    ON offline_grant_renewals (grant_id, renewed_at DESC);
