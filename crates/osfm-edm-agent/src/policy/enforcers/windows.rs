//! Windows policy enforcer — enforces policy rules using OS-level commands (stub).
//!
//! ## Planned approach (not yet implemented)
//!
//! - **Firewall**: `netsh advfirewall set allprofiles state on/off`
//! - **USB storage**: Registry key `HKLM\SYSTEM\CurrentControlSet\Services\USBSTOR\Start` (3=disabled, 4=disabled)
//! - **Screen lock**: `powercfg /change monitor-timeout-ac <minutes>` + screensaver registry keys
//! - **Auto-updates**: `Set-MpPreference` / Windows Update registry keys
//!
//! All methods work from user-space with Administrator privileges.

use tracing::warn;

/// Enforce firewall policy (Windows — not yet implemented).
pub fn enforce_firewall(_enabled: bool) {
    warn!("Windows firewall enforcement not yet implemented (planned: netsh advfirewall)");
}

/// Enforce USB storage policy (Windows — not yet implemented).
pub fn enforce_usb_storage(_allow: bool) {
    warn!("Windows USB enforcement not yet implemented (planned: USBSTOR registry key)");
}

/// Enforce screen lock policy (Windows — not yet implemented).
pub fn enforce_screen_lock(_timeout_minutes: u32, _require_password: bool) {
    warn!("Windows screen lock enforcement not yet implemented (planned: powercfg)");
}

/// Enforce auto-update policy (Windows — not yet implemented).
pub fn enforce_auto_updates(_policy: &osfm_edm_common::policy::UpdatePolicy) {
    warn!("Windows auto-update enforcement not yet implemented (planned: Windows Update registry)");
}
