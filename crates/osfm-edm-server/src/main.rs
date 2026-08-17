//! osfm-edm-server — Axum API server for the OSFM-EDM endpoint management platform.
//!
//! Connects to PostgreSQL, runs migrations, and serves the REST API + WebSocket hub.

mod api;
mod config;
mod db;
mod error;
mod middleware;
mod services;
mod state;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::middleware as axum_mw;
use axum::routing::get;
use axum::{Router, response::IntoResponse};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::services::pki::CertificateAuthority;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("osfm_edm_server=debug,tower_http=debug")),
        )
        .init();

    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    tracing::info!("OSFM-EDM server starting up");

    let config = Config::from_env()?;
    tracing::info!(port = config.server_port, "Configuration loaded");

    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;
    tracing::info!("Connected to PostgreSQL");

    sqlx::migrate!("../../migrations").run(&db).await?;
    tracing::info!("Database migrations applied");

    let data_dir = PathBuf::from("data");
    let ca = match CertificateAuthority::load_or_create(&data_dir) {
        Ok(ca) => {
            if let Ok(fp) = ca.ca_fingerprint_sha256() {
                tracing::info!("CA SHA-256 fingerprint: {fp}");
            }
            Some(ca)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize PKI — enrollment and TLS will be unavailable");
            None
        }
    };

    let job_signer = match services::signing::JobSigner::load_or_create(&data_dir) {
        Ok(signer) => {
            tracing::info!("Job signing initialized");
            Some(signer)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize job signing — agents will refuse dispatched jobs");
            None
        }
    };

    let state = AppState::new(db, config.clone(), ca, job_signer);
    api::auth::ensure_admin_user(&state).await?;

    let cors = CorsLayer::new()
        .allow_origin(
            config
                .dashboard_origin
                .parse::<HeaderValue>()
                .unwrap_or_else(|_| HeaderValue::from_static("http://localhost:3000")),
        )
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .allow_credentials(true);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ca.crt", get(ca_crt_handler))
        .route("/ca.fingerprint", get(ca_fp_handler))
        .route("/enroll.sh", get(enroll_sh_handler))
        .route("/enroll.ps1", get(enroll_ps1_handler))
        .route("/ws", get(ws::agent_hub::ws_handler))
        .nest("/api/v1", api::router())
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::audit::audit_layer,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));

    if config.allow_insecure_http {
        tracing::warn!(%addr, "ALLOW_INSECURE_HTTP — binding plaintext HTTP");
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    } else {
        let (cert_pem, key_pem) = load_tls_material(&config, state.ca.as_ref(), &data_dir)?;
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem(
            cert_pem.into_bytes(),
            key_pem.into_bytes(),
        )
        .await?;
        tracing::info!(%addr, "HTTPS listening");
        axum_server::bind_rustls(addr, tls)
            .serve(app.into_make_service())
            .await?;
    }

    Ok(())
}

fn load_tls_material(
    config: &Config,
    ca: Option<&CertificateAuthority>,
    data_dir: &std::path::Path,
) -> anyhow::Result<(String, String)> {
    if let (Some(cert), Some(key)) = (&config.tls_cert_path, &config.tls_key_path) {
        return Ok((std::fs::read_to_string(cert)?, std::fs::read_to_string(key)?));
    }
    let ca = ca.ok_or_else(|| anyhow::anyhow!("internal CA required to auto-issue a server certificate"))?;
    Ok(services::pki::load_or_create_server_material(
        data_dir,
        ca,
        &config.tls_hostnames(),
    )?)
}

async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "data": { "status": "ok", "version": env!("CARGO_PKG_VERSION") },
        "error": null,
    }))
}

async fn ca_crt_handler(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> impl IntoResponse {
    match state.ca.as_ref() {
        Some(ca) => (
            [(header::CONTENT_TYPE, "application/x-pem-file")],
            ca.ca_cert_pem.clone(),
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "CA not initialized").into_response(),
    }
}

async fn ca_fp_handler(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> impl IntoResponse {
    match state.ca.as_ref().and_then(|c| c.ca_fingerprint_sha256().ok()) {
        Some(fp) => fp,
        None => "unavailable".into(),
    }
}

async fn enroll_sh_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    let server = &state.config.server_url;
    let body = format!(
        r#"#!/bin/sh
set -eu
SERVER="${{OSFM_SERVER:-{server}}}"
FINGERPRINT="${{OSFM_CA_FINGERPRINT:-}}"
TOKEN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --token) TOKEN="$2"; shift 2 ;;
    --server) SERVER="$2"; shift 2 ;;
    --ca-fingerprint) FINGERPRINT="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
if [ -z "$TOKEN" ] || [ -z "$FINGERPRINT" ]; then
  echo "usage: enroll.sh --token <token> --ca-fingerprint <hex-from-server-log> [--server URL]" >&2
  echo "The fingerprint must come from the server console (CA SHA-256), not from this script." >&2
  exit 2
fi
if ! command -v osfm-edm-agent >/dev/null 2>&1; then
  echo "osfm-edm-agent not on PATH. Build it: cargo install --path crates/osfm-edm-agent" >&2
  exit 1
fi
exec osfm-edm-agent --server "$SERVER" --token "$TOKEN" --ca-fingerprint "$FINGERPRINT"
"#
    );
    ([(header::CONTENT_TYPE, "text/x-shellscript")], body)
}

async fn enroll_ps1_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    let server = &state.config.server_url;
    let body = format!(
        r#"param([Parameter(Mandatory=$true)][string]$Token, [string]$Server = "{server}", [Parameter(Mandatory=$true)][string]$CaFingerprint)
$agent = Get-Command osfm-edm-agent -ErrorAction SilentlyContinue
if (-not $agent) {{ throw "osfm-edm-agent not on PATH" }}
& osfm-edm-agent --server $Server --token $Token --ca-fingerprint $CaFingerprint
"#
    );
    ([(header::CONTENT_TYPE, "text/plain")], body)
}
