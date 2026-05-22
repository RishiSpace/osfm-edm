//! Agent configuration — loaded from ~/.osfm_edm/config.toml.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Agent configuration persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Server URL for the WebSocket connection.
    pub server_url: String,
    /// Unique device ID assigned during enrollment.
    pub device_id: Uuid,
    /// Path to the device's TLS certificate.
    pub cert_path: String,
    /// Path to the device's TLS private key.
    pub key_path: String,
    /// Path to the server CA certificate.
    pub ca_path: String,
    /// Heartbeat interval in seconds.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    /// Telemetry interval in seconds.
    #[serde(default = "default_telemetry_interval")]
    pub telemetry_interval: u64,
    /// Whether the system monitor (process/file/network events) is enabled.
    #[serde(default = "default_monitor_enabled")]
    pub monitor_enabled: bool,
    /// How often to flush system event batches to the server (seconds).
    #[serde(default = "default_monitor_batch_interval")]
    pub monitor_batch_interval: u64,
    /// Paths to monitor for file events (fanotify mount points on Linux).
    #[serde(default = "default_monitor_paths")]
    pub monitor_paths: Vec<String>,
}

fn default_heartbeat_interval() -> u64 {
    60
}

fn default_telemetry_interval() -> u64 {
    60
}

fn default_monitor_enabled() -> bool {
    true
}

fn default_monitor_batch_interval() -> u64 {
    5
}

fn default_monitor_paths() -> Vec<String> {
    vec!["/".to_string()]
}

impl AgentConfig {
    /// Config directory path — defaults to ~/.osfm_edm/
    pub fn config_dir() -> PathBuf {
        dirs_or_default()
    }

    /// Full path to the config file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load configuration from disk.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path();
        if !path.exists() {
            return Err(ConfigError::NotEnrolled);
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        let config: AgentConfig = toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        Ok(config)
    }

    /// Save configuration to disk.
    pub fn save(&self) -> Result<(), ConfigError> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(Self::config_path(), content)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(())
    }

    /// Save a PEM file to the config directory.
    pub fn save_pem(filename: &str, content: &str) -> Result<PathBuf, ConfigError> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        let path = dir.join(filename);
        std::fs::write(&path, content)
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(path)
    }
}

fn dirs_or_default() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        Path::new(&home).join(".osfm_edm")
    } else {
        PathBuf::from("/etc/osfm_edm")
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Agent is not enrolled — run with --token to enroll")]
    NotEnrolled,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
}
