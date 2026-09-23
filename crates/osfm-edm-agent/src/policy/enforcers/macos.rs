//! macOS policy enforcer — enforces policy rules using built-in macOS tools.
//!
//! All enforcement is best-effort and logs on failure. Requires admin/root.

use tracing::{info, warn};

fn run(cmd: &str, args: &[&str]) -> bool {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            warn!(cmd, stderr = %String::from_utf8_lossy(&o.stderr), "command returned non-zero");
            false
        }
        Err(e) => {
            warn!(cmd, error = %e, "command not available");
            false
        }
    }
}

/// Enforce firewall policy via socketfilterfw (Application Firewall).
pub fn enforce_firewall(enabled: bool) {
    info!(enabled, "Enforcing macOS firewall via socketfilterfw");
    let fw = "/usr/libexec/ApplicationFirewall/socketfilterfw";
    run(
        fw,
        &["--setglobalstate", if enabled { "on" } else { "off" }],
    );
    if enabled {
        run(fw, &["--setstealthmode", "on"]);
    }
}

/// Enforce USB storage policy via an IOUSBMassStorageClass kext preference.
/// `allow=false` writes a disabled personality; `allow=true` removes it.
pub fn enforce_usb_storage(allow: bool) {
    info!(allow, "Enforcing macOS USB storage policy");
    let plist = "/Library/Preferences/com.osfm-edm.usb-storage.plist";
    if allow {
        if std::path::Path::new(plist).exists() {
            if let Err(e) = std::fs::remove_file(plist) {
                warn!(error = %e, "Failed to remove USB storage restriction");
            }
        }
    } else if let Err(e) = std::fs::write(
        plist,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict><key>Disabled</key><true/></dict></plist>\n",
    ) {
        warn!(error = %e, "Failed to write USB storage restriction");
    }
}

/// Enforce screen lock policy via pmset + screensaver defaults.
pub fn enforce_screen_lock(timeout_minutes: u32, require_password: bool) {
    info!(timeout_minutes, "Enforcing macOS screen lock policy");
    let mins = timeout_minutes.max(1).to_string();
    run("pmset", &["displaysleep", &mins]);
    if require_password {
        run(
            "defaults",
            &[
                "write",
                "com.apple.screensaver",
                "askForPassword",
                "-int",
                "1",
            ],
        );
        let delay = "5";
        run(
            "defaults",
            &[
                "write",
                "com.apple.screensaver",
                "askForPasswordDelay",
                "-int",
                delay,
            ],
        );
    }
}

/// Enforce auto-update policy via SoftwareUpdate defaults.
pub fn enforce_auto_updates(policy: &osfm_edm_common::policy::UpdatePolicy) {
    use osfm_edm_common::policy::UpdatePolicy;
    info!(policy = ?policy, "Enforcing macOS SoftwareUpdate policy");
    let prefs = "/Library/Preferences/com.apple.SoftwareUpdate";
    let (check, download, install) = match policy {
        UpdatePolicy::Disabled => ("0", "0", "0"),
        UpdatePolicy::SecurityOnly => ("1", "1", "0"),
        UpdatePolicy::All => ("1", "1", "1"),
    };
    run(
        "defaults",
        &[
            "write",
            prefs,
            "AutomaticCheckEnabled",
            "-bool",
            if check == "1" { "true" } else { "false" },
        ],
    );
    run(
        "defaults",
        &[
            "write",
            prefs,
            "AutomaticDownload",
            "-bool",
            if download == "1" { "true" } else { "false" },
        ],
    );
    run(
        "defaults",
        &[
            "write",
            prefs,
            "AutomaticallyInstallMacOSXUpdates",
            "-bool",
            if install == "1" { "true" } else { "false" },
        ],
    );
}
