//! Patches API — query patch/update status across the fleet.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the patches sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/device/:device_id", get(device_patch_status))
        .route("/summary", get(fleet_patch_summary))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PatchRow {
    patch_id: String,
    title: Option<String>,
    severity: Option<String>,
    status: String,
    detected_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /api/v1/patches/device/:device_id — patches reported for a device.
async fn device_patch_status(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let patches: Vec<PatchRow> = sqlx::query_as(
        "SELECT patch_id, title, severity, status, detected_at \
         FROM patches WHERE device_id = $1 ORDER BY detected_at DESC LIMIT 500",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?;

    let pending = patches.iter().filter(|p| p.status == "pending").count();

    Ok(Json(serde_json::json!({
        "data": {
            "device_id": device_id,
            "pending_count": pending,
            "patches": patches,
        },
        "error": null
    })))
}

/// GET /api/v1/patches/summary — fleet-wide patch summary.
async fn fleet_patch_summary(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let total_devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&state.db)
        .await?;

    let devices_with_pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT device_id) FROM patches WHERE status = 'pending'",
    )
    .fetch_one(&state.db)
    .await?;

    let pending_by_severity: Vec<(Option<String>, i64)> = sqlx::query_as(
        "SELECT severity, COUNT(*) FROM patches WHERE status = 'pending' GROUP BY severity",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "total_devices": total_devices,
            "devices_with_pending_patches": devices_with_pending,
            "pending_by_severity": pending_by_severity
                .into_iter()
                .map(|(sev, count)| serde_json::json!({ "severity": sev, "count": count }))
                .collect::<Vec<_>>(),
        },
        "error": null
    })))
}
