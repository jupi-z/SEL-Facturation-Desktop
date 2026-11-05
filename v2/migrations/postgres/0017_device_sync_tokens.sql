ALTER TABLE devices ADD COLUMN sync_token_hash TEXT;
CREATE UNIQUE INDEX devices_sync_token_hash_idx
    ON devices (sync_token_hash)
    WHERE sync_token_hash IS NOT NULL;
