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

use axum::http::{header, HeaderValue, Method};
use axum::middleware as axum_mw;
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::services::pki::CertificateAuthority;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing subscriber with environment filter.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("osfm_edm_server=debug,tower_http=debug")),
        )
        .init();

    tracing::info!("OSFM-EDM server starting up");

    // Load configuration from environment.
    let config = Config::from_env()?;
    tracing::info!(port = config.server_port, agent_port = config.agent_port, "Configuration loaded");

    // Connect to PostgreSQL.
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;
    tracing::info!("Connected to PostgreSQL");

    // Run database migrations.
    sqlx::migrate!("../../migrations")
        .run(&db)
        .await?;
    tracing::info!("Database migrations applied");

    // Initialize PKI (load or generate CA).
    let data_dir = PathBuf::from("data");
    let ca = match CertificateAuthority::load_or_create(&data_dir) {
        Ok(ca) => {
            tracing::info!("PKI initialized");
            Some(ca)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to initialize PKI — enrollment will be unavailable");
            None
        }
    };

    // Build application state.
    let state = AppState::new(db, config.clone(), ca);

    // Create default admin user on first boot.
    api::auth::ensure_admin_user(&state).await?;

    // Configure CORS.
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

    // Build the Axum router.
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ws", get(ws::agent_hub::ws_handler))
        .nest("/api/v1", api::router())
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::audit::audit_layer,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Bind and serve.
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    tracing::info!(%addr, "HTTP server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint — returns server status and version.
async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "data": {
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "error": null,
    }))
}
