//! Software API — query installed software per device.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the software sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/device/{device_id}", get(list_device_software))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SoftwareRow {
    pub id: Uuid,
    pub device_id: Uuid,
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
}

/// GET /api/v1/software/device/:device_id — list installed software on a device.
async fn list_device_software(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let software: Vec<SoftwareRow> = sqlx::query_as(
        "SELECT id, device_id, name, version, publisher, install_date \
         FROM installed_software WHERE device_id = $1 ORDER BY name",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": software, "error": null })))
}
