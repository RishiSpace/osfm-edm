-- migrations/001_init_devices.sql — Core devices table.
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE devices (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hostname     TEXT NOT NULL,
    os           TEXT NOT NULL CHECK (os IN ('windows', 'linux', 'macos')),
    os_version   TEXT,
    arch         TEXT,
    ip_address   TEXT,
    agent_version TEXT,
    enrolled_at  TIMESTAMPTZ DEFAULT now(),
    last_seen    TIMESTAMPTZ,
    status       TEXT NOT NULL DEFAULT 'offline' CHECK (status IN ('online', 'offline', 'stale'))
);
