-- migrations/008_users.sql — Users, authentication, and refresh tokens.
CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username      TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    totp_secret   TEXT,
    totp_enabled  BOOLEAN NOT NULL DEFAULT false,
    role          TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'viewer')),
    created_at    TIMESTAMPTZ DEFAULT now(),
    last_login    TIMESTAMPTZ
);

CREATE TABLE refresh_tokens (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   TEXT UNIQUE NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ DEFAULT now(),
    revoked      BOOLEAN NOT NULL DEFAULT false
);
