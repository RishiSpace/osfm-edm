//! Jobs API — job creation, dispatch, listing, and log retrieval.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use osfm_edm_common::protocol::ServerMessage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the jobs sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_jobs).post(create_job))
        .route("/{id}", get(get_job))
        .route("/{id}/cancel", post(cancel_job))
}

// --- Row types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub device_id: Uuid,
    pub payload: serde_json::Value,
    pub status: String,
    pub exit_code: Option<i32>,
    pub log_output: Option<serde_json::Value>,
    pub created_by: Option<Uuid>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Request types ---

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub device_id: Uuid,
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    pub device_id: Option<Uuid>,
    pub status: Option<String>,
}

// --- Handlers ---

/// POST /api/v1/jobs — create and dispatch a job.
async fn create_job(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateJobRequest>,
) -> ApiResult<impl IntoResponse> {
    // Verify device exists.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(body.device_id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(ApiError::NotFound(format!("Device {} not found", body.device_id)));
    }

    // Insert the job.
    let job: JobRow = sqlx::query_as(
        "INSERT INTO jobs (device_id, payload, status, created_by) VALUES ($1, $2, 'pending', $3) \
         RETURNING id, device_id, payload, status, exit_code, log_output, created_by, created_at, completed_at",
    )
    .bind(body.device_id)
    .bind(&body.payload)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await?;

    // Try to dispatch immediately if agent is connected.
    let payload: osfm_edm_common::jobs::JobPayload =
        serde_json::from_value(body.payload.clone()).map_err(|e| {
            ApiError::BadRequest(format!("Invalid job payload: {e}"))
        })?;

    let dispatched = state
        .send_to_agent(
            &body.device_id,
            ServerMessage::DispatchJob {
                job_id: job.id,
                payload,
                signature: String::new(), // TODO: Ed25519 signing
            },
        )
        .await;

    if dispatched {
        let _ = sqlx::query("UPDATE jobs SET status = 'dispatched' WHERE id = $1")
            .bind(job.id)
            .execute(&state.db)
            .await;
        tracing::info!(job_id = %job.id, device_id = %body.device_id, "Job dispatched to agent");
    } else {
        tracing::info!(job_id = %job.id, device_id = %body.device_id, "Job queued — agent offline");
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": job, "error": null })),
    ))
}

/// GET /api/v1/jobs — list jobs with optional filters.
async fn list_jobs(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(params): Query<ListJobsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let jobs: Vec<JobRow> = if let Some(device_id) = params.device_id {
        sqlx::query_as(
            "SELECT id, device_id, payload, status, exit_code, log_output, created_by, created_at, completed_at \
             FROM jobs WHERE device_id = $1 ORDER BY created_at DESC LIMIT 100",
        )
        .bind(device_id)
        .fetch_all(&state.db)
        .await?
    } else if let Some(status) = &params.status {
        sqlx::query_as(
            "SELECT id, device_id, payload, status, exit_code, log_output, created_by, created_at, completed_at \
             FROM jobs WHERE status = $1 ORDER BY created_at DESC LIMIT 100",
        )
        .bind(status)
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, device_id, payload, status, exit_code, log_output, created_by, created_at, completed_at \
             FROM jobs ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(serde_json::json!({ "data": jobs, "error": null })))
}

/// GET /api/v1/jobs/:id — get job detail with logs.
async fn get_job(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let job: JobRow = sqlx::query_as(
        "SELECT id, device_id, payload, status, exit_code, log_output, created_by, created_at, completed_at \
         FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Job {id} not found")))?;

    Ok(Json(serde_json::json!({ "data": job, "error": null })))
}

/// POST /api/v1/jobs/:id/cancel — cancel/revoke a running job.
async fn cancel_job(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    // Get the job to find its device.
    #[derive(sqlx::FromRow)]
    struct JobDevice {
        device_id: Uuid,
        status: String,
    }

    let job: JobDevice = sqlx::query_as("SELECT device_id, status FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Job {id} not found")))?;

    if job.status == "completed" || job.status == "failed" || job.status == "cancelled" {
        return Err(ApiError::Conflict(format!("Job is already {}", job.status)));
    }

    // Send revocation to agent.
    state
        .send_to_agent(&job.device_id, ServerMessage::RevokeJob { job_id: id })
        .await;

    let _ = sqlx::query("UPDATE jobs SET status = 'cancelled', completed_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;

    Ok(Json(serde_json::json!({ "data": { "message": "Job cancelled" }, "error": null })))
}
