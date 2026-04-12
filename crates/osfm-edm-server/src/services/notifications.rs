//! Notifications service — dispatches alert notifications.
//! Currently log-based; future: SMTP/webhook integration.

use sqlx::PgPool;
use uuid::Uuid;

/// Send a notification for an alert event.
/// Currently just logs; will integrate SMTP/webhooks in a future phase.
pub async fn notify(db: &PgPool, alert_event_id: Uuid) {
    #[derive(sqlx::FromRow)]
    struct AlertEvent {
        severity: String,
        message: String,
        device_id: Uuid,
    }

    let event: Option<AlertEvent> = sqlx::query_as(
        "SELECT severity, message, device_id FROM alert_events WHERE id = $1",
    )
    .bind(alert_event_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    if let Some(event) = event {
        match event.severity.as_str() {
            "critical" => {
                tracing::error!(
                    device_id = %event.device_id,
                    severity = "critical",
                    "ALERT: {}",
                    event.message
                );
            }
            "warning" => {
                tracing::warn!(
                    device_id = %event.device_id,
                    severity = "warning",
                    "ALERT: {}",
                    event.message
                );
            }
            _ => {
                tracing::info!(
                    device_id = %event.device_id,
                    severity = %event.severity,
                    "ALERT: {}",
                    event.message
                );
            }
        }
    }
}
