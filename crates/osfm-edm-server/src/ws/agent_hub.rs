//! Agent hub — WebSocket handler for agent connections.
//!
//! Accepts WebSocket upgrades on `/ws?device_id=<uuid>`, authenticates the agent,
//! and runs bidirectional message loops. Incoming agent messages are dispatched to
//! the database; outgoing server messages are forwarded from the AppState channel.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use osfm_edm_common::protocol::{AgentMessage, ServerMessage};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::state::{AgentConnection, AppState};

/// Maximum number of system events accepted in a single batch (DoS guard).
const MAX_EVENTS_PER_BATCH: usize = 1000;

/// Query parameters for the WebSocket upgrade request.
#[derive(Debug, Deserialize)]
pub struct WsParams {
    /// The device UUID (provided by the agent after enrollment).
    pub device_id: Uuid,
}

/// WebSocket upgrade handler — called from the router at `/ws`.
///
/// The agent must present its per-device token (issued at enrollment) as an
/// `Authorization: Bearer <token>` header. The server compares the SHA-256 of
/// the presented token against the stored hash — the device_id alone is NOT
/// an authentication credential.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
) -> Response {
    let device_id = params.device_id;
    tracing::info!(device_id = %device_id, "Agent WebSocket upgrade request");

    if !authenticate_agent(&state, device_id, &headers).await {
        tracing::warn!(device_id = %device_id, "Rejected WebSocket: agent authentication failed");
        return (StatusCode::UNAUTHORIZED, "invalid or missing device token").into_response();
    }

    ws.on_upgrade(move |socket| handle_agent_connection(socket, state, device_id))
        .into_response()
}

/// Verify the agent's Bearer token against the stored SHA-256 hash.
async fn authenticate_agent(state: &AppState, device_id: Uuid, headers: &HeaderMap) -> bool {
    let presented = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = presented else {
        tracing::warn!(device_id = %device_id, "Missing Authorization header on agent WS");
        return false;
    };

    let presented_hash = format!("{:x}", Sha256::digest(token.as_bytes()));

    let stored: Option<String> =
        sqlx::query_scalar("SELECT auth_token_hash FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    match stored {
        Some(hash) => hash == presented_hash,
        None => {
            tracing::warn!(
                device_id = %device_id,
                "Device has no auth token (enrolled before token auth) — re-enroll the agent"
            );
            false
        }
    }
}

/// Manages the full lifecycle of a single agent WebSocket connection.
async fn handle_agent_connection(socket: WebSocket, state: Arc<AppState>, device_id: Uuid) {
    // Verify the device exists in the database.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

    if !exists {
        tracing::warn!(device_id = %device_id, "Rejected WebSocket: device not found");
        return;
    }

    // Create a channel for pushing messages to this agent.
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(64);

    // Register the connection.
    let conn = AgentConnection {
        device_id,
        connected_at: chrono::Utc::now(),
        tx,
    };
    state.connected_agents.insert(device_id, conn);

    // Update device status to online.
    let _ = sqlx::query("UPDATE devices SET status = 'online', last_seen = now() WHERE id = $1")
        .bind(device_id)
        .execute(&state.db)
        .await;

    tracing::info!(
        device_id = %device_id,
        online_count = state.connected_agents.len(),
        "Agent connected"
    );

    // Dispatch any pending jobs and push assigned policies.
    {
        let s = state.clone();
        let did = device_id;
        tokio::spawn(async move {
            crate::services::job_queue::dispatch_pending_jobs(&s, did).await;
            crate::services::policy_engine::push_policies_to_device(&s, did).await;
            let _ = s
                .send_to_agent(&did, ServerMessage::RequestInventory)
                .await;
        });
    }

    // Split the WebSocket into read and write halves.
    let (mut ws_write, mut ws_read) = socket.split();

    // Clone state for the read task.
    let read_state = state.clone();
    let read_device_id = device_id;

    // Spawn the write task — forwards ServerMessages from the channel to the WebSocket.
    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(text) => {
                    if ws_write.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize ServerMessage");
                }
            }
        }
    });

    // Read loop — process incoming AgentMessages.
    while let Some(Ok(msg)) = ws_read.next().await {
        match msg {
            Message::Text(text) => {
                match serde_json::from_str::<AgentMessage>(&text) {
                    Ok(agent_msg) => {
                        process_agent_message(&read_state, read_device_id, agent_msg).await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            device_id = %read_device_id,
                            error = %e,
                            "Failed to parse agent message"
                        );
                    }
                }
            }
            Message::Ping(data) => {
                // Axum auto-responds to pings, but we handle it just in case.
                let _ = state
                    .connected_agents
                    .get(&device_id)
                    .map(|_| tracing::trace!("Received ping from {}", device_id));
                let _ = data; // consumed
            }
            Message::Close(_) => {
                tracing::info!(device_id = %read_device_id, "Agent sent close frame");
                break;
            }
            _ => {}
        }
    }

    // Clean up: remove from connected agents, mark offline.
    state.connected_agents.remove(&device_id);
    let _ = sqlx::query("UPDATE devices SET status = 'offline' WHERE id = $1")
        .bind(device_id)
        .execute(&state.db)
        .await;

    write_task.abort();

    tracing::info!(
        device_id = %device_id,
        online_count = state.connected_agents.len(),
        "Agent disconnected"
    );
}

/// Process a single incoming agent message — dispatches to the database.
async fn process_agent_message(state: &AppState, device_id: Uuid, msg: AgentMessage) {
    match msg {
        AgentMessage::Heartbeat { agent_version } => {
            tracing::debug!(device_id = %device_id, agent_version, "Heartbeat");
            let _ = sqlx::query(
                "UPDATE devices SET last_seen = now(), agent_version = $1, status = 'online' WHERE id = $2",
            )
            .bind(&agent_version)
            .bind(device_id)
            .execute(&state.db)
            .await;
        }

        AgentMessage::TelemetryReport { snapshot } => {
            tracing::debug!(device_id = %device_id, cpu = snapshot.cpu_pct, "Telemetry received");
            let _ = sqlx::query(
                "INSERT INTO device_metrics (device_id, time, cpu_pct, ram_used_mb, ram_total_mb, disk_used_gb, disk_total_gb, uptime_secs) \
                 VALUES ($1, now(), $2, $3, $4, $5, $6, $7)",
            )
            .bind(device_id)
            .bind(snapshot.cpu_pct)
            .bind(snapshot.ram_used_mb as i64)
            .bind(snapshot.ram_total_mb as i64)
            .bind(snapshot.disk_used_gb)
            .bind(snapshot.disk_total_gb)
            .bind(snapshot.uptime_secs as i64)
            .execute(&state.db)
            .await;

            // Check alert rules against this fresh telemetry.
            crate::services::alert_engine::check_alerts(&state.db, &state.config, device_id).await;
        }

        AgentMessage::SystemEventBatch { events } => {
            tracing::debug!(device_id = %device_id, count = events.len(), "System events received");
            // DoS guard: cap batch size, skip the excess rather than aborting.
            let accepted = if events.len() > MAX_EVENTS_PER_BATCH {
                tracing::warn!(
                    device_id = %device_id,
                    count = events.len(),
                    max = MAX_EVENTS_PER_BATCH,
                    "Event batch exceeds limit — truncating"
                );
                &events[..MAX_EVENTS_PER_BATCH]
            } else {
                &events[..]
            };
            for event in accepted {
                let event_json = serde_json::to_value(event).unwrap_or_default();
                let event_type = match event {
                    osfm_edm_common::events::SystemEvent::ProcessStarted { .. } => "process_started",
                    osfm_edm_common::events::SystemEvent::ProcessExited { .. } => "process_exited",
                    osfm_edm_common::events::SystemEvent::FileAccessed { .. } => "file_accessed",
                    osfm_edm_common::events::SystemEvent::NetworkConnected { .. } => "network_connected",
                    osfm_edm_common::events::SystemEvent::RegistryChanged { .. } => "registry_changed",
                };
                let _ = sqlx::query(
                    "INSERT INTO kernel_events (device_id, time, event_type, payload) VALUES ($1, now(), $2, $3)",
                )
                .bind(device_id)
                .bind(event_type)
                .bind(&event_json)
                .execute(&state.db)
                .await;
            }
        }

        AgentMessage::JobLog {
            job_id,
            line,
            stream,
        } => {
            tracing::debug!(job_id = %job_id, stream, "Job log line");
            // Insert the log line into the job_logs table.
            let _ = sqlx::query(
                "INSERT INTO job_logs (job_id, line, stream) VALUES ($1, $2, $3)",
            )
            .bind(job_id)
            .bind(&line)
            .bind(&stream)
            .execute(&state.db)
            .await;
        }

        AgentMessage::JobCompleted { job_id, exit_code } => {
            tracing::info!(job_id = %job_id, exit_code, "Job completed");
            let status = if exit_code == 0 { "completed" } else { "failed" };
            let _ = sqlx::query(
                "UPDATE jobs SET status = $1, exit_code = $2, finished_at = now() WHERE id = $3",
            )
            .bind(status)
            .bind(exit_code)
            .bind(job_id)
            .execute(&state.db)
            .await;
        }

        AgentMessage::ComplianceReport { reports } => {
            tracing::info!(device_id = %device_id, count = reports.len(), "Compliance reports received");
            for report in &reports {
                let report_json = serde_json::to_value(report).unwrap_or_default();
                let _ = sqlx::query(
                    "INSERT INTO compliance_reports (device_id, policy_id, compliant, detail, reported_at) \
                     VALUES ($1, $2, $3, $4, now()) \
                     ON CONFLICT (device_id, policy_id) DO UPDATE SET compliant = $3, detail = $4, reported_at = now()",
                )
                .bind(device_id)
                .bind(report.policy_id)
                .bind(report.compliant)
                .bind(&report_json)
                .execute(&state.db)
                .await;
            }
        }

        AgentMessage::InventoryReport { software, patches } => {
            tracing::info!(
                device_id = %device_id,
                software_count = software.len(),
                patch_count = patches.len(),
                "Inventory report received"
            );
            // Replace software inventory + upsert patches atomically.
            if let Err(e) = persist_inventory(&state.db, device_id, &software, &patches).await {
                tracing::error!(device_id = %device_id, error = %e, "Failed to persist inventory");
            }
        }

        AgentMessage::ShellOutput { session_id, data } => {
            tracing::debug!(
                device_id = %device_id,
                session_id = %session_id,
                bytes = data.len(),
                "Shell output received"
            );
            // Relay to SSE subscribers via broadcast channel.
            if let Some(tx) = state.shell_broadcasts.get(&session_id) {
                let _ = tx.send(crate::state::ShellEvent {
                    session_id,
                    data,
                    closed: false,
                    exit_code: None,
                });
            }
        }

        AgentMessage::ShellClosed { session_id, exit_code } => {
            tracing::info!(
                device_id = %device_id,
                session_id = %session_id,
                exit_code = ?exit_code,
                "Shell session closed"
            );
            // Broadcast close event, then remove the broadcast channel.
            if let Some(tx) = state.shell_broadcasts.get(&session_id) {
                let _ = tx.send(crate::state::ShellEvent {
                    session_id,
                    data: String::new(),
                    closed: true,
                    exit_code,
                });
            }
            state.shell_broadcasts.remove(&session_id);
            state.shell_sessions.remove(&session_id);
        }
    }
}

/// Replace a device's software inventory and upsert its patch list in one
/// transaction (prevents torn state where old and new inventory interleave).
async fn persist_inventory(
    db: &sqlx::PgPool,
    device_id: Uuid,
    software: &[osfm_edm_common::protocol::SoftwareItem],
    patches: &[osfm_edm_common::protocol::PatchItem],
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM installed_software WHERE device_id = $1")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;

    for item in software {
        sqlx::query(
            "INSERT INTO installed_software (device_id, name, version, publisher, install_date) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(device_id)
        .bind(&item.name)
        .bind(item.version.as_deref())
        .bind(item.publisher.as_deref())
        .bind(item.install_date.as_deref())
        .execute(&mut *tx)
        .await?;
    }

    for patch in patches {
        // Coerce agent-provided strings into the CHECK-constrained domains.
        let severity = match patch.severity.as_deref().map(|s| s.to_lowercase()).as_deref() {
            Some("critical") => "critical",
            Some("important") => "important",
            Some("moderate") => "moderate",
            Some("low") => "low",
            _ => "unknown",
        };
        let status = match patch.status.as_str() {
            "installed" => "installed",
            "failed" => "failed",
            // Agent reports "available" for pending updates.
            _ => "pending",
        };
        sqlx::query(
            "INSERT INTO patches (device_id, patch_id, title, severity, status) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (device_id, patch_id) DO UPDATE \
             SET title = EXCLUDED.title, severity = EXCLUDED.severity, status = EXCLUDED.status",
        )
        .bind(device_id)
        .bind(&patch.patch_id)
        .bind(patch.title.as_deref())
        .bind(severity)
        .bind(status)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await
}
