//! Windows policy enforcer — enforces policy rules using built-in OS commands.
//!
//! All enforcement is best-effort and logs on failure. Requires Administrator.

use tracing::{info, warn};

/// Enforce firewall policy via netsh.
pub fn enforce_firewall(enabled: bool) {
    let state = if enabled { "on" } else { "off" };
    info!(state, "Enforcing Windows firewall via netsh");
    match std::process::Command::new("netsh")
        .args(["advfirewall", "set", "allprofiles", "state", state])
        .output()
    {
        Ok(o) if o.status.success() => info!("Windows firewall set {state}"),
        Ok(o) => warn!(stderr = %String::from_utf8_lossy(&o.stderr), "netsh returned non-zero"),
        Err(e) => warn!(error = %e, "netsh not available"),
    }
}

/// Enforce USB storage policy via the USBSTOR service Start value
/// (3 = manual/allowed, 4 = disabled).
pub fn enforce_usb_storage(allow: bool) {
    let value = if allow { "3" } else { "4" };
    info!(allow, "Enforcing Windows USB storage policy");
    match std::process::Command::new("reg")
        .args([
            "add",
            r"HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR",
            "/v",
            "Start",
            "/t",
            "REG_DWORD",
            "/d",
            value,
            "/f",
        ])
        .output()
    {
        Ok(o) if o.status.success() => info!("USBSTOR Start={value}"),
        Ok(o) => warn!(stderr = %String::from_utf8_lossy(&o.stderr), "reg returned non-zero"),
        Err(e) => warn!(error = %e, "reg not available"),
    }
}

/// Enforce screen lock policy via powercfg + screensaver registry keys.
pub fn enforce_screen_lock(timeout_minutes: u32, require_password: bool) {
    info!(timeout_minutes, "Enforcing Windows screen lock policy");
    let mins = timeout_minutes.max(1).to_string();
    for args in [
        vec!["/change", "monitor-timeout-ac", &mins],
        vec!["/change", "monitor-timeout-dc", &mins],
    ] {
        if let Err(e) = std::process::Command::new("powercfg").args(&args).output() {
            warn!(error = %e, "powercfg not available");
        }
    }
    let timeout_secs = (timeout_minutes.max(1) * 60).to_string();
    let keys: Vec<(&str, &str, &str)> = vec![
        (
            r"HKCU\Control Panel\Desktop",
            "ScreenSaveTimeOut",
            timeout_secs.as_str(),
        ),
        (
            r"HKCU\Control Panel\Desktop",
            "ScreenSaverIsSecure",
            if require_password { "1" } else { "0" },
        ),
    ];
    for (key, name, data) in keys {
        match std::process::Command::new("reg")
            .args(["add", key, "/v", name, "/t", "REG_SZ", "/d", data, "/f"])
            .output()
        {
            Ok(o) if o.status.success() => {}
            Ok(o) => warn!(stderr = %String::from_utf8_lossy(&o.stderr), "reg returned non-zero"),
            Err(e) => warn!(error = %e, "reg not available"),
        }
    }
}

/// Enforce auto-update policy via the Windows Update AU registry key.
pub fn enforce_auto_updates(policy: &osfm_edm_common::policy::UpdatePolicy) {
    use osfm_edm_common::policy::UpdatePolicy;
    info!(policy = ?policy, "Enforcing Windows Update policy");
    let au_options = match policy {
        UpdatePolicy::Disabled => "1",
        UpdatePolicy::SecurityOnly => "3",
        UpdatePolicy::All => "4",
    };
    match std::process::Command::new("reg")
        .args([
            "add",
            r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
            "/v",
            "AUOptions",
            "/t",
            "REG_DWORD",
            "/d",
            au_options,
            "/f",
        ])
        .output()
    {
        Ok(o) if o.status.success() => info!("Windows Update AUOptions={au_options}"),
        Ok(o) => warn!(stderr = %String::from_utf8_lossy(&o.stderr), "reg returned non-zero"),
        Err(e) => warn!(error = %e, "reg not available"),
    }
}
