CREATE UNIQUE INDEX devices_org_public_key_idx
    ON devices (organization_id, public_key_id);
