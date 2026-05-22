//! macOS system monitor — user-space event collection via Endpoint Security framework.
//!
//! ## Planned approach (not yet implemented)
//!
//! - **Process events**: Endpoint Security `ES_EVENT_TYPE_NOTIFY_EXEC` and
//!   `ES_EVENT_TYPE_NOTIFY_EXIT` for real-time process lifecycle events.
//!
//! - **File events**: Endpoint Security `ES_EVENT_TYPE_NOTIFY_OPEN`,
//!   `ES_EVENT_TYPE_NOTIFY_WRITE`, `ES_EVENT_TYPE_NOTIFY_UNLINK`, etc.
//!
//! - **Network events**: `NetworkExtension` framework or Endpoint Security
//!   `ES_EVENT_TYPE_NOTIFY_CONNECT` for socket-level connection events.
//!
//! Requires a System Extension with the Endpoint Security entitlement
//! and user approval via System Preferences → Privacy & Security.

use osfm_edm_common::events::SystemEvent;
use tokio::sync::mpsc;

use super::MonitorConfig;

/// Run the macOS system monitor (not yet implemented).
pub async fn run_monitor(config: MonitorConfig, tx: mpsc::Sender<Vec<SystemEvent>>) {
    tracing::warn!(
        "macOS system monitor not yet implemented — \
         planned: Endpoint Security framework for process/file/network events"
    );

    // Keep the task alive so the channel stays open.
    let _ = (config, tx);
    std::future::pending::<()>().await;
}
