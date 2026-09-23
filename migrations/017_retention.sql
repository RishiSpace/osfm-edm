-- Retention policy for high-volume and sensitive tables.
-- Devices re-report continuously, so keep raw telemetry/events/logs bounded
-- while preserving the latest compliance state indefinitely.
--
-- Data handling: audit + metrics are Internal; agent tokens stay hashed.
-- Run order after all base migrations; safe to re-apply.

-- Trim raw telemetry older than 30 days.
DELETE FROM device_metrics WHERE time < now() - INTERVAL '30 days';

-- Trim system events older than 30 days.
DELETE FROM kernel_events WHERE time < now() - INTERVAL '30 days';

-- Keep only the latest 500 log lines per job; drop the rest.
DELETE FROM job_logs a USING (
    SELECT id FROM (
        SELECT id, ROW_NUMBER() OVER (PARTITION BY job_id ORDER BY time DESC) AS rn
        FROM job_logs
    ) ranked WHERE rn > 500
) old WHERE a.id = old.id;

-- Trim resolved alert events older than 90 days; unresolved stay.
DELETE FROM alert_events WHERE resolved_at IS NOT NULL AND resolved_at < now() - INTERVAL '90 days';

-- Trim audit log older than 1 year (compliance Reports derive from live tables).
DELETE FROM audit_log WHERE time < now() - INTERVAL '365 days';
