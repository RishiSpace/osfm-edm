//! System events — types for user-space system monitoring (process, file, network, registry).
//!
//! On Linux, events are collected via procfs, netlink proc connector, and fanotify.
//! On Windows, events are collected via ETW and Win32 APIs.
//! On macOS, events are collected via the Endpoint Security framework.

use serde::{Deserialize, Serialize};

/// A system-level event captured by the user-space monitoring infrastructure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SystemEvent {
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

/// File system operation types tracked by system monitoring.
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
