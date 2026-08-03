-- migrations/012_fix_kernel_events_schema.sql
-- Add the `payload` column that agent_hub.rs expects when inserting system events.
-- The original schema (002) had pid/process_path/detail but the server code inserts
-- a single JSONB `payload` column containing the full event.
ALTER TABLE kernel_events ADD COLUMN IF NOT EXISTS payload JSONB;
