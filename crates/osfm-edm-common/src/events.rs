//! Kernel events — types for eBPF (Linux) and KMDF (Windows) event monitoring.

use serde::{Deserialize, Serialize};

/// A kernel-level event captured by the host driver infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KernelEvent {
    ProcessStarted {
        pid: u32,
        ppid: u32,
        path: String,
        cmdline: String,
        user: Option<String>,
        timestamp: i64,
    },
    ProcessExited {
        pid: u32,
        exit_code: i32,
        timestamp: i64,
    },
    FileAccessed {
        pid: u32,
        path: String,
        operation: FileOperation,
        timestamp: i64,
    },
    NetworkConnected {
        pid: u32,
        src: String,
        dst: String,
        protocol: NetworkProtocol,
        timestamp: i64,
    },
    RegistryChanged {
        pid: u32,
        key: String,
        operation: RegistryOperation,
        timestamp: i64,
    },
}

/// File system operation types tracked by kernel monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    Read,
    Write,
    Create,
    Delete,
    Rename,
}

/// Network protocol for connection events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocol {
    Tcp,
    Udp,
}

/// Windows registry operation types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryOperation {
    Create,
    Modify,
    Delete,
}
