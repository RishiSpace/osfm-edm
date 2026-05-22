//! System monitor — user-space process, file, and network event monitoring.
//!
//! Provides real-time system event collection without kernel drivers.
//! Uses platform-specific APIs:
//! - **Linux**: procfs, netlink proc connector, fanotify
//! - **Windows**: ETW (Event Tracing for Windows), Win32 APIs
//! - **macOS**: Endpoint Security framework

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

use osfm_edm_common::events::SystemEvent;
use tokio::sync::mpsc;

/// Configuration for the system monitor.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Whether monitoring is enabled.
    pub enabled: bool,
    /// How often to flush event batches to the server (seconds).
    pub batch_interval_secs: u64,
    /// Paths to monitor for file events (Linux: fanotify mount points).
    pub monitor_paths: Vec<String>,
    /// Which event categories to collect: "process", "file", "network".
    pub collect: Vec<String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            batch_interval_secs: 5,
            monitor_paths: vec!["/".to_string()],
            collect: vec![
                "process".to_string(),
                "file".to_string(),
                "network".to_string(),
            ],
        }
    }
}

/// Start the system monitor and return a receiver for collected events.
///
/// Events are batched internally and flushed at the configured interval.
/// The caller should drain the receiver and forward batches to the server.
pub async fn start(config: MonitorConfig) -> mpsc::Receiver<Vec<SystemEvent>> {
    let (tx, rx) = mpsc::channel::<Vec<SystemEvent>>(64);

    if !config.enabled {
        tracing::info!("System monitor disabled by configuration");
        return rx;
    }

    #[cfg(target_os = "linux")]
    {
        tokio::spawn(linux::run_monitor(config, tx));
    }

    #[cfg(target_os = "windows")]
    {
        tokio::spawn(windows::run_monitor(config, tx));
    }

    #[cfg(target_os = "macos")]
    {
        tokio::spawn(macos::run_monitor(config, tx));
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        tracing::warn!("System monitoring not supported on this platform");
    }

    rx
}
