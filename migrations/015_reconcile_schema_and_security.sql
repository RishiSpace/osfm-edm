-- migrations/015_reconcile_schema_and_security.sql
-- Reconcile remaining schema drift between migrations and server code, and add
-- the per-device auth token column used for WebSocket agent authentication.

-- ── jobs ─────────────────────────────────────────────────────────────────────
-- The jobs API (api/jobs.rs) expects created_by and log_output columns and
-- inserts without a job_type value. finished_at (from 006) is the canonical
-- completion timestamp — the code standardizes on it.
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS created_by UUID REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE jobs ADD COLUMN IF NOT EXISTS log_output JSONB;
ALTER TABLE jobs ALTER COLUMN job_type DROP NOT NULL;

-- ── policies ─────────────────────────────────────────────────────────────────
-- The policies API selects version (bumped on update) and enabled (soft toggle)
-- which the original migration never created.
ALTER TABLE policies ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE policies ADD COLUMN IF NOT EXISTS enabled BOOLEAN NOT NULL DEFAULT true;

-- ── groups ───────────────────────────────────────────────────────────────────
-- The API uniformly uses device_groups; the original migration created `groups`.
ALTER TABLE groups RENAME TO device_groups;

-- ── software inventory ───────────────────────────────────────────────────────
-- The server code uniformly uses installed_software; the original migration
-- created `software_inventory`.
ALTER TABLE software_inventory RENAME TO installed_software;

-- ── policy assignments ───────────────────────────────────────────────────────
-- Reshape (target_type, target_id) into explicit (device_id, group_id) columns
-- which the API and policy engine use. Existing data is migrated.
ALTER TABLE policy_assignments
    ADD COLUMN IF NOT EXISTS device_id UUID REFERENCES devices(id) ON DELETE CASCADE;
ALTER TABLE policy_assignments
    ADD COLUMN IF NOT EXISTS group_id UUID REFERENCES device_groups(id) ON DELETE CASCADE;

UPDATE policy_assignments SET device_id = target_id WHERE target_type = 'device';
UPDATE policy_assignments SET group_id   = target_id WHERE target_type = 'group';

ALTER TABLE policy_assignments DROP CONSTRAINT IF EXISTS policy_assignments_policy_id_target_type_target_id_key;
ALTER TABLE policy_assignments DROP COLUMN IF EXISTS target_type;
ALTER TABLE policy_assignments DROP COLUMN IF EXISTS target_id;

-- Partial unique indexes so ON CONFLICT DO NOTHING keeps working per target.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_policy_assignments_device
    ON policy_assignments (policy_id, device_id) WHERE device_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uniq_policy_assignments_group
    ON policy_assignments (policy_id, group_id) WHERE group_id IS NOT NULL;

-- ── device agent tokens ──────────────────────────────────────────────────────
-- SHA-256 hash of a per-device secret issued at enrollment. Agents present the
-- token (as a Bearer header) when opening the WebSocket; the server compares
-- hashes. Devices enrolled before this column exists have NULL and must be
-- re-enrolled to connect.
ALTER TABLE devices ADD COLUMN IF NOT EXISTS auth_token_hash TEXT;
