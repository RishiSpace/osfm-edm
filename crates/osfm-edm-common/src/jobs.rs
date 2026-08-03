//! Job types — payloads for remote command execution, software management, and maintenance.

use serde::{Deserialize, Serialize};

/// Payload describing what a job should do on the target device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobPayload {
    RunScript {
        shell: ShellType,
        script: String,
    },
    InstallPackage {
        manager: PackageManager,
        package: String,
        version: Option<String>,
    },
    UninstallPackage {
        manager: PackageManager,
        package: String,
    },
    PushFile {
        destination: String,
        content_b64: String,
        permissions: Option<String>,
    },
    Reboot {
        delay_seconds: u32,
    },
    CollectInventory,
    RunPatchUpdate {
        patch_ids: Vec<String>,
    },
}

/// Shell interpreter for script execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellType {
    Bash,
    Powershell,
    Sh,
    Cmd,
}

/// Package manager used for software install/uninstall operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageManager {
    Winget,
    Chocolatey,
    Homebrew,
    Apt,
    Dnf,
    Pacman,
}

/// Domain separator prepended to job signing bytes so signatures cannot be
/// replayed across contexts.
const JOB_SIGN_DOMAIN: &[u8] = b"OSFM-EDM-JOB-SIGN-v1\0";

/// Canonical byte representation that the server signs (Ed25519) and the agent
/// verifies before executing a dispatched job.
///
/// Layout: `DOMAIN || job_id (16 raw bytes) || serde_json(payload)`.
/// Deterministic because `JobPayload` contains no map types — struct field
/// order is fixed by serde.
pub fn canonical_job_signing_bytes(job_id: &uuid::Uuid, payload: &JobPayload) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(JOB_SIGN_DOMAIN);
    bytes.extend_from_slice(job_id.as_bytes());
    bytes.extend_from_slice(&serde_json::to_vec(payload).unwrap_or_default());
    bytes
}

/// Current execution status of a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Done => write!(f, "done"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}
