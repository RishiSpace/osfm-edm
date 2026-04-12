//! Application state — shared across all handlers via Arc<AppState>.

use std::sync::Arc;

use dashmap::DashMap;
use osfm_edm_common::protocol::ServerMessage;
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::Config;
use crate::services::pki::CertificateAuthority;

/// Represents a connected agent's WebSocket write handle.
#[derive(Debug)]
pub struct AgentConnection {
    pub device_id: Uuid,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Channel to push messages to this agent's WebSocket write loop.
    pub tx: mpsc::Sender<ServerMessage>,
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
}

impl AppState {
    /// Create a new AppState instance.
    pub fn new(db: PgPool, config: Config, ca: Option<CertificateAuthority>) -> Arc<Self> {
        Arc::new(Self {
            db,
            config,
            connected_agents: DashMap::new(),
            ca,
        })
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
            .finish()
    }
}
