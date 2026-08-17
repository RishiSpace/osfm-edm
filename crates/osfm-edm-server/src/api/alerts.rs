//! Alerts API — alert rule CRUD and event management.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post, delete};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the alerts sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/:id", get(get_rule).patch(update_rule).delete(delete_rule))
        .route("/events", get(list_events))
        .route("/events/:id/resolve", post(resolve_event))
}

// --- Row types ---

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AlertRuleRow {
    id: Uuid,
    name: String,
    metric: Option<String>,
    operator: Option<String>,
    threshold: Option<f64>,
    severity: Option<String>,
    condition: serde_json::Value,
    channels: serde_json::Value,
    enabled: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AlertEventRow {
    id: Uuid,
    rule_id: Uuid,
    device_id: Option<Uuid>,
    severity: Option<String>,
    message: Option<String>,
    triggered_at: Option<chrono::DateTime<chrono::Utc>>,
    resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Request types ---

#[derive(Debug, Deserialize)]
struct CreateRuleRequest {
    name: String,
    /// Metric name: "cpu_pct", "ram_pct", or "disk_pct".
    metric: String,
    /// Comparison operator: ">", ">=", "<", "<=", "==".
    operator: String,
    /// Threshold value to compare against.
    threshold: f64,
    /// Alert severity: "critical", "warning", "info".
    severity: Option<String>,
    /// Notification channels: { "email": "...", "webhook": "...", "ntfy": "..." }.
    channels: Option<serde_json::Value>,
    /// Whether the rule is enabled (default: true).
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateRuleRequest {
    name: Option<String>,
    metric: Option<String>,
    operator: Option<String>,
    threshold: Option<f64>,
    severity: Option<String>,
    channels: Option<serde_json::Value>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListEventsQuery {
    /// Filter events by device_id.
    device_id: Option<Uuid>,
    /// Filter events by severity.
    severity: Option<String>,
    /// Only show unresolved events.
    unresolved: Option<bool>,
    /// Maximum number of events to return (default: 100).
    limit: Option<i64>,
    /// Offset for pagination.
    offset: Option<i64>,
}

// --- Handlers ---

/// POST /api/v1/alerts/rules — create a new alert rule.
async fn create_rule(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateRuleRequest>,
) -> ApiResult<impl IntoResponse> {
    auth.require_admin()?;

    let severity = body.severity.unwrap_or_else(|| "warning".to_string());
    let channels = body.channels.unwrap_or_else(|| serde_json::json!({}));
    let enabled = body.enabled.unwrap_or(true);

    // Store the structured condition as JSONB for the original schema column.
    let condition = serde_json::json!({
        "metric": body.metric,
        "operator": body.operator,
        "threshold": body.threshold,
    });

    let rule: AlertRuleRow = sqlx::query_as(
        "INSERT INTO alert_rules (name, metric, operator, threshold, severity, condition, channels, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id, name, metric, operator, threshold, severity, condition, channels, enabled, created_at",
    )
    .bind(&body.name)
    .bind(&body.metric)
    .bind(&body.operator)
    .bind(body.threshold)
    .bind(&severity)
    .bind(&condition)
    .bind(&channels)
    .bind(enabled)
    .fetch_one(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": rule, "error": null })),
    ))
}

/// GET /api/v1/alerts/rules — list all alert rules.
async fn list_rules(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT id, name, metric, operator, threshold, severity, condition, channels, enabled, created_at \
         FROM alert_rules ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({ "data": rules, "error": null })))
}

/// GET /api/v1/alerts/rules/:id — get alert rule detail.
async fn get_rule(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule: AlertRuleRow = sqlx::query_as(
        "SELECT id, name, metric, operator, threshold, severity, condition, channels, enabled, created_at \
         FROM alert_rules WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("Alert rule {id} not found")))?;

    Ok(Json(serde_json::json!({ "data": rule, "error": null })))
}

/// PATCH /api/v1/alerts/rules/:id — update an alert rule.
async fn update_rule(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRuleRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_admin()?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM alert_rules WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.db)
        .await?;

    if !exists {
        return Err(ApiError::NotFound(format!("Alert rule {id} not found")));
    }

    if let Some(name) = &body.name {
        sqlx::query("UPDATE alert_rules SET name = $1 WHERE id = $2")
            .bind(name).bind(id).execute(&state.db).await?;
    }
    if let Some(metric) = &body.metric {
        sqlx::query("UPDATE alert_rules SET metric = $1 WHERE id = $2")
            .bind(metric).bind(id).execute(&state.db).await?;
    }
    if let Some(operator) = &body.operator {
        sqlx::query("UPDATE alert_rules SET operator = $1 WHERE id = $2")
            .bind(operator).bind(id).execute(&state.db).await?;
    }
    if let Some(threshold) = body.threshold {
        sqlx::query("UPDATE alert_rules SET threshold = $1 WHERE id = $2")
            .bind(threshold).bind(id).execute(&state.db).await?;
    }
    if let Some(severity) = &body.severity {
        sqlx::query("UPDATE alert_rules SET severity = $1 WHERE id = $2")
            .bind(severity).bind(id).execute(&state.db).await?;
    }
    if let Some(channels) = &body.channels {
        sqlx::query("UPDATE alert_rules SET channels = $1 WHERE id = $2")
            .bind(channels).bind(id).execute(&state.db).await?;
    }
    if let Some(enabled) = body.enabled {
        sqlx::query("UPDATE alert_rules SET enabled = $1 WHERE id = $2")
            .bind(enabled).bind(id).execute(&state.db).await?;
    }

    // Also update the condition JSONB to stay in sync.
    let rule: AlertRuleRow = sqlx::query_as(
        "SELECT id, name, metric, operator, threshold, severity, condition, channels, enabled, created_at \
         FROM alert_rules WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    // Sync the condition JSONB with the individual columns.
    if rule.metric.is_some() || rule.operator.is_some() || rule.threshold.is_some() {
        let condition = serde_json::json!({
            "metric": rule.metric,
            "operator": rule.operator,
            "threshold": rule.threshold,
        });
        sqlx::query("UPDATE alert_rules SET condition = $1 WHERE id = $2")
            .bind(&condition).bind(id).execute(&state.db).await?;
    }

    Ok(Json(serde_json::json!({ "data": rule, "error": null })))
}

/// DELETE /api/v1/alerts/rules/:id — delete an alert rule.
async fn delete_rule(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<impl IntoResponse> {
    auth.require_admin()?;

    let result = sqlx::query("DELETE FROM alert_rules WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("Alert rule {id} not found")));
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "data": { "message": "Alert rule deleted" }, "error": null })),
    ))
}

/// GET /api/v1/alerts/events — list triggered alert events with optional filters.
async fn list_events(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Query(params): Query<ListEventsQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);

    // Build a dynamic query with optional filters.
    let mut conditions = vec!["TRUE".to_string()];
    let mut bind_index = 1u32;

    if params.device_id.is_some() {
        conditions.push(format!("ae.device_id = ${bind_index}"));
        bind_index += 1;
    }
    if params.severity.is_some() {
        conditions.push(format!("ae.severity = ${bind_index}"));
        bind_index += 1;
    }
    if params.unresolved.unwrap_or(false) {
        conditions.push("ae.resolved_at IS NULL".to_string());
    }

    let where_clause = conditions.join(" AND ");
    let query_str = format!(
        "SELECT ae.id, ae.rule_id, ae.device_id, ae.severity, ae.message, ae.triggered_at, ae.resolved_at \
         FROM alert_events ae \
         WHERE {where_clause} \
         ORDER BY ae.triggered_at DESC NULLS LAST \
         LIMIT ${bind_index} OFFSET ${}",
        bind_index + 1
    );

    // We need to use sqlx::query_as with dynamic binding.
    let mut query = sqlx::query_as::<_, AlertEventRow>(&query_str);
    if let Some(device_id) = params.device_id {
        query = query.bind(device_id);
    }
    if let Some(severity) = &params.severity {
        query = query.bind(severity.clone());
    }
    query = query.bind(limit).bind(offset);

    let events: Vec<AlertEventRow> = query.fetch_all(&state.db).await?;

    Ok(Json(serde_json::json!({ "data": events, "error": null })))
}

/// POST /api/v1/alerts/events/:id/resolve — resolve a triggered alert event.
async fn resolve_event(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    auth.require_admin()?;

    let result = sqlx::query(
        "UPDATE alert_events SET resolved_at = now() WHERE id = $1 AND resolved_at IS NULL",
    )
    .bind(id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!(
            "Alert event {id} not found or already resolved"
        )));
    }

    Ok(Json(serde_json::json!({ "data": { "message": "Alert event resolved" }, "error": null })))
}
