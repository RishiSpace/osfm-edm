//! Groups API — device group CRUD and membership management.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the groups sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_groups).post(create_group))
        .route("/{id}", get(get_group).delete(delete_group))
        .route("/{id}/members", get(list_members).post(add_member))
        .route("/{id}/members/{device_id}", delete(remove_member))
}

// --- Row types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct GroupRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MemberRow {
    pub device_id: Uuid,
    pub hostname: String,
    pub os: String,
    pub status: String,
}

// --- Request types ---

#[derive(Debug, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub device_id: Uuid,
}

// --- Handlers ---

/// POST /api/v1/groups — create a new group.
async fn create_group(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(body): Json<CreateGroupRequest>,
) -> ApiResult<impl IntoResponse> {
    let group: GroupRow = sqlx::query_as(
        "INSERT INTO device_groups (name, description) VALUES ($1, $2) \
         RETURNING id, name, description, created_at",
    )
    .bind(&body.name)
    .bind(&body.description)
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": group, "error": null }))))
}

/// GET /api/v1/groups — list all groups with member counts.
async fn list_groups(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let groups: Vec<GroupRow> = sqlx::query_as(
        "SELECT id, name, description, created_at FROM device_groups ORDER BY name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": groups, "error": null })))
}

/// GET /api/v1/groups/:id — get group detail.
async fn get_group(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let group: GroupRow = sqlx::query_as(
        "SELECT id, name, description, created_at FROM device_groups WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Group {id} not found")))?;

    Ok(Json(serde_json::json!({ "data": group, "error": null })))
}

/// DELETE /api/v1/groups/:id — delete a group.
async fn delete_group(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let result = sqlx::query("DELETE FROM device_groups WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Group {id} not found")));
    }

    Ok(Json(serde_json::json!({ "data": { "message": "Group deleted" }, "error": null })))
}

/// GET /api/v1/groups/:id/members — list devices in a group.
async fn list_members(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let members: Vec<MemberRow> = sqlx::query_as(
        "SELECT d.id as device_id, d.hostname, d.os, d.status \
         FROM group_members gm JOIN devices d ON gm.device_id = d.id \
         WHERE gm.group_id = $1 ORDER BY d.hostname",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": members, "error": null })))
}

/// POST /api/v1/groups/:id/members — add a device to a group.
async fn add_member(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> ApiResult<impl IntoResponse> {
    sqlx::query(
        "INSERT INTO group_members (group_id, device_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(body.device_id)
    .execute(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": { "message": "Member added" }, "error": null }))))
}

/// DELETE /api/v1/groups/:id/members/:device_id — remove a device from a group.
async fn remove_member(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path((id, device_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND device_id = $2")
        .bind(id)
        .bind(device_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "data": { "message": "Member removed" }, "error": null })))
}
