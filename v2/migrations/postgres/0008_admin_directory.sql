-- Preserve issued grants for audit while allowing immediate central revocation.
ALTER TABLE offline_grants ADD COLUMN revoked_at TIMESTAMPTZ;
CREATE INDEX offline_grants_active_device_idx
    ON offline_grants (device_id, expires_at)
    WHERE revoked_at IS NULL;
