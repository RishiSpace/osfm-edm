-- migrations/006_jobs.sql — Job queue and execution logs.
CREATE TABLE jobs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    job_type     TEXT NOT NULL,
    payload      JSONB,
    status       TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending', 'running', 'done', 'failed', 'cancelled')),
    created_at   TIMESTAMPTZ DEFAULT now(),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ,
    exit_code    INTEGER
);

CREATE TABLE job_logs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id       UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    time         TIMESTAMPTZ DEFAULT now(),
    line         TEXT NOT NULL,
    stream       TEXT NOT NULL CHECK (stream IN ('stdout', 'stderr'))
);

CREATE INDEX idx_jobs_device_status ON jobs (device_id, status);
CREATE INDEX idx_job_logs_job_id ON job_logs (job_id, time);
