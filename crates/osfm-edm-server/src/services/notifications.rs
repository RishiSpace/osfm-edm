//! Notifications service — dispatches alert notifications via SMTP, webhook, and ntfy.sh.
//!
//! Each alert rule has a `channels` JSONB column in the database that specifies
//! which notification backends to use. If no channels are configured on the rule,
//! the system falls back to the server-level defaults (env vars).

use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;

/// An alert event with its associated rule configuration.
#[derive(Debug, sqlx::FromRow)]
struct AlertNotification {
    severity: String,
    message: String,
    device_id: Uuid,
    rule_name: String,
    channels: serde_json::Value,
}

/// Dispatch a notification for a triggered alert event.
///
/// Reads the alert event and its associated rule channels from the database,
/// then sends notifications via all configured backends (SMTP, webhook, ntfy.sh).
/// Falls back to server-level notification config if no rule-level channels are set.
pub async fn notify(db: &PgPool, alert_event_id: Uuid, config: &Config) {
    let notification: Option<AlertNotification> = sqlx::query_as(
        "SELECT ae.severity, ae.message, ae.device_id, ar.name AS rule_name, ar.channels \
         FROM alert_events ae \
         JOIN alert_rules ar ON ae.rule_id = ar.id \
         WHERE ae.id = $1",
    )
    .bind(alert_event_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let Some(n) = notification else {
        tracing::warn!(alert_event_id = %alert_event_id, "Alert event not found for notification");
        return;
    };

    // Always log the alert.
    log_alert(&n);

    // Determine which channels to use (rule-level or server-level fallback).
    let channels = &n.channels;

    // ── SMTP Email ──
    let smtp_recipient = channels
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if smtp_recipient.is_some() || config.smtp_host.is_some() {
        let to = smtp_recipient.unwrap_or_else(|| config.smtp_from.clone());
        send_email(config, &to, &n).await;
    }

    // ── Webhook ──
    let webhook_url = channels
        .get("webhook")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| config.webhook_url.clone());

    if let Some(url) = webhook_url {
        send_webhook(&url, &n).await;
    }

    // ── ntfy.sh ──
    let ntfy_topic = channels
        .get("ntfy")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| config.ntfy_topic.clone());

    if let Some(topic) = ntfy_topic {
        send_ntfy(&config.ntfy_server, &topic, &n).await;
    }
}

/// Log the alert to tracing (always done regardless of channel config).
fn log_alert(n: &AlertNotification) {
    match n.severity.as_str() {
        "critical" => {
            tracing::error!(
                device_id = %n.device_id,
                rule = %n.rule_name,
                severity = "critical",
                "ALERT: {}",
                n.message
            );
        }
        "warning" => {
            tracing::warn!(
                device_id = %n.device_id,
                rule = %n.rule_name,
                severity = "warning",
                "ALERT: {}",
                n.message
            );
        }
        _ => {
            tracing::info!(
                device_id = %n.device_id,
                rule = %n.rule_name,
                severity = %n.severity,
                "ALERT: {}",
                n.message
            );
        }
    }
}

// ─── SMTP Email ──────────────────────────────────────────────────────────────

/// Send an alert notification via SMTP email.
async fn send_email(config: &Config, to: &str, n: &AlertNotification) {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let Some(smtp_host) = &config.smtp_host else {
        tracing::debug!("SMTP not configured — skipping email notification");
        return;
    };

    let subject = format!(
        "[OSFM-EDM] {} alert: {}",
        n.severity.to_uppercase(),
        n.rule_name
    );

    let body = format!(
        "Alert: {rule}\n\
         Severity: {severity}\n\
         Device: {device}\n\
         \n\
         {message}\n\
         \n\
         — OSFM-EDM Endpoint Management",
        rule = n.rule_name,
        severity = n.severity,
        device = n.device_id,
        message = n.message,
    );

    let email = match Message::builder()
        .from(
            config
                .smtp_from
                .parse()
                .unwrap_or_else(|_| "osfm-edm@localhost".parse().unwrap()),
        )
        .to(match to.parse() {
            Ok(addr) => addr,
            Err(e) => {
                tracing::warn!(error = %e, to, "Invalid email recipient");
                return;
            }
        })
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body)
    {
        Ok(email) => email,
        Err(e) => {
            tracing::error!(error = %e, "Failed to build email message");
            return;
        }
    };

    let mut transport_builder =
        match AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host) {
            Ok(t) => t.port(config.smtp_port),
            Err(e) => {
                tracing::error!(error = %e, smtp_host, "Failed to create SMTP transport");
                return;
            }
        };

    if let (Some(user), Some(pass)) = (&config.smtp_user, &config.smtp_password) {
        transport_builder =
            transport_builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }

    let mailer = transport_builder.build();

    match mailer.send(email).await {
        Ok(_) => {
            tracing::info!(to, rule = %n.rule_name, "Email notification sent");
        }
        Err(e) => {
            tracing::error!(error = %e, to, "Failed to send email notification");
        }
    }
}

// ─── Webhook ─────────────────────────────────────────────────────────────────

/// Send an alert notification via HTTP webhook (JSON POST).
async fn send_webhook(url: &str, n: &AlertNotification) {
    let payload = serde_json::json!({
        "event": "alert",
        "severity": n.severity,
        "rule": n.rule_name,
        "device_id": n.device_id.to_string(),
        "message": n.message,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::new();
    match client.post(url).json(&payload).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                tracing::info!(url, rule = %n.rule_name, "Webhook notification sent");
            } else {
                tracing::warn!(
                    url,
                    status = %resp.status(),
                    "Webhook returned non-2xx status"
                );
            }
        }
        Err(e) => {
            tracing::error!(error = %e, url, "Failed to send webhook notification");
        }
    }
}

// ─── ntfy.sh ─────────────────────────────────────────────────────────────────

/// Send an alert notification via ntfy.sh (or compatible self-hosted server).
///
/// ntfy.sh is a simple pub-sub notification service. We POST the alert message
/// to `{server}/{topic}` with priority and tags headers.
async fn send_ntfy(server: &str, topic: &str, n: &AlertNotification) {
    let url = format!("{}/{}", server.trim_end_matches('/'), topic);

    let priority = match n.severity.as_str() {
        "critical" => "5", // max/urgent
        "warning" => "4",  // high
        _ => "3",          // default
    };

    let title = format!(
        "OSFM-EDM: {} — {}",
        n.severity.to_uppercase(),
        n.rule_name
    );

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("Title", &title)
        .header("Priority", priority)
        .header("Tags", format!("{}alert", if n.severity == "critical" { "rotating_light," } else { "" }))
        .body(format!("Device: {}\n{}", n.device_id, n.message))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                tracing::info!(topic, rule = %n.rule_name, "ntfy notification sent");
            } else {
                tracing::warn!(
                    topic,
                    status = %resp.status(),
                    "ntfy returned non-2xx status"
                );
            }
        }
        Err(e) => {
            tracing::error!(error = %e, topic, "Failed to send ntfy notification");
        }
    }
}
