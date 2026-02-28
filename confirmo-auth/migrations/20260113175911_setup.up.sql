CREATE TABLE IF NOT EXISTS credentials (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR NOT NULL UNIQUE,
    password_hash VARCHAR NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    login_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS credential_verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    code VARCHAR NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    credential_id UUID NOT NULL REFERENCES credentials (id) ON DELETE CASCADE,
    CONSTRAINT unique_active_code UNIQUE (credential_id, code)
);

CREATE TYPE device_type as ENUM ('ios', 'android', 'web', 'desktop');

CREATE TABLE IF NOT EXISTS devices (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id UUID NOT NULL UNIQUE,
    name VARCHAR NOT NULL,
    platform device_type NOT NULL,
    os_version VARCHAR,
    browser VARCHAR,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_trusted BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    credential_id UUID NOT NULL REFERENCES credentials (id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE sessions (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    device_id UUID NOT NULL REFERENCES devices (device_id) ON DELETE CASCADE ON UPDATE CASCADE,
    credential_id UUID NOT NULL REFERENCES credentials (id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE UNIQUE INDEX unique_active_session_per_user ON sessions (credential_id) WHERE revoked_at IS NULL;

CREATE INDEX idx_credentials_email ON credentials (email);

CREATE INDEX idx_credential_verifications_lookup ON credential_verifications (credential_id, used_at, expires_at, created_at DESC);
