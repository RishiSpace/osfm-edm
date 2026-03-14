-- migrations/004_groups.sql — Device groups and membership.
CREATE TABLE groups (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name         TEXT NOT NULL UNIQUE,
    description  TEXT,
    created_at   TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE group_members (
    group_id     UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    added_at     TIMESTAMPTZ DEFAULT now(),
    PRIMARY KEY (group_id, device_id)
);
