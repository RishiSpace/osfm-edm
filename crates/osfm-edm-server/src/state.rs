//! Application state — shared across all handlers via Arc<AppState>.

use std::sync::Arc;

use dashmap::DashMap;
use osfm_edm_common::protocol::ServerMessage;
use sqlx::PgPool;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::config::Config;
use crate::services::pki::CertificateAuthority;
use crate::services::signing::JobSigner;

/// Represents a connected agent's WebSocket write handle.
#[derive(Debug)]
pub struct AgentConnection {
    pub device_id: Uuid,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Channel to push messages to this agent's WebSocket write loop.
    pub tx: mpsc::Sender<ServerMessage>,
}

/// An event from an agent's shell session, relayed to SSE subscribers.
#[derive(Debug, Clone)]
pub struct ShellEvent {
    /// The shell session this event belongs to.
    pub session_id: Uuid,
    /// Shell output data (stdout/stderr). Empty if this is a close event.
    pub data: String,
    /// True when the shell session has closed.
    pub closed: bool,
    /// Exit code, set only when `closed` is true.
    pub exit_code: Option<i32>,
}

/// Ownership metadata for an open shell session. Shell endpoints only serve
/// the user who opened the session (or an admin).
#[derive(Debug, Clone)]
pub struct ShellSessionMeta {
    /// The user who opened the session.
    pub owner_id: Uuid,
    /// The device the shell runs on.
    pub device_id: Uuid,
}

/// Central application state shared across all request handlers.
pub struct AppState {
    /// PostgreSQL connection pool.
    pub db: PgPool,
    /// Server configuration.
    pub config: Config,
    /// Map of device_id → active agent connection. Used to push messages to agents.
    pub connected_agents: DashMap<Uuid, AgentConnection>,
    /// Internal Certificate Authority for mTLS.
    pub ca: Option<CertificateAuthority>,
    /// Ed25519 signer used to sign jobs dispatched to agents.
    pub job_signer: Option<JobSigner>,
    /// Map of session_id → broadcast sender for shell output relay to SSE clients.
    pub shell_broadcasts: DashMap<Uuid, broadcast::Sender<ShellEvent>>,
    /// Map of session_id → shell session ownership metadata.
    pub shell_sessions: DashMap<Uuid, ShellSessionMeta>,
    /// Failed login attempts per username, for simple in-memory rate limiting.
    pub login_attempts: DashMap<String, Vec<std::time::Instant>>,
}

impl AppState {
    /// Create a new AppState instance.
    pub fn new(db: PgPool, config: Config, ca: Option<CertificateAuthority>, job_signer: Option<JobSigner>) -> Arc<Self> {
        Arc::new(Self {
            db,
            config,
            connected_agents: DashMap::new(),
            ca,
            job_signer,
            shell_broadcasts: DashMap::new(),
            shell_sessions: DashMap::new(),
            login_attempts: DashMap::new(),
        })
    }

    /// Sign a job payload for dispatch. Returns an empty signature if the
    /// signing service is unavailable (agents will refuse such jobs).
    pub fn sign_job(&self, job_id: &Uuid, payload: &osfm_edm_common::jobs::JobPayload) -> String {
        match &self.job_signer {
            Some(signer) => signer.sign_job(job_id, payload),
            None => {
                tracing::error!("Job signer not initialized — dispatching with empty signature");
                String::new()
            }
        }
    }

    /// Send a message to a specific connected agent.
    pub async fn send_to_agent(&self, device_id: &Uuid, msg: ServerMessage) -> bool {
        if let Some(conn) = self.connected_agents.get(device_id) {
            conn.tx.send(msg).await.is_ok()
        } else {
            false
        }
    }

    /// Broadcast a message to all connected agents.
    pub async fn broadcast(&self, msg: ServerMessage) {
        for entry in self.connected_agents.iter() {
            let _ = entry.tx.send(msg.clone()).await;
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .field("connected_agents_count", &self.connected_agents.len())
            .field("ca_initialized", &self.ca.is_some())
            .field("job_signer_initialized", &self.job_signer.is_some())
            .finish()
    }
}
