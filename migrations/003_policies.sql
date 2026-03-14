-- migrations/003_policies.sql — Policy definitions and assignments.
CREATE TABLE policies (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL,
    description  TEXT,
    rules        JSONB NOT NULL DEFAULT '[]',
    created_at   TIMESTAMPTZ DEFAULT now(),
    updated_at   TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE policy_assignments (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id    UUID NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    target_type  TEXT NOT NULL CHECK (target_type IN ('device', 'group')),
    target_id    UUID NOT NULL,
    assigned_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE (policy_id, target_type, target_id)
);
