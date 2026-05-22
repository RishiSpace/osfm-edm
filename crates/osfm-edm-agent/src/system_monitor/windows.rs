//! Windows system monitor — user-space event collection via ETW and Win32 APIs.
//!
//! ## Planned approach (not yet implemented)
//!
//! - **Process events**: ETW provider `Microsoft-Windows-Kernel-Process` for
//!   real-time process create/exit notifications with full command line.
//!   Alternative: WMI `Win32_ProcessStartTrace` / `Win32_ProcessStopTrace`.
//!
//! - **File events**: ETW provider `Microsoft-Windows-Kernel-File` or
//!   `ReadDirectoryChangesW` for directory-level file change notifications.
//!
//! - **Network events**: ETW provider `Microsoft-Windows-Kernel-Network` for
//!   TCP/UDP connection events with PID attribution.
//!
//! - **Registry events**: `RegNotifyChangeKeyValue` Win32 API for watching
//!   specific registry keys, or ETW `Microsoft-Windows-Kernel-Registry`.
//!
//! All methods work from user-space with Administrator privileges.

use osfm_edm_common::events::SystemEvent;
use tokio::sync::mpsc;

use super::MonitorConfig;

/// Run the Windows system monitor (not yet implemented).
pub async fn run_monitor(config: MonitorConfig, tx: mpsc::Sender<Vec<SystemEvent>>) {
    tracing::warn!(
        "Windows system monitor not yet implemented — \
         planned: ETW for process/file/network, RegNotifyChangeKeyValue for registry"
    );

    // Keep the task alive so the channel stays open.
    let _ = (config, tx);
    std::future::pending::<()>().await;
}
