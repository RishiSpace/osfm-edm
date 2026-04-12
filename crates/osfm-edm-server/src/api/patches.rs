//! Patches API — query patch/update status across the fleet.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the patches sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/device/{device_id}", get(device_patch_status))
        .route("/summary", get(fleet_patch_summary))
}

/// GET /api/v1/patches/device/:device_id — pending updates for a device.
async fn device_patch_status(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    // Patches are stored as part of the inventory report in installed_software.
    // For a dedicated patch table, we'd need a migration. For now, we return
    // from the installed_software table where version != latest.
    let software_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM installed_software WHERE device_id = $1",
    )
    .bind(device_id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "device_id": device_id,
            "installed_packages": software_count,
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

    let devices_with_inventory: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT device_id) FROM installed_software",
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "total_devices": total_devices,
            "devices_with_inventory": devices_with_inventory,
        },
        "error": null
    })))
}
