//! Devices API — device CRUD and telemetry queries.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the devices sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_devices))
        .route("/{id}", get(get_device))
        .route("/{id}", patch(update_device))
        .route("/{id}", delete(delete_device))
        .route("/{id}/telemetry", get(get_telemetry))
}

// --- Row types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DeviceRow {
    pub id: Uuid,
    pub hostname: String,
    pub os: String,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub ip_address: Option<String>,
    pub agent_version: Option<String>,
    pub enrolled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MetricRow {
    pub device_id: Uuid,
    pub time: chrono::DateTime<chrono::Utc>,
    pub cpu_pct: Option<f64>,
    pub ram_used_mb: Option<i64>,
    pub ram_total_mb: Option<i64>,
    pub disk_used_gb: Option<f64>,
    pub disk_total_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
}

// --- Request types ---

#[derive(Debug, Deserialize)]
pub struct UpdateDeviceRequest {
    pub hostname: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelemetryQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Handlers ---

/// GET /api/v1/devices — list all devices.
async fn list_devices(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let devices: Vec<DeviceRow> = sqlx::query_as(
        "SELECT id, hostname, os, os_version, arch, ip_address, agent_version, enrolled_at, last_seen, status FROM devices ORDER BY enrolled_at DESC"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": devices,
        "error": null,
    })))
}

/// GET /api/v1/devices/:id — get device detail.
async fn get_device(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let device: DeviceRow = sqlx::query_as(
        "SELECT id, hostname, os, os_version, arch, ip_address, agent_version, enrolled_at, last_seen, status FROM devices WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Device {id} not found")))?;

    Ok(Json(serde_json::json!({
        "data": device,
        "error": null,
    })))
}

/// PATCH /api/v1/devices/:id — update device hostname.
async fn update_device(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateDeviceRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Verify device exists.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    if !exists {
        return Err(ApiError::NotFound(format!("Device {id} not found")));
    }

    if let Some(hostname) = &body.hostname {
        sqlx::query("UPDATE devices SET hostname = $1 WHERE id = $2")
            .bind(hostname)
            .bind(id)
            .execute(&state.db)
            .await?;
    }

    // Return updated device.
    let device: DeviceRow = sqlx::query_as(
        "SELECT id, hostname, os, os_version, arch, ip_address, agent_version, enrolled_at, last_seen, status FROM devices WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": device,
        "error": null,
    })))
}

/// DELETE /api/v1/devices/:id — revoke device certificate (soft delete).
async fn delete_device(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    if !exists {
        return Err(ApiError::NotFound(format!("Device {id} not found")));
    }

    // Revoke the device's certificate rather than deleting data.
    sqlx::query("UPDATE certificates SET revoked = true, revoked_at = now() WHERE device_id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    // Mark device as offline.
    sqlx::query("UPDATE devices SET status = 'offline' WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    tracing::info!(device_id = %id, "Device certificate revoked");

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": { "message": "Device revoked" },
            "error": null,
        })),
    ))
}

/// GET /api/v1/devices/:id/telemetry — query device metrics with time range.
async fn get_telemetry(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(params): Query<TelemetryQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let from = params
        .from
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::hours(24));
    let to = params.to.unwrap_or_else(chrono::Utc::now);

    let metrics: Vec<MetricRow> = sqlx::query_as(
        "SELECT device_id, time, cpu_pct, ram_used_mb, ram_total_mb, disk_used_gb, disk_total_gb, uptime_secs \
         FROM device_metrics WHERE device_id = $1 AND time >= $2 AND time <= $3 \
         ORDER BY time ASC"
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": metrics,
        "error": null,
    })))
}
