-- migrations/014_fix_jobs_status.sql
-- Expand the jobs status CHECK constraint to include 'dispatched' and 'completed'
-- which are used by job_queue.rs and agent_hub.rs respectively.
ALTER TABLE jobs DROP CONSTRAINT IF EXISTS jobs_status_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_status_check
    CHECK (status IN ('pending', 'dispatched', 'running', 'completed', 'done', 'failed', 'cancelled'));
