//! Settings API — server configuration management.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

use crate::error::ApiResult;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the settings sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_settings))
        .route("/status", get(server_status))
}

/// GET /api/v1/settings — read current server configuration (non-secret fields).
async fn get_settings(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "data": {
            "server_port": state.config.server_port,
            "agent_port": state.config.agent_port,
            "server_url": state.config.server_url,
            "tls_configured": state.config.tls_cert_path.is_some(),
            "ca_initialized": state.ca.is_some(),
        },
        "error": null
    })))
}

/// GET /api/v1/settings/status — server runtime status.
async fn server_status(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let total_devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&state.db)
        .await?;

    let online_devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM devices WHERE status = 'online'",
    )
    .fetch_one(&state.db)
    .await?;

    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await?;

    let total_policies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policies")
        .fetch_one(&state.db)
        .await?;

    let pending_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE status IN ('pending', 'dispatched')",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "total_devices": total_devices,
            "online_devices": online_devices,
            "connected_agents": state.connected_agents.len(),
            "total_users": total_users,
            "total_policies": total_policies,
            "pending_jobs": pending_jobs,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "error": null
    })))
}
