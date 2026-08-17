//! First-time enrollment. TLS is pinned to the server CA.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::AgentConfig;
use crate::tls;

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
    #[serde(default)]
    device_token: String,
    #[serde(default)]
    server_signing_pubkey: String,
}

#[derive(Debug, Serialize)]
struct EnrollRequest {
    token: String,
    hostname: String,
    os: String,
    os_version: Option<String>,
    arch: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EnrollError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("{0}")]
    Tls(String),
}

pub struct EnrollOpts {
    pub ca_path: Option<String>,
    pub ca_fingerprint: Option<String>,
    pub insecure: bool,
}

pub async fn enroll(server_url: &str, token: &str, opts: &EnrollOpts) -> Result<AgentConfig, EnrollError> {
    let hostname = gethostname();
    let os = current_os();
    let os_version = os_version_string();
    let arch = std::env::consts::ARCH.to_string();

    tracing::info!(server = server_url, hostname = %hostname, "Starting enrollment");

    let client = build_enroll_client(server_url, opts).await?;

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

    if !data.ca_pem.is_empty() {
        if let Some(fp) = &opts.ca_fingerprint {
            if !tls::fingerprints_match(&data.ca_pem, fp) {
                return Err(EnrollError::Tls(
                    "server returned a CA that does not match --ca-fingerprint".into(),
                ));
            }
        }
    }

    let cert_path = AgentConfig::save_pem("device.crt", &data.cert_pem)?;
    let key_path = AgentConfig::save_pem("device.key", &data.key_pem)?;
    let ca_path = AgentConfig::save_pem("ca.crt", &data.ca_pem)?;

    if data.device_token.is_empty() {
        tracing::warn!("Server returned no device token — agent will not be able to connect over WebSocket");
    }
    if data.server_signing_pubkey.is_empty() {
        tracing::warn!("Server returned no signing public key — job signature verification will reject all jobs");
    }

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
        device_token: data.device_token,
        server_pubkey: data.server_signing_pubkey,
    };

    config.save()?;
    tracing::info!(device_id = %data.device_id, "Enrollment successful");
    Ok(config)
}

async fn build_enroll_client(server_url: &str, opts: &EnrollOpts) -> Result<reqwest::Client, EnrollError> {
    if opts.insecure {
        tracing::warn!("--insecure: accepting any TLS certificate (MITM possible)");
        return Ok(reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?);
    }

    if let Some(path) = &opts.ca_path {
        let pem = std::fs::read(path).map_err(|e| EnrollError::Tls(e.to_string()))?;
        let cert = reqwest::Certificate::from_pem(&pem).map_err(|e| EnrollError::Tls(e.to_string()))?;
        return Ok(reqwest::Client::builder()
            .add_root_certificate(cert)
            .https_only(server_url.starts_with("https://"))
            .build()?);
    }

    if server_url.starts_with("https://") {
        let fp = opts.ca_fingerprint.as_deref().ok_or_else(|| {
            EnrollError::Tls(
                "HTTPS enrollment requires --ca PATH, --ca-fingerprint HEX (from the server log), or --insecure".into(),
            )
        })?;
        let insecure = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let ca_url = format!("{}/ca.crt", server_url.trim_end_matches('/'));
        let pem = insecure
            .get(&ca_url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        if !tls::fingerprints_match(&pem, fp) {
            return Err(EnrollError::Tls(format!(
                "CA fingerprint mismatch (expected {fp})"
            )));
        }
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| EnrollError::Tls(e.to_string()))?;
        return Ok(reqwest::Client::builder().add_root_certificate(cert).build()?);
    }

    Ok(reqwest::Client::new())
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
