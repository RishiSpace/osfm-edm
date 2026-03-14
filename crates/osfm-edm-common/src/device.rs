//! Device types — core device model and status enums.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Operating system type of a managed device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsType {
    Windows,
    Linux,
    Macos,
}

impl std::fmt::Display for OsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsType::Windows => write!(f, "windows"),
            OsType::Linux => write!(f, "linux"),
            OsType::Macos => write!(f, "macos"),
        }
    }
}

/// Current connection status of a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Online,
    Offline,
    Stale,
}

impl std::fmt::Display for DeviceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceStatus::Online => write!(f, "online"),
            DeviceStatus::Offline => write!(f, "offline"),
            DeviceStatus::Stale => write!(f, "stale"),
        }
    }
}

/// Full device record as stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: Uuid,
    pub hostname: String,
    pub os: OsType,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub ip_address: Option<String>,
    pub agent_version: Option<String>,
    pub enrolled_at: Option<i64>,
    pub last_seen: Option<i64>,
    pub status: DeviceStatus,
}

/// Request body for enrolling a new device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentRequest {
    pub token: String,
    pub hostname: String,
    pub os: OsType,
    pub os_version: Option<String>,
    pub arch: Option<String>,
}

/// Response returned after successful enrollment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    pub device_id: Uuid,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub server_url: String,
}
