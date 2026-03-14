-- migrations/009_audit_log.sql — Immutable audit trail for all state-changing API operations.
CREATE TABLE audit_log (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    time         TIMESTAMPTZ DEFAULT now(),
    user_id      UUID REFERENCES users(id) ON DELETE SET NULL,
    action       TEXT NOT NULL,
    target_type  TEXT,
    target_id    UUID,
    detail       JSONB,
    ip_address   TEXT
);
CREATE INDEX idx_audit_log_time ON audit_log (time DESC);
