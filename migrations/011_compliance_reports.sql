-- Compliance reports: per-device, per-policy compliance evaluation results.
CREATE TABLE IF NOT EXISTS compliance_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    policy_id UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    compliant BOOLEAN NOT NULL DEFAULT false,
    detail JSONB DEFAULT '{}',
    reported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (device_id, policy_id)
);

CREATE INDEX IF NOT EXISTS idx_compliance_device ON compliance_reports(device_id);
CREATE INDEX IF NOT EXISTS idx_compliance_policy ON compliance_reports(policy_id);
