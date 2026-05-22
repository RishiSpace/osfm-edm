//! Shell API — open and close remote terminal sessions on devices.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use osfm_edm_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:device_id", post(open_shell))
        .route("/:session_id/close", delete(close_shell))
        .route("/:session_id/input", post(send_input))
}

/// Open a remote shell session on a device.
///
/// POST /api/v1/shell/:device_id
///
/// Returns the session_id for the new shell session.
async fn open_shell(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session_id = Uuid::new_v4();

    let sent = state
        .send_to_agent(&device_id, ServerMessage::OpenShell { session_id })
        .await;

    if !sent {
        return Err(ApiError::NotFound(format!(
            "Device {device_id} is not connected"
        )));
    }

    tracing::info!(
        device_id = %device_id,
        session_id = %session_id,
        "Shell session opened"
    );

    Ok(Json(serde_json::json!({
        "data": {
            "session_id": session_id,
            "device_id": device_id,
        },
        "error": null,
    })))
}

#[derive(serde::Deserialize)]
struct ShellInputBody {
    data: String,
    device_id: Uuid,
}

/// Send input to an active shell session.
///
/// POST /api/v1/shell/:session_id/input
/// Body: { "data": "ls -la\n", "device_id": "..." }
async fn send_input(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<ShellInputBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sent = state
        .send_to_agent(
            &body.device_id,
            ServerMessage::ShellInput {
                session_id,
                data: body.data,
            },
        )
        .await;

    if !sent {
        return Err(ApiError::NotFound(format!(
            "Device {} is not connected",
            body.device_id
        )));
    }

    Ok(Json(serde_json::json!({
        "data": { "status": "sent" },
        "error": null,
    })))
}

/// Close a remote shell session.
///
/// DELETE /api/v1/shell/:session_id/close
/// Query param: device_id
async fn close_shell(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let device_id_str = params
        .get("device_id")
        .ok_or_else(|| ApiError::BadRequest("Missing device_id query parameter".to_string()))?;

    let device_id: Uuid = device_id_str
        .parse()
        .map_err(|_| ApiError::BadRequest("Invalid device_id".to_string()))?;

    let sent = state
        .send_to_agent(&device_id, ServerMessage::CloseShell { session_id })
        .await;

    if !sent {
        return Err(ApiError::NotFound(format!(
            "Device {device_id} is not connected"
        )));
    }

    Ok(Json(serde_json::json!({
        "data": { "status": "closed" },
        "error": null,
    })))
}
