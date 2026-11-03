ALTER TABLE users ADD COLUMN password_hash TEXT;

CREATE TABLE auth_sessions (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    access_token_hash TEXT NOT NULL UNIQUE,
    refresh_token_hash TEXT NOT NULL UNIQUE,
    issued_at TEXT NOT NULL,
    access_expires_at TEXT NOT NULL,
    refresh_expires_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE INDEX auth_sessions_user_idx ON auth_sessions(user_id, revoked_at);
CREATE INDEX auth_sessions_access_idx ON auth_sessions(access_token_hash, revoked_at);
CREATE INDEX auth_sessions_refresh_idx ON auth_sessions(refresh_token_hash, revoked_at);
