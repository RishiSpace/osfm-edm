//! Policy engine service — pushes policies to connected agents when assignments change.

use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use osfm_edm_common::protocol::ServerMessage;

use crate::state::AppState;

/// Push all assigned policies to a device that just connected.
pub async fn push_policies_to_device(state: &Arc<AppState>, device_id: Uuid) {
    #[derive(sqlx::FromRow)]
    struct PolicyRow {
        id: Uuid,
        name: String,
        rules: serde_json::Value,
        version: i32,
    }

    let policies: Vec<PolicyRow> = sqlx::query_as(
        "SELECT p.id, p.name, p.rules, p.version FROM policies p \
         JOIN policy_assignments pa ON p.id = pa.policy_id \
         WHERE (pa.device_id = $1 OR pa.group_id IN \
           (SELECT group_id FROM group_members WHERE device_id = $1)) \
         AND p.enabled = true",
    )
    .bind(device_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if policies.is_empty() {
        return;
    }

    let defs: Vec<osfm_edm_common::policy::PolicyDefinition> = policies
        .into_iter()
        .map(|p| osfm_edm_common::policy::PolicyDefinition {
            id: p.id,
            name: p.name,
            rules: serde_json::from_value(p.rules).unwrap_or_default(),
        })
        .collect();

    tracing::info!(device_id = %device_id, count = defs.len(), "Pushing policies to connected agent");

    state
        .send_to_agent(
            &device_id,
            ServerMessage::PushPolicy { policies: defs },
        )
        .await;
}
