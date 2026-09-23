//! Policy engine — evaluates received policies against local system state and
//! sends compliance reports back to the server. After evaluation, attempts
//! enforcement on supported platforms (Linux).

use osfm_edm_common::policy::{
    ComplianceReport, ComplianceViolation, PolicyDefinition, PolicyRule,
};
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
                #[cfg(target_os = "windows")]
                enforcers::windows::enforce_firewall(true);
                #[cfg(target_os = "macos")]
                enforcers::macos::enforce_firewall(true);
                #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
                tracing::debug!("Firewall enforcement not supported on this platform");
            }
        }
        PolicyRule::UsbStorage { allow } => {
            #[cfg(target_os = "linux")]
            enforcers::linux::enforce_usb_storage(*allow);
            #[cfg(target_os = "windows")]
            enforcers::windows::enforce_usb_storage(*allow);
            #[cfg(target_os = "macos")]
            enforcers::macos::enforce_usb_storage(*allow);
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            tracing::debug!("USB storage enforcement not supported on this platform");
        }
        PolicyRule::ScreenLock {
            timeout_minutes,
            require_password,
        } => {
            if *timeout_minutes > 0 || *require_password {
                #[cfg(target_os = "linux")]
                enforcers::linux::enforce_screen_lock(*timeout_minutes, *require_password);
                #[cfg(target_os = "windows")]
                enforcers::windows::enforce_screen_lock(*timeout_minutes, *require_password);
                #[cfg(target_os = "macos")]
                enforcers::macos::enforce_screen_lock(*timeout_minutes, *require_password);
                #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
                tracing::debug!("Screen lock enforcement not supported on this platform");
            }
        }
        PolicyRule::OsUpdate { auto_install, .. } => {
            #[cfg(target_os = "linux")]
            enforcers::linux::enforce_auto_updates(auto_install);
            #[cfg(target_os = "windows")]
            enforcers::windows::enforce_auto_updates(auto_install);
            #[cfg(target_os = "macos")]
            enforcers::macos::enforce_auto_updates(auto_install);
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            tracing::debug!("Auto-update enforcement not supported on this platform");
        }
        PolicyRule::ProcessBlacklist { deny } => {
            // Process blacklist is a monitoring rule — we can kill blacklisted processes.
            if !deny.is_empty() {
                kill_blacklisted_processes(deny);
            }
        }
        PolicyRule::SystemEvents { .. } => {
            // System event collection config — no enforcement action needed.
        }
    }
}

/// Kill any currently running blacklisted processes (sysinfo is cross-platform).
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
            if !enabled {
                return None;
            }
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
            if *allow {
                return None;
            }
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
        PolicyRule::ScreenLock {
            timeout_minutes,
            require_password,
        } => {
            if *timeout_minutes == 0 && !require_password {
                return None;
            }
            match check_screen_lock(*timeout_minutes, *require_password) {
                Ok(()) => None,
                Err(message) => Some(ComplianceViolation {
                    rule_type: "screen_lock".to_string(),
                    message,
                }),
            }
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
            if deny.is_empty() {
                return None;
            }
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

/// Fail closed: if we cannot prove the lock policy, report a violation.
fn check_screen_lock(timeout_minutes: u32, require_password: bool) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        return check_screen_lock_linux(timeout_minutes, require_password);
    }
    if cfg!(target_os = "windows") {
        return check_screen_lock_windows(timeout_minutes, require_password);
    }
    if cfg!(target_os = "macos") {
        return check_screen_lock_macos(timeout_minutes, require_password);
    }
    Err("screen lock check is not implemented on this platform".into())
}

fn check_screen_lock_linux(timeout_minutes: u32, require_password: bool) -> Result<(), String> {
    let max_idle = timeout_minutes.saturating_mul(60);
    if let Ok(out) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.session", "idle-delay"])
        .output()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let secs = text
            .split_whitespace()
            .rev()
            .find_map(|t| t.parse::<u32>().ok());
        if let Some(secs) = secs {
            if timeout_minutes > 0 && secs > max_idle {
                return Err(format!("idle-delay is {secs}s, policy max {max_idle}s"));
            }
        }
        if require_password {
            let lock = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.screensaver", "lock-enabled"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("true"))
                .unwrap_or(false);
            if !lock {
                return Err("screensaver lock-enabled is not true".into());
            }
        }
        return Ok(());
    }
    Err("could not read screen-lock settings (gsettings missing)".into())
}

/// Windows: verify the screensaver timeout + password-protect settings via reg.
#[cfg(target_os = "windows")]
fn check_screen_lock_windows(timeout_minutes: u32, require_password: bool) -> Result<(), String> {
    let max_secs = timeout_minutes.saturating_mul(60);
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Control Panel\Desktop",
            "/v",
            "ScreenSaveTimeOut",
        ])
        .output()
        .map_err(|_| "could not query ScreenSaveTimeOut".to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let secs = text.split_whitespace().rev().find_map(|t| {
        t.trim_start_matches("0x").parse::<u32>().ok().or_else(|| {
            if t.starts_with("0x") {
                u32::from_str_radix(t.trim_start_matches("0x"), 16).ok()
            } else {
                None
            }
        })
    });
    if let Some(secs) = secs {
        if timeout_minutes > 0 && secs > max_secs {
            return Err(format!(
                "ScreenSaveTimeOut is {secs}s, policy max {max_secs}s"
            ));
        }
    }
    if require_password {
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Control Panel\Desktop",
                "/v",
                "ScreenSaverIsSecure",
            ])
            .output()
            .map_err(|_| "could not query ScreenSaverIsSecure".to_string())?;
        if !String::from_utf8_lossy(&out.stdout).contains('1') {
            return Err("ScreenSaverIsSecure is not 1".into());
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn check_screen_lock_windows(_timeout_minutes: u32, _require_password: bool) -> Result<(), String> {
    Err("not on Windows".into())
}

/// macOS: verify `askForPassword` + `askForPasswordDelay` via defaults.
#[cfg(target_os = "macos")]
fn check_screen_lock_macos(timeout_minutes: u32, require_password: bool) -> Result<(), String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.screensaver", "askForPassword"])
        .output()
        .map_err(|_| "could not read askForPassword".to_string())?;
    if require_password && !String::from_utf8_lossy(&out.stdout).contains('1') {
        return Err("askForPassword is not 1".into());
    }
    let out = std::process::Command::new("defaults")
        .args(["read", "com.apple.screensaver", "askForPasswordDelay"])
        .output()
        .map_err(|_| "could not read askForPasswordDelay".to_string())?;
    let secs = String::from_utf8_lossy(out.stdout)
        .split_whitespace()
        .find_map(|t| t.parse::<u32>().ok())
        .unwrap_or(u32::MAX);
    let max_secs = timeout_minutes.saturating_mul(60);
    if timeout_minutes > 0 && secs > max_secs {
        return Err(format!(
            "askForPasswordDelay is {secs}s, policy max {max_secs}s"
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn check_screen_lock_macos(_timeout_minutes: u32, _require_password: bool) -> Result<(), String> {
    Err("not on macOS".into())
}

/// Check if firewall is enabled.
fn check_firewall_enabled() -> bool {
    if cfg!(target_os = "linux") {
        return std::process::Command::new("ufw")
            .arg("status")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("active"))
            .unwrap_or(false);
    }
    if cfg!(target_os = "windows") {
        return std::process::Command::new("netsh")
            .args(["advfirewall", "show", "allprofiles", "state"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("ON"))
            .unwrap_or(false);
    }
    if cfg!(target_os = "macos") {
        return std::process::Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .args(["--getglobalstate"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("enabled"))
            .unwrap_or(false);
    }
    true
}

/// Check if USB storage is blocked.
fn check_usb_storage_loaded() -> bool {
    if cfg!(target_os = "linux") {
        return std::process::Command::new("lsmod")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("usb_storage"))
            .unwrap_or(false);
    }
    if cfg!(target_os = "windows") {
        // USBSTOR Start=4 means disabled (not loaded); anything else counts as loaded.
        return std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR",
                "/v",
                "Start",
            ])
            .output()
            .map(|o| !String::from_utf8_lossy(&o.stdout).contains("0x4"))
            .unwrap_or(true);
    }
    false
}

/// Check if auto-updates are configured.
fn check_auto_updates() -> bool {
    if cfg!(target_os = "linux") {
        return std::path::Path::new("/etc/apt/apt.conf.d/20auto-upgrades").exists();
    }
    if cfg!(target_os = "windows") {
        return std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
                "/v",
                "AUOptions",
            ])
            .output()
            .map(|o| {
                let t = String::from_utf8_lossy(&o.stdout);
                t.contains("0x3") || t.contains("0x4")
            })
            .unwrap_or(false);
    }
    if cfg!(target_os = "macos") {
        return std::process::Command::new("defaults")
            .args([
                "read",
                "/Library/Preferences/com.apple.SoftwareUpdate",
                "AutomaticCheckEnabled",
            ])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains('1'))
            .unwrap_or(false);
    }
    true
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
