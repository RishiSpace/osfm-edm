//! Policy types — policy definitions, rules, and compliance reporting.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A complete policy definition containing one or more enforceable rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDefinition {
    pub id: Uuid,
    pub name: String,
    pub rules: Vec<PolicyRule>,
}

/// Individual policy rules that can be enforced on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyRule {
    ScreenLock {
        timeout_minutes: u32,
        require_password: bool,
    },
    Firewall {
        enabled: bool,
    },
    OsUpdate {
        auto_install: UpdatePolicy,
        reboot_window: Option<String>,
    },
    ProcessBlacklist {
        deny: Vec<String>,
    },
    UsbStorage {
        allow: bool,
    },
    SystemEvents {
        /// Which event types to collect (e.g., "process", "file", "network").
        collect: Vec<String>,
        /// File/directory paths to monitor (default: ["/"]).
        monitor_paths: Option<Vec<String>>,
        /// How often to flush event batches to the server, in seconds (default: 5).
        batch_interval_secs: Option<u64>,
    },
}

/// OS update auto-install policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    Disabled,
    SecurityOnly,
    All,
}

/// Result of evaluating a device's compliance against a policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub device_id: Uuid,
    pub policy_id: Uuid,
    pub compliant: bool,
    pub violations: Vec<ComplianceViolation>,
    pub checked_at: i64,
}

/// A single rule violation within a compliance check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceViolation {
    pub rule_type: String,
    pub message: String,
}
