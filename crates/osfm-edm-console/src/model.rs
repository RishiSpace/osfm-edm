//! API DTOs — field names match the server JSON envelope.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub data: Option<T>,
    pub error: Option<ApiErrBody>,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginData {
    pub access_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub totp_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub hostname: String,
    pub os: String,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub agent_version: Option<String>,
    pub last_seen: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metric {
    pub time: String,
    pub cpu_pct: Option<f64>,
    pub ram_used_mb: Option<i64>,
    pub ram_total_mb: Option<i64>,
    pub disk_used_gb: Option<f64>,
    pub disk_total_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub device_id: Uuid,
    pub payload: serde_json::Value,
    pub status: String,
    pub exit_code: Option<i32>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    #[serde(default)]
    pub logs: Vec<JobLog>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JobLog {
    pub line: String,
    pub stream: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub rules: serde_json::Value,
    pub version: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupMember {
    pub device_id: Uuid,
    pub hostname: String,
    pub os: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub metric: Option<String>,
    pub operator: Option<String>,
    pub threshold: Option<f64>,
    pub severity: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertEvent {
    pub id: Uuid,
    pub message: Option<String>,
    pub severity: Option<String>,
    pub triggered_at: Option<String>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SoftwareItem {
    pub name: String,
    pub version: Option<String>,
    pub publisher: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevicePatches {
    pub pending_count: i64,
    pub patches: Vec<PatchItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchItem {
    pub patch_id: String,
    pub title: Option<String>,
    pub status: String,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerStatus {
    pub total_devices: i64,
    pub online_devices: i64,
    pub connected_agents: i64,
    pub total_policies: i64,
    pub pending_jobs: i64,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComplianceFleet {
    pub compliance_rate: f64,
    pub compliant: i64,
    pub non_compliant: i64,
    pub recent_violations: Vec<ComplianceRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComplianceRow {
    pub device_id: Uuid,
    pub policy_id: Uuid,
    pub compliant: bool,
    pub reported_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server_url: String,
    pub server_port: u16,
    pub tls_configured: bool,
    pub ca_initialized: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnrollToken {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShellOpen {
    pub session_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct LoginReq<'a> {
    pub username: &'a str,
    pub password: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_code: Option<&'a str>,
}
