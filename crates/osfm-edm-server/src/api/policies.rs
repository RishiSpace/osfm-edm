//! Policies API — policy CRUD and assignment to devices/groups.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the policies sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_policies).post(create_policy))
        .route("/{id}", get(get_policy).patch(update_policy).delete(delete_policy))
        .route("/{id}/assign", post(assign_policy))
        .route("/{id}/unassign", post(unassign_policy))
}

// --- Row types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PolicyRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub rules: serde_json::Value,
    pub version: i32,
    pub enabled: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Request types ---

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub rules: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub rules: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AssignPolicyRequest {
    pub device_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
}

// --- Handlers ---

/// POST /api/v1/policies — create a new policy.
async fn create_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Json(body): Json<CreatePolicyRequest>,
) -> ApiResult<impl IntoResponse> {
    let policy: PolicyRow = sqlx::query_as(
        "INSERT INTO policies (name, description, rules) VALUES ($1, $2, $3) \
         RETURNING id, name, description, rules, version, enabled, created_at, updated_at",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.rules)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": policy, "error": null })),
    ))
}

/// GET /api/v1/policies — list all policies.
async fn list_policies(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let policies: Vec<PolicyRow> = sqlx::query_as(
        "SELECT id, name, description, rules, version, enabled, created_at, updated_at FROM policies ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": policies, "error": null })))
}

/// GET /api/v1/policies/:id — get policy detail.
async fn get_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let policy: PolicyRow = sqlx::query_as(
        "SELECT id, name, description, rules, version, enabled, created_at, updated_at FROM policies WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Policy {id} not found")))?;

    Ok(Json(serde_json::json!({ "data": policy, "error": null })))
}

/// PATCH /api/v1/policies/:id — update policy.
async fn update_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePolicyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Check exists.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM policies WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if !exists {
        return Err(ApiError::NotFound(format!("Policy {id} not found")));
    }

    if let Some(name) = &body.name {
        sqlx::query("UPDATE policies SET name = $1, updated_at = now() WHERE id = $2")
            .bind(name).bind(id).execute(&state.db).await?;
    }
    if let Some(desc) = &body.description {
        sqlx::query("UPDATE policies SET description = $1, updated_at = now() WHERE id = $2")
            .bind(desc).bind(id).execute(&state.db).await?;
    }
    if let Some(rules) = &body.rules {
        sqlx::query("UPDATE policies SET rules = $1, version = version + 1, updated_at = now() WHERE id = $2")
            .bind(rules).bind(id).execute(&state.db).await?;
    }
    if let Some(enabled) = body.enabled {
        sqlx::query("UPDATE policies SET enabled = $1, updated_at = now() WHERE id = $2")
            .bind(enabled).bind(id).execute(&state.db).await?;
    }

    // Return updated.
    let policy: PolicyRow = sqlx::query_as(
        "SELECT id, name, description, rules, version, enabled, created_at, updated_at FROM policies WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    // Push updated policies to connected agents assigned to this policy.
    push_policy_to_agents(&state, id).await;

    Ok(Json(serde_json::json!({ "data": policy, "error": null })))
}

/// DELETE /api/v1/policies/:id — delete policy.
async fn delete_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    let result = sqlx::query("DELETE FROM policies WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Policy {id} not found")));
    }

    Ok((StatusCode::OK, Json(serde_json::json!({ "data": { "message": "Policy deleted" }, "error": null }))))
}

/// POST /api/v1/policies/:id/assign — assign policy to a device or group.
async fn assign_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignPolicyRequest>,
) -> ApiResult<impl IntoResponse> {
    sqlx::query(
        "INSERT INTO policy_assignments (policy_id, device_id, group_id) VALUES ($1, $2, $3) \
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(body.device_id)
    .bind(body.group_id)
    .execute(&state.db)
    .await?;

    // Push the policy to the target agent if connected.
    push_policy_to_agents(&state, id).await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": { "message": "Policy assigned" }, "error": null }))))
}

/// POST /api/v1/policies/:id/unassign — remove policy assignment.
async fn unassign_policy(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignPolicyRequest>,
) -> ApiResult<impl IntoResponse> {
    sqlx::query(
        "DELETE FROM policy_assignments WHERE policy_id = $1 AND \
         (device_id = $2 OR group_id = $3)",
    )
    .bind(id)
    .bind(body.device_id)
    .bind(body.group_id)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": { "message": "Policy unassigned" }, "error": null })))
}

/// Push the policy to all agents that are assigned to it and currently connected.
async fn push_policy_to_agents(state: &AppState, policy_id: Uuid) {
    // Get the policy definition.
    let policy: Option<PolicyRow> = sqlx::query_as(
        "SELECT id, name, description, rules, version, enabled, created_at, updated_at FROM policies WHERE id = $1 AND enabled = true",
    )
    .bind(policy_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let Some(policy) = policy else { return };

    // Build a PolicyDefinition from the row.
    let policy_def = osfm_edm_common::policy::PolicyDefinition {
        id: policy.id,
        name: policy.name,
        rules: serde_json::from_value(policy.rules).unwrap_or_default(),
    };

    // Find all device_ids assigned to this policy.
    let device_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT device_id FROM policy_assignments WHERE policy_id = $1 AND device_id IS NOT NULL \
         UNION \
         SELECT DISTINCT gm.device_id FROM policy_assignments pa \
         JOIN group_members gm ON pa.group_id = gm.group_id \
         WHERE pa.policy_id = $1",
    )
    .bind(policy_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let msg = osfm_edm_common::protocol::ServerMessage::PushPolicy {
        policies: vec![policy_def],
    };

    for did in device_ids {
        state.send_to_agent(&did, msg.clone()).await;
    }
}
