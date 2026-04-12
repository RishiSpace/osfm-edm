//! Job queue service — dispatches pending jobs to connected agents.

use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use osfm_edm_common::protocol::ServerMessage;

use crate::state::AppState;

/// Dispatch all pending jobs for a device that just connected.
/// Called when an agent connects via WebSocket.
pub async fn dispatch_pending_jobs(state: &Arc<AppState>, device_id: Uuid) {
    #[derive(sqlx::FromRow)]
    struct PendingJob {
        id: Uuid,
        payload: serde_json::Value,
    }

    let jobs: Vec<PendingJob> = sqlx::query_as(
        "SELECT id, payload FROM jobs WHERE device_id = $1 AND status = 'pending' ORDER BY created_at",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for job in jobs {
        let payload: osfm_edm_common::jobs::JobPayload =
            match serde_json::from_value(job.payload) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(job_id = %job.id, error = %e, "Invalid job payload");
                    continue;
                }
            };

        let sent = state
            .send_to_agent(
                &device_id,
                ServerMessage::DispatchJob {
                    job_id: job.id,
                    payload,
                    signature: String::new(),
                },
            )
            .await;

        if sent {
            let _ = sqlx::query("UPDATE jobs SET status = 'dispatched' WHERE id = $1")
                .bind(job.id)
                .execute(&state.db)
                .await;
            tracing::info!(job_id = %job.id, device_id = %device_id, "Dispatched pending job");
        }
    }
}
