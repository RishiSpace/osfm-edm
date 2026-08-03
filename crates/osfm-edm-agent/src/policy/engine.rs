//! Policy engine — evaluates received policies against local system state and
//! sends compliance reports back to the server. After evaluation, attempts
//! enforcement on supported platforms (Linux).

use osfm_edm_common::policy::{ComplianceReport, ComplianceViolation, PolicyDefinition, PolicyRule};
use osfm_edm_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::enforcers;

/// Evaluate a set of policies against the current system state.
pub async fn evaluate_policies(
    device_id: Uuid,
    policies: Vec<PolicyDefinition>,
    outbound_tx: &mpsc::Sender<AgentMessage>,
) {
    let mut reports = Vec::new();

    for policy in &policies {
        let violations = evaluate_single_policy(policy);
        let compliant = violations.is_empty();

        // If non-compliant, attempt to enforce the rules.
        if !compliant {
            tracing::info!(
                policy_id = %policy.id,
                policy_name = %policy.name,
                violations = violations.len(),
                "Non-compliant — attempting enforcement"
            );
            enforce_policy_rules(&policy.rules);
        }

        reports.push(ComplianceReport {
            device_id,
            policy_id: policy.id,
            compliant,
            violations,
            checked_at: chrono::Utc::now().timestamp(),
        });
    }

    if !reports.is_empty() {
        tracing::info!(
            device_id = %device_id,
            count = reports.len(),
            "Sending compliance reports"
        );
        let _ = outbound_tx
            .send(AgentMessage::ComplianceReport { reports })
            .await;
    }
}

/// Evaluate a single policy — returns a list of violations (empty = compliant).
fn evaluate_single_policy(policy: &PolicyDefinition) -> Vec<ComplianceViolation> {
    let mut violations = Vec::new();

    for rule in &policy.rules {
        if let Some(violation) = check_rule(rule) {
            violations.push(violation);
        }
    }

    violations
}

/// Attempt enforcement of all rules in a policy via platform-specific enforcers.
fn enforce_policy_rules(rules: &[PolicyRule]) {
    for rule in rules {
        enforce_rule(rule);
    }
}

/// Attempt to enforce a single policy rule using platform-specific mechanisms.
fn enforce_rule(rule: &PolicyRule) {
    match rule {
        PolicyRule::Firewall { enabled } => {
            if *enabled {
                #[cfg(target_os = "linux")]
                enforcers::linux::enforce_firewall(true);
                #[cfg(not(target_os = "linux"))]
                tracing::debug!("Firewall enforcement not supported on this platform");
            }
        }
        PolicyRule::UsbStorage { allow } => {
            #[cfg(target_os = "linux")]
            enforcers::linux::enforce_usb_storage(*allow);
            #[cfg(not(target_os = "linux"))]
            tracing::debug!("USB storage enforcement not supported on this platform");
        }
        PolicyRule::ScreenLock { timeout_minutes, require_password } => {
            if *timeout_minutes > 0 || *require_password {
                #[cfg(target_os = "linux")]
                enforcers::linux::enforce_screen_lock(*timeout_minutes, *require_password);
                #[cfg(not(target_os = "linux"))]
                tracing::debug!("Screen lock enforcement not supported on this platform");
            }
        }
        PolicyRule::OsUpdate { auto_install, .. } => {
            #[cfg(target_os = "linux")]
            enforcers::linux::enforce_auto_updates(auto_install);
            #[cfg(not(target_os = "linux"))]
            tracing::debug!("Auto-update enforcement not supported on this platform");
        }
        PolicyRule::ProcessBlacklist { deny } => {
            // Process blacklist is a monitoring rule — we can kill blacklisted processes.
            if !deny.is_empty() {
                #[cfg(target_os = "linux")]
                kill_blacklisted_processes(deny);
            }
        }
        PolicyRule::SystemEvents { .. } => {
            // System event collection config — no enforcement action needed.
        }
    }
}

/// Kill any currently running blacklisted processes (Linux only).
#[cfg(target_os = "linux")]
fn kill_blacklisted_processes(deny: &[String]) {
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        let name = process.name().to_string();
        if deny.iter().any(|d| name.contains(d.as_str())) {
            tracing::warn!(
                pid = process.pid().as_u32(),
                name = %name,
                "Killing blacklisted process"
            );
            process.kill();
        }
    }
}

/// Check a single policy rule against the local system. Returns None if compliant.
fn check_rule(rule: &PolicyRule) -> Option<ComplianceViolation> {
    match rule {
        PolicyRule::Firewall { enabled } => {
            if !enabled { return None; }
            if !check_firewall_enabled() {
                Some(ComplianceViolation {
                    rule_type: "firewall".to_string(),
                    message: "Firewall is not active".to_string(),
                })
            } else {
                None
            }
        }
        PolicyRule::UsbStorage { allow } => {
            if *allow { return None; }
            // Check if usb-storage module is loaded.
            if check_usb_storage_loaded() {
                Some(ComplianceViolation {
                    rule_type: "usb_storage".to_string(),
                    message: "USB storage is not blocked".to_string(),
                })
            } else {
                None
            }
        }
        PolicyRule::ScreenLock { timeout_minutes, require_password } => {
            if *timeout_minutes == 0 && !require_password { return None; }
            // Screen lock compliance — platform-specific checks would go here.
            None
        }
        PolicyRule::OsUpdate { auto_install, .. } => {
            // Check if auto-updates are configured (Linux: unattended-upgrades).
            match auto_install {
                osfm_edm_common::policy::UpdatePolicy::Disabled => None,
                _ => {
                    if !check_auto_updates() {
                        Some(ComplianceViolation {
                            rule_type: "os_update".to_string(),
                            message: "Automatic updates not configured".to_string(),
                        })
                    } else {
                        None
                    }
                }
            }
        }
        PolicyRule::ProcessBlacklist { deny } => {
            if deny.is_empty() { return None; }
            let running = check_blacklisted_processes(deny);
            if !running.is_empty() {
                Some(ComplianceViolation {
                    rule_type: "process_blacklist".to_string(),
                    message: format!("Blacklisted processes running: {}", running.join(", ")),
                })
            } else {
                None
            }
        }
        PolicyRule::SystemEvents { .. } => {
            // System event collection config — no compliance check needed.
            None
        }
    }
}

/// Check if firewall is enabled on Linux.
fn check_firewall_enabled() -> bool {
    if cfg!(target_os = "linux") {
        std::process::Command::new("ufw")
            .arg("status")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("active"))
            .unwrap_or(false)
    } else {
        true
    }
}

/// Check if USB storage module is loaded (Linux).
fn check_usb_storage_loaded() -> bool {
    if cfg!(target_os = "linux") {
        std::process::Command::new("lsmod")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("usb_storage"))
            .unwrap_or(false)
    } else {
        false
    }
}

/// Check if auto-updates are configured.
fn check_auto_updates() -> bool {
    if cfg!(target_os = "linux") {
        std::path::Path::new("/etc/apt/apt.conf.d/20auto-upgrades").exists()
    } else {
        true
    }
}

/// Check if any blacklisted processes are currently running.
fn check_blacklisted_processes(deny: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        let name = process.name().to_string();
        if deny.iter().any(|d| name.contains(d.as_str())) {
            found.push(name);
        }
    }
    found
}

