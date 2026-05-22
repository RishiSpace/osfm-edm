//! Enrollment — handles first-time agent enrollment with the server.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AgentConfig;

/// Response from the server enrollment endpoint.
#[derive(Debug, Deserialize)]
struct EnrollApiResponse {
    data: Option<EnrollData>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct EnrollData {
    device_id: Uuid,
    cert_pem: String,
    key_pem: String,
    ca_pem: String,
    server_url: String,
}

#[derive(Debug, Serialize)]
struct EnrollRequest {
    token: String,
    hostname: String,
    os: String,
    os_version: Option<String>,
    arch: Option<String>,
}

/// Errors during enrollment.
#[derive(Debug, thiserror::Error)]
pub enum EnrollError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),
}

/// Run the enrollment flow: POST to the server, save certs and config.
pub async fn enroll(server_url: &str, token: &str) -> Result<AgentConfig, EnrollError> {
    let hostname = gethostname();
    let os = current_os();
    let os_version = os_version_string();
    let arch = std::env::consts::ARCH.to_string();

    tracing::info!(server = server_url, hostname = %hostname, "Starting enrollment");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Allow self-signed server certs during enrollment
        .build()?;

    let url = format!("{}/api/v1/enroll", server_url.trim_end_matches('/'));
    let resp: EnrollApiResponse = client
        .post(&url)
        .json(&EnrollRequest {
            token: token.to_string(),
            hostname: hostname.clone(),
            os: os.clone(),
            os_version: Some(os_version),
            arch: Some(arch),
        })
        .send()
        .await?
        .json()
        .await?;

    if let Some(err) = resp.error {
        return Err(EnrollError::Server(format!("{err}")));
    }

    let data = resp
        .data
        .ok_or_else(|| EnrollError::Server("Empty response from server".to_string()))?;

    // Save certificates to disk.
    let cert_path = AgentConfig::save_pem("device.crt", &data.cert_pem)?;
    let key_path = AgentConfig::save_pem("device.key", &data.key_pem)?;
    let ca_path = AgentConfig::save_pem("ca.crt", &data.ca_pem)?;

    // Build and save config.
    let config = AgentConfig {
        server_url: data.server_url,
        device_id: data.device_id,
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        ca_path: ca_path.to_string_lossy().to_string(),
        heartbeat_interval: 60,
        telemetry_interval: 60,
        monitor_enabled: true,
        monitor_batch_interval: 5,
        monitor_paths: vec!["/".to_string()],
    };

    config.save()?;
    tracing::info!(device_id = %data.device_id, "Enrollment successful");

    Ok(config)
}

fn gethostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn current_os() -> String {
    if cfg!(target_os = "linux") {
        "linux".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

fn os_version_string() -> String {
    sysinfo::System::os_version().unwrap_or_else(|| "unknown".to_string())
}
