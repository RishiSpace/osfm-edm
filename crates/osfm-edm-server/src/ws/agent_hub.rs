//! Agent hub — WebSocket handler for agent connections.
//!
//! Accepts WebSocket upgrades on `/ws?device_id=<uuid>`, authenticates the agent,
//! and runs bidirectional message loops. Incoming agent messages are dispatched to
//! the database; outgoing server messages are forwarded from the AppState channel.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use osfm_edm_common::protocol::{AgentMessage, ServerMessage};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::state::{AgentConnection, AppState};

/// Query parameters for the WebSocket upgrade request.
#[derive(Debug, Deserialize)]
pub struct WsParams {
    /// The device UUID (provided by the agent after enrollment).
    pub device_id: Uuid,
}

/// WebSocket upgrade handler — called from the router at `/ws`.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsParams>,
) -> impl IntoResponse {
    let device_id = params.device_id;
    tracing::info!(device_id = %device_id, "Agent WebSocket upgrade request");

    ws.on_upgrade(move |socket| handle_agent_connection(socket, state, device_id))
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
            for event in &events {
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
            // Append the log line to the job's log_output JSONB array.
            let log_entry = serde_json::json!({ "stream": stream, "line": line, "ts": chrono::Utc::now().to_rfc3339() });
            let _ = sqlx::query(
                "UPDATE jobs SET log_output = COALESCE(log_output, '[]'::jsonb) || $1::jsonb WHERE id = $2",
            )
            .bind(&log_entry)
            .bind(job_id)
            .execute(&state.db)
            .await;
        }

        AgentMessage::JobCompleted { job_id, exit_code } => {
            tracing::info!(job_id = %job_id, exit_code, "Job completed");
            let status = if exit_code == 0 { "completed" } else { "failed" };
            let _ = sqlx::query(
                "UPDATE jobs SET status = $1, exit_code = $2, completed_at = now() WHERE id = $3",
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
            // Clear old inventory and insert fresh.
            let _ = sqlx::query("DELETE FROM installed_software WHERE device_id = $1")
                .bind(device_id)
                .execute(&state.db)
                .await;

            for item in &software {
                let _ = sqlx::query(
                    "INSERT INTO installed_software (device_id, name, version, publisher, install_date) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(device_id)
                .bind(&item.name)
                .bind(item.version.as_deref())
                .bind(item.publisher.as_deref())
                .bind(item.install_date.as_deref())
                .execute(&state.db)
                .await;
            }
        }

        AgentMessage::ShellOutput { session_id, data } => {
            tracing::debug!(
                device_id = %device_id,
                session_id = %session_id,
                bytes = data.len(),
                "Shell output received"
            );
            // TODO: relay to dashboard via SSE/WS when dashboard is implemented.
            // For now, the output is logged and available for future streaming.
        }

        AgentMessage::ShellClosed { session_id, exit_code } => {
            tracing::info!(
                device_id = %device_id,
                session_id = %session_id,
                exit_code = ?exit_code,
                "Shell session closed"
            );
        }
    }
}
