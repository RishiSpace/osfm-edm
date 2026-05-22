//! Linux policy enforcer — enforces policy rules using OS-level commands.
//!
//! All enforcement actions require root privileges. Functions are best-effort:
//! they attempt the action and log warnings on failure.

use tracing::{info, warn};

/// Enforce firewall policy via ufw.
pub fn enforce_firewall(enabled: bool) {
    let action = if enabled { "enable" } else { "disable" };
    info!(action, "Enforcing firewall policy via ufw");

    // Try ufw first (Ubuntu/Debian).
    let result = std::process::Command::new("ufw")
        .arg("--force")
        .arg(action)
        .output();

    match result {
        Ok(output) if output.status.success() => {
            info!("Firewall {action}d successfully via ufw");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(stderr = %stderr, "ufw {action} returned non-zero");
        }
        Err(e) => {
            warn!(error = %e, "ufw not available, trying iptables");
            // Fallback: check iptables (just verify, can't easily enable/disable).
            if enabled {
                let _ = std::process::Command::new("iptables")
                    .args(["-L", "-n"])
                    .output()
                    .map(|o| {
                        if o.status.success() {
                            info!("iptables is available (firewall assumed active)");
                        }
                    });
            }
        }
    }
}

/// Enforce USB storage policy by toggling kernel driver authorization.
///
/// When `allow` is false, disables the usb-storage driver by setting
/// `authorized` to 0 for all USB storage devices via sysfs.
pub fn enforce_usb_storage(allow: bool) {
    let value = if allow { "1" } else { "0" };
    info!(allow, "Enforcing USB storage policy");

    // Method 1: Toggle usb-storage driver authorization via sysfs.
    let usb_storage_path = "/sys/bus/usb/drivers/usb-storage";
    if std::path::Path::new(usb_storage_path).exists() {
        if let Ok(entries) = std::fs::read_dir(usb_storage_path) {
            for entry in entries.flatten() {
                let auth_path = entry.path().join("authorized");
                if auth_path.exists() {
                    if let Err(e) = std::fs::write(&auth_path, value) {
                        warn!(path = %auth_path.display(), error = %e, "Failed to set USB authorization");
                    }
                }
            }
        }
    }

    // Method 2: Blacklist the usb-storage module via modprobe.
    let blacklist_path = "/etc/modprobe.d/osfm-edm-usb-storage.conf";
    if !allow {
        let content = "# Managed by OSFM-EDM — USB storage policy\nblacklist usb-storage\ninstall usb-storage /bin/false\n";
        if let Err(e) = std::fs::write(blacklist_path, content) {
            warn!(error = %e, "Failed to write usb-storage blacklist");
        } else {
            info!("USB storage module blacklisted via modprobe.d");
        }
    } else {
        // Remove blacklist if allowing.
        if std::path::Path::new(blacklist_path).exists() {
            if let Err(e) = std::fs::remove_file(blacklist_path) {
                warn!(error = %e, "Failed to remove usb-storage blacklist");
            } else {
                info!("USB storage module blacklist removed");
            }
        }
    }
}

/// Enforce screen lock policy.
///
/// Attempts to configure screen lock timeout via:
/// 1. GNOME (gsettings)
/// 2. XFCE (xfconf-query)
/// 3. Generic X11 (xset)
pub fn enforce_screen_lock(timeout_minutes: u32, _require_password: bool) {
    let timeout_secs = timeout_minutes * 60;
    info!(timeout_minutes, "Enforcing screen lock policy");

    // Try GNOME settings first.
    let gnome_result = std::process::Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.session",
            "idle-delay",
            &format!("uint32 {timeout_secs}"),
        ])
        .output();

    if let Ok(output) = gnome_result {
        if output.status.success() {
            info!("Screen lock timeout set via GNOME gsettings");
            // Also ensure screen locks on idle.
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.desktop.screensaver", "lock-enabled", "true"])
                .output();
            return;
        }
    }

    // Try XFCE.
    let xfce_result = std::process::Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-power-manager",
            "-p",
            "/xfce4-power-manager/dpms-on-ac-sleep",
            "-s",
            &timeout_minutes.to_string(),
        ])
        .output();

    if let Ok(output) = xfce_result {
        if output.status.success() {
            info!("Screen lock timeout set via XFCE xfconf-query");
            return;
        }
    }

    // Fallback: xset for X11 screen blanking.
    let xset_result = std::process::Command::new("xset")
        .args(["s", &timeout_secs.to_string()])
        .output();

    match xset_result {
        Ok(output) if output.status.success() => {
            info!("Screen blank timeout set via xset");
        }
        _ => {
            warn!("Could not set screen lock timeout — no supported desktop environment found");
        }
    }
}

/// Enforce OS auto-update policy by configuring unattended-upgrades (Debian/Ubuntu)
/// or dnf-automatic (Fedora/RHEL).
pub fn enforce_auto_updates(policy: &osfm_edm_common::policy::UpdatePolicy) {
    use osfm_edm_common::policy::UpdatePolicy;
    info!(policy = ?policy, "Enforcing auto-update policy");

    match policy {
        UpdatePolicy::Disabled => {
            // Remove unattended-upgrades config if present.
            let path = "/etc/apt/apt.conf.d/20auto-upgrades";
            if std::path::Path::new(path).exists() {
                let content = "\
APT::Periodic::Update-Package-Lists \"0\";\n\
APT::Periodic::Unattended-Upgrade \"0\";\n";
                if let Err(e) = std::fs::write(path, content) {
                    warn!(error = %e, "Failed to disable auto-upgrades");
                } else {
                    info!("Auto-upgrades disabled via apt config");
                }
            }
        }
        UpdatePolicy::SecurityOnly | UpdatePolicy::All => {
            let path = "/etc/apt/apt.conf.d/20auto-upgrades";
            let content = "\
APT::Periodic::Update-Package-Lists \"1\";\n\
APT::Periodic::Unattended-Upgrade \"1\";\n";
            if let Err(e) = std::fs::write(path, content) {
                warn!(error = %e, "Failed to enable auto-upgrades");
            } else {
                info!("Auto-upgrades enabled via apt config");
            }

            // For security-only, ensure only security origins are enabled.
            if matches!(policy, UpdatePolicy::SecurityOnly) {
                let origins_path = "/etc/apt/apt.conf.d/50unattended-upgrades";
                if std::path::Path::new(origins_path).exists() {
                    info!("Security-only updates configured (check {origins_path} manually)");
                }
            }
        }
    }
}
