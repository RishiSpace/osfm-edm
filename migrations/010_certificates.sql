-- migrations/010_certificates.sql — Enrollment tokens and device TLS certificates.
CREATE TABLE enrollment_tokens (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token        TEXT UNIQUE NOT NULL,
    created_by   UUID REFERENCES users(id) ON DELETE SET NULL,
    used         BOOLEAN NOT NULL DEFAULT false,
    used_at      TIMESTAMPTZ,
    used_by      UUID REFERENCES devices(id) ON DELETE SET NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE certificates (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE UNIQUE,
    fingerprint  TEXT UNIQUE NOT NULL,
    pem          TEXT NOT NULL,
    issued_at    TIMESTAMPTZ DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked      BOOLEAN NOT NULL DEFAULT false,
    revoked_at   TIMESTAMPTZ
);
