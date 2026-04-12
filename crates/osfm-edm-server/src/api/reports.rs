//! Reports API — compliance summary and per-device reports.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the reports sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/compliance", get(fleet_compliance))
        .route("/compliance/{device_id}", get(device_compliance))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ComplianceRow {
    pub id: Uuid,
    pub device_id: Uuid,
    pub policy_id: Uuid,
    pub compliant: bool,
    pub detail: Option<serde_json::Value>,
    pub reported_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// GET /api/v1/reports/compliance — fleet-wide compliance summary.
async fn fleet_compliance(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM compliance_reports")
        .fetch_one(&state.db)
        .await?;

    let compliant: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_reports WHERE compliant = true",
    )
    .fetch_one(&state.db)
    .await?;

    let non_compliant = total - compliant;

    let recent: Vec<ComplianceRow> = sqlx::query_as(
        "SELECT id, device_id, policy_id, compliant, detail, reported_at \
         FROM compliance_reports WHERE compliant = false \
         ORDER BY reported_at DESC LIMIT 20",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "total_evaluations": total,
            "compliant": compliant,
            "non_compliant": non_compliant,
            "compliance_rate": if total > 0 { (compliant as f64 / total as f64) * 100.0 } else { 100.0 },
            "recent_violations": recent,
        },
        "error": null
    })))
}

/// GET /api/v1/reports/compliance/:device_id — per-device compliance.
async fn device_compliance(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
    Path(device_id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let reports: Vec<ComplianceRow> = sqlx::query_as(
        "SELECT id, device_id, policy_id, compliant, detail, reported_at \
         FROM compliance_reports WHERE device_id = $1 ORDER BY reported_at DESC",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await?;

    let compliant_count = reports.iter().filter(|r| r.compliant).count();
    let total = reports.len();

    Ok(Json(serde_json::json!({
        "data": {
            "device_id": device_id,
            "total_policies": total,
            "compliant": compliant_count,
            "non_compliant": total - compliant_count,
            "reports": reports,
        },
        "error": null
    })))
}
