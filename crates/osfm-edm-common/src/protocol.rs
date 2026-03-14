//! Protocol types — WebSocket message envelopes for agent ↔ server communication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::events::KernelEvent;
use crate::jobs::JobPayload;
use crate::policy::{ComplianceReport, PolicyDefinition};

/// Messages sent from the server to an agent over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Keepalive ping from server.
    Heartbeat,
    /// Push updated policy definitions to the agent.
    PushPolicy {
        policies: Vec<PolicyDefinition>,
    },
    /// Dispatch a signed job for the agent to execute.
    DispatchJob {
        job_id: Uuid,
        payload: JobPayload,
        signature: String,
    },
    /// Cancel a previously dispatched job.
    RevokeJob {
        job_id: Uuid,
    },
    /// Request the agent to send a telemetry snapshot immediately.
    RequestTelemetry,
    /// Request the agent to send a software/patch inventory.
    RequestInventory,
}

/// Messages sent from an agent to the server over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msg_type", rename_all = "snake_case")]
pub enum AgentMessage {
    /// Heartbeat response with agent version.
    Heartbeat {
        agent_version: String,
    },
    /// System telemetry snapshot (CPU, RAM, disk, uptime).
    TelemetryReport {
        snapshot: TelemetrySnapshot,
    },
    /// Batch of kernel events from the driver infrastructure.
    KernelEventBatch {
        events: Vec<KernelEvent>,
    },
    /// A single log line from a running job.
    JobLog {
        job_id: Uuid,
        line: String,
        stream: String,
    },
    /// Notification that a job has finished executing.
    JobCompleted {
        job_id: Uuid,
        exit_code: i32,
    },
    /// Compliance evaluation results for assigned policies.
    ComplianceReport {
        reports: Vec<ComplianceReport>,
    },
    /// Software and patch inventory from the device.
    InventoryReport {
        software: Vec<SoftwareItem>,
        patches: Vec<PatchItem>,
    },
}

/// Point-in-time system resource snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub cpu_pct: f64,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub uptime_secs: u64,
    pub timestamp: i64,
}

/// A software package installed on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareItem {
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
    pub install_date: Option<String>,
}

/// A pending or installed patch on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchItem {
    pub patch_id: String,
    pub title: Option<String>,
    pub severity: Option<String>,
    pub status: String,
}
