//! Application state — shared across all handlers via Arc<AppState>.

use std::sync::Arc;

use dashmap::DashMap;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::services::pki::CertificateAuthority;

/// Represents a connected agent's WebSocket write handle.
/// Populated in Phase 6 when the WebSocket hub is implemented.
#[derive(Debug)]
pub struct AgentConnection {
    pub device_id: Uuid,
    pub connected_at: chrono::DateTime<chrono::Utc>,
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
