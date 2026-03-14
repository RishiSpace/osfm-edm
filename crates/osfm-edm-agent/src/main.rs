//! osfm-edm-agent — User-space agent for managed devices.
//!
//! Enrolls with the OSFM-EDM server, maintains a WebSocket connection, and
//! sends heartbeats + telemetry at configurable intervals.

mod config;
mod enrollment;
mod jobs;
mod kernel_bridge;
mod policy;
mod telemetry;
mod transport;

use clap::Parser;
use osfm_edm_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use crate::config::AgentConfig;

/// OSFM-EDM Agent — endpoint management agent
#[derive(Parser, Debug)]
#[command(name = "osfm-edm-agent", version, about)]
struct Cli {
    /// Server URL for enrollment (e.g., https://osfm-edm.local:8443)
    #[arg(long)]
    server: Option<String>,

    /// One-time enrollment token
    #[arg(long)]
    token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("osfm_edm_agent=debug")),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!("OSFM-EDM agent starting");

    // Load config or run enrollment.
    let config = match AgentConfig::load() {
        Ok(config) => {
            tracing::info!(device_id = %config.device_id, "Loaded existing configuration");
            config
        }
        Err(config::ConfigError::NotEnrolled) => {
            // Need to enroll.
            let server = cli.server.as_deref().unwrap_or("https://localhost:8443");
            let token = cli.token.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Not enrolled. Use --server <url> --token <token> to enroll."
                )
            })?;

            enrollment::enroll(server, token).await?
        }
        Err(e) => {
            anyhow::bail!("Failed to load config: {e}");
        }
    };

    // Create channels for WebSocket communication.
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<AgentMessage>(256);
    let (inbound_tx, mut inbound_rx) = mpsc::channel(256);

    // Spawn the WebSocket connection loop.
    let ws_config = config.clone();
    tokio::spawn(async move {
        transport::websocket::run_ws_loop(&ws_config, &mut outbound_rx, inbound_tx).await;
    });

    // Spawn heartbeat + telemetry loop.
    let heartbeat_tx = outbound_tx.clone();
    let agent_version = env!("CARGO_PKG_VERSION").to_string();
    let heartbeat_interval = config.heartbeat_interval;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            tokio::time::Duration::from_secs(heartbeat_interval),
        );

        loop {
            interval.tick().await;

            // Send heartbeat.
            let _ = heartbeat_tx
                .send(AgentMessage::Heartbeat {
                    agent_version: agent_version.clone(),
                })
                .await;

            // Collect and send telemetry.
            let snapshot = telemetry::system::collect_snapshot();
            let _ = heartbeat_tx
                .send(AgentMessage::TelemetryReport { snapshot })
                .await;

            tracing::debug!("Sent heartbeat + telemetry");
        }
    });

    // Main message handling loop — process server messages.
    tracing::info!("Agent running — press Ctrl+C to stop");
    loop {
        tokio::select! {
            msg = inbound_rx.recv() => {
                match msg {
                    Some(server_msg) => {
                        handle_server_message(server_msg, &outbound_tx).await;
                    }
                    None => {
                        tracing::error!("Inbound channel closed");
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received shutdown signal");
                break;
            }
        }
    }

    Ok(())
}

/// Handle incoming server messages.
async fn handle_server_message(
    msg: osfm_edm_common::protocol::ServerMessage,
    _outbound_tx: &mpsc::Sender<AgentMessage>,
) {
    use osfm_edm_common::protocol::ServerMessage;

    match msg {
        ServerMessage::Heartbeat => {
            tracing::debug!("Received server heartbeat");
        }
        ServerMessage::RequestTelemetry => {
            tracing::info!("Server requested telemetry — sending snapshot");
            let snapshot = telemetry::system::collect_snapshot();
            let _ = _outbound_tx
                .send(AgentMessage::TelemetryReport { snapshot })
                .await;
        }
        ServerMessage::PushPolicy { policies } => {
            tracing::info!(count = policies.len(), "Received policy push");
            // Policy enforcement will be implemented in Phase 7.
        }
        ServerMessage::DispatchJob { job_id, .. } => {
            tracing::info!(job_id = %job_id, "Received job dispatch");
            // Job execution will be implemented in Phase 7.
        }
        ServerMessage::RevokeJob { job_id } => {
            tracing::info!(job_id = %job_id, "Received job revocation");
        }
        ServerMessage::RequestInventory => {
            tracing::info!("Server requested inventory");
            // Inventory collection will be implemented in Phase 9.
        }
    }
}
