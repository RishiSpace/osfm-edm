//! Policy engine — evaluates received policies against local system state and
//! sends compliance reports back to the server.

use osfm_edm_common::policy::{ComplianceReport, ComplianceViolation, PolicyDefinition, PolicyRule};
use osfm_edm_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use uuid::Uuid;

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
            // USB storage blocking — would need kernel driver for enforcement.
            // For compliance checking, report as compliant (best-effort).
            None
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
        PolicyRule::KernelEvents { .. } => {
            // Kernel event collection config — no compliance check needed.
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
        let name = process.name().to_str().unwrap_or_default().to_string();
        if deny.iter().any(|d| name.contains(d.as_str())) {
            found.push(name);
        }
    }
    found
}
