-- migrations/002_timescale_metrics.sql — TimescaleDB hypertables for metrics and kernel events.
CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE TABLE device_metrics (
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    time         TIMESTAMPTZ NOT NULL,
    cpu_pct      DOUBLE PRECISION,
    ram_used_mb  BIGINT,
    ram_total_mb BIGINT,
    disk_used_gb DOUBLE PRECISION,
    disk_total_gb DOUBLE PRECISION,
    uptime_secs  BIGINT
);
SELECT create_hypertable('device_metrics', 'time');

CREATE TABLE kernel_events (
    device_id    UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    time         TIMESTAMPTZ NOT NULL,
    event_type   TEXT NOT NULL,
    pid          INTEGER,
    process_path TEXT,
    detail       JSONB
);
SELECT create_hypertable('kernel_events', 'time');
