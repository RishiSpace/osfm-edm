-- migrations/007_alerts.sql — Patches, alert rules, and alert events.
CREATE TABLE patches (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    patch_id     TEXT NOT NULL,
    title        TEXT,
    severity     TEXT CHECK (severity IN ('critical', 'important', 'moderate', 'low', 'unknown')),
    status       TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending', 'installed', 'failed')),
    detected_at  TIMESTAMPTZ DEFAULT now(),
    applied_at   TIMESTAMPTZ,
    UNIQUE (device_id, patch_id)
);

CREATE TABLE alert_rules (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL,
    condition    JSONB NOT NULL,
    channels     JSONB NOT NULL DEFAULT '{}',
    enabled      BOOLEAN NOT NULL DEFAULT true,
    created_at   TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE alert_events (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_id      UUID NOT NULL REFERENCES alert_rules(id) ON DELETE CASCADE,
    device_id    UUID REFERENCES devices(id) ON DELETE SET NULL,
    fired_at     TIMESTAMPTZ DEFAULT now(),
    detail       JSONB,
    resolved_at  TIMESTAMPTZ
);
