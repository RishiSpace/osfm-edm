//! Shell API — open/close remote terminal sessions and stream output via SSE.
//!
//! All endpoints require authentication. A session is bound to the user who
//! opened it: only that user (or an admin) can send input, read output, or
//! close it.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::error::ApiError;
use crate::middleware::auth::AuthUser;
use crate::state::{AppState, ShellSessionMeta};
use osfm_edm_common::protocol::ServerMessage;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/:device_id", post(open_shell))
        .route("/:session_id/close", delete(close_shell))
        .route("/:session_id/input", post(send_input))
        .route("/:session_id/stream", get(stream_shell))
}

/// Open a remote shell session on a device.
///
/// POST /api/v1/shell/:device_id
///
/// Returns the session_id for the new shell session.
async fn open_shell(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    auth.require_admin()?;

    let session_id = Uuid::new_v4();

    // Create the broadcast channel for this session's output relay.
    let (tx, _) = tokio::sync::broadcast::channel(256);
    state.shell_broadcasts.insert(session_id, tx);

    let sent = state
        .send_to_agent(&device_id, ServerMessage::OpenShell { session_id })
        .await;

    if !sent {
        // Clean up the broadcast channel since the agent isn't connected.
        state.shell_broadcasts.remove(&session_id);
        return Err(ApiError::NotFound(format!(
            "Device {device_id} is not connected"
        )));
    }

    // Bind the session to the opening user.
    state.shell_sessions.insert(
        session_id,
        ShellSessionMeta {
            owner_id: auth.user_id,
            device_id,
        },
    );

    tracing::info!(
        device_id = %device_id,
        session_id = %session_id,
        user = %auth.username,
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
}

/// Fetch session metadata and check the requester is allowed to use it.
fn authorize_session(
    state: &AppState,
    session_id: Uuid,
    auth: &AuthUser,
) -> Result<ShellSessionMeta, ApiError> {
    let meta = state
        .shell_sessions
        .get(&session_id)
        .map(|m| m.clone())
        .ok_or_else(|| {
            ApiError::NotFound(format!("Shell session {session_id} not found or already closed"))
        })?;

    if meta.owner_id != auth.user_id && !auth.is_admin() {
        return Err(ApiError::Forbidden(
            "You do not own this shell session".to_string(),
        ));
    }

    Ok(meta)
}

/// Send input to an active shell session.
///
/// POST /api/v1/shell/:session_id/input
/// Body: { "data": "ls -la\n" }
async fn send_input(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(session_id): Path<Uuid>,
    Json(body): Json<ShellInputBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let meta = authorize_session(&state, session_id, &auth)?;

    let sent = state
        .send_to_agent(
            &meta.device_id,
            ServerMessage::ShellInput {
                session_id,
                data: body.data,
            },
        )
        .await;

    if !sent {
        return Err(ApiError::NotFound(format!(
            "Device {} is not connected",
            meta.device_id
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
async fn close_shell(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let meta = authorize_session(&state, session_id, &auth)?;

    let sent = state
        .send_to_agent(&meta.device_id, ServerMessage::CloseShell { session_id })
        .await;

    if !sent {
        return Err(ApiError::NotFound(format!(
            "Device {} is not connected",
            meta.device_id
        )));
    }

    state.shell_sessions.remove(&session_id);

    Ok(Json(serde_json::json!({
        "data": { "status": "closed" },
        "error": null,
    })))
}

/// Stream shell output via Server-Sent Events (SSE).
///
/// GET /api/v1/shell/:session_id/stream
///
/// Returns a text/event-stream that yields:
/// - `event: output` with shell stdout/stderr data
/// - `event: closed` with exit code when the session ends
async fn stream_shell(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(session_id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Ownership check — only the owner (or admin) may read shell output.
    authorize_session(&state, session_id, &auth)?;

    let rx = state
        .shell_broadcasts
        .get(&session_id)
        .map(|entry| entry.value().subscribe())
        .ok_or_else(|| {
            ApiError::NotFound(format!("Shell session {session_id} not found or already closed"))
        })?;

    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                if event.closed {
                    let data = serde_json::json!({
                        "exit_code": event.exit_code,
                    });
                    Some(Ok(Event::default()
                        .event("closed")
                        .data(data.to_string())))
                } else {
                    Some(Ok(Event::default()
                        .event("output")
                        .data(event.data)))
                }
            }
            Err(_) => {
                // Broadcast channel lagged or closed — end the stream.
                None
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
