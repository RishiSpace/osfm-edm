//! Alert engine — evaluates alert rules against incoming telemetry and creates alert events.

use crate::config::Config;
use sqlx::PgPool;
use uuid::Uuid;

/// Check alert rules for a device after a telemetry snapshot is received.
/// Called from the WebSocket hub after inserting telemetry.
pub async fn check_alerts(db: &PgPool, config: &Config, device_id: Uuid) {
    // Fetch active alert rules.
    #[derive(sqlx::FromRow)]
    struct AlertRule {
        id: Uuid,
        name: String,
        metric: String,
        operator: String,
        threshold: f64,
        severity: String,
    }

    let rules: Vec<AlertRule> = sqlx::query_as(
        "SELECT id, name, metric, operator, threshold, severity FROM alert_rules WHERE enabled = true",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    if rules.is_empty() {
        return;
    }

    // Get the latest telemetry for this device. Columns are nullable —
    // decode as Option and skip rules whose inputs are missing instead of
    // failing the whole check (a NULL row previously aborted evaluation).
    #[derive(sqlx::FromRow)]
    struct LatestMetrics {
        cpu_pct: Option<f64>,
        ram_used_mb: Option<i64>,
        ram_total_mb: Option<i64>,
        disk_used_gb: Option<f64>,
        disk_total_gb: Option<f64>,
    }

    let metrics: Option<LatestMetrics> = sqlx::query_as(
        "SELECT cpu_pct, ram_used_mb, ram_total_mb, disk_used_gb, disk_total_gb \
         FROM device_metrics WHERE device_id = $1 ORDER BY time DESC LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let Some(m) = metrics else { return };

    for rule in rules {
        let metric_value = match rule.metric.as_str() {
            "cpu_pct" => m.cpu_pct,
            "ram_pct" => match (m.ram_used_mb, m.ram_total_mb) {
                (Some(used), Some(total)) if total > 0 => {
                    Some((used as f64 / total as f64) * 100.0)
                }
                _ => None,
            },
            "disk_pct" => match (m.disk_used_gb, m.disk_total_gb) {
                (Some(used), Some(total)) if total > 0.0 => Some((used / total) * 100.0),
                _ => None,
            },
            _ => None,
        };

        let Some(value) = metric_value else { continue };

        let triggered = match rule.operator.as_str() {
            ">" | "gt" => value > rule.threshold,
            ">=" | "gte" => value >= rule.threshold,
            "<" | "lt" => value < rule.threshold,
            "<=" | "lte" => value <= rule.threshold,
            "==" | "eq" => (value - rule.threshold).abs() < f64::EPSILON,
            _ => false,
        };

        if triggered {
            let open: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM alert_events \
                 WHERE rule_id = $1 AND device_id = $2 AND resolved_at IS NULL)",
            )
            .bind(rule.id)
            .bind(device_id)
            .fetch_one(db)
            .await
            .unwrap_or(false);
            if open {
                continue;
            }

            tracing::warn!(
                device_id = %device_id,
                rule = %rule.name,
                metric = %rule.metric,
                value = value,
                threshold = rule.threshold,
                severity = %rule.severity,
                "Alert triggered"
            );

            let event_id: Option<Uuid> = sqlx::query_scalar(
                "INSERT INTO alert_events (rule_id, device_id, severity, message, triggered_at) \
                 VALUES ($1, $2, $3, $4, now()) RETURNING id",
            )
            .bind(rule.id)
            .bind(device_id)
            .bind(&rule.severity)
            .bind(format!(
                "{}: {} is {:.1} (threshold: {} {})",
                rule.name, rule.metric, value, rule.operator, rule.threshold
            ))
            .fetch_optional(db)
            .await
            .ok()
            .flatten();

            // Dispatch notifications (SMTP, webhook, ntfy.sh).
            if let Some(event_id) = event_id {
                crate::services::notifications::notify(db, event_id, config).await;
            }
        }
    }
}
