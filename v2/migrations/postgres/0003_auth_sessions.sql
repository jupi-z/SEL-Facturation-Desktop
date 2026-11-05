ALTER TABLE users ADD COLUMN password_hash TEXT;

CREATE TABLE auth_sessions (
    id UUID PRIMARY KEY,
    organization_id UUID NOT NULL REFERENCES organizations(id),
    user_id UUID NOT NULL REFERENCES users(id),
    access_token_hash TEXT NOT NULL UNIQUE,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX auth_sessions_user_idx ON auth_sessions(user_id, revoked_at);
CREATE INDEX auth_sessions_access_idx ON auth_sessions(access_token_hash, revoked_at);
CREATE INDEX auth_sessions_refresh_idx ON auth_sessions(refresh_token_hash, revoked_at);
