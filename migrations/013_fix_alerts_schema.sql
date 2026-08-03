-- migrations/013_fix_alerts_schema.sql
-- Fix alert_rules: add columns that alert_engine.rs queries expect.
-- The original schema (007) used a generic `condition JSONB` column, but the
-- alert engine queries individual typed columns for rule evaluation.
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS metric TEXT;
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS operator TEXT;
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS threshold DOUBLE PRECISION;
ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS severity TEXT DEFAULT 'warning';

-- Fix alert_events: add columns that alert_engine.rs and notifications.rs expect.
-- The original schema had `fired_at` + `detail` but the code writes
-- `severity`, `message`, `triggered_at`.
ALTER TABLE alert_events ADD COLUMN IF NOT EXISTS severity TEXT;
ALTER TABLE alert_events ADD COLUMN IF NOT EXISTS message TEXT;
ALTER TABLE alert_events ADD COLUMN IF NOT EXISTS triggered_at TIMESTAMPTZ;
