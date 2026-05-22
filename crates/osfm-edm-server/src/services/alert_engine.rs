//! Alert engine — evaluates alert rules against incoming telemetry and creates alert events.

use sqlx::PgPool;
use uuid::Uuid;
use crate::config::Config;

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

    // Get the latest telemetry for this device.
    #[derive(sqlx::FromRow)]
    struct LatestMetrics {
        cpu_pct: f64,
        ram_used_mb: i64,
        ram_total_mb: i64,
        disk_used_gb: f64,
        disk_total_gb: f64,
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
            "cpu_pct" => Some(m.cpu_pct),
            "ram_pct" => {
                if m.ram_total_mb > 0 {
                    Some((m.ram_used_mb as f64 / m.ram_total_mb as f64) * 100.0)
                } else {
                    None
                }
            }
            "disk_pct" => {
                if m.disk_total_gb > 0.0 {
                    Some((m.disk_used_gb / m.disk_total_gb) * 100.0)
                } else {
                    None
                }
            }
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
            tracing::warn!(
                device_id = %device_id,
                rule = %rule.name,
                metric = %rule.metric,
                value = value,
                threshold = rule.threshold,
                severity = %rule.severity,
                "Alert triggered"
            );

            // Insert alert event and dispatch notification.
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
