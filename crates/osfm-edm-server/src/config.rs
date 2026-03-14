//! Server configuration — all values loaded from environment variables.

use std::env;

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string.
    pub database_url: String,
    /// Secret used to sign JWT access and refresh tokens.
    pub jwt_secret: String,
    /// Port for the HTTP API server (dashboard + REST API).
    pub server_port: u16,
    /// Port for the agent WebSocket listener (mTLS).
    pub agent_port: u16,
    /// Public server URL used in enrollment responses.
    pub server_url: String,
    /// Default admin username created on first boot.
    pub admin_username: String,
    /// Default admin password created on first boot.
    pub admin_password: String,
    /// Optional TLS certificate path for the API server.
    pub tls_cert_path: Option<String>,
    /// Optional TLS private key path for the API server.
    pub tls_key_path: Option<String>,
    /// Allowed CORS origin for the dashboard.
    pub dashboard_origin: String,
}

impl Config {
    /// Load configuration from environment variables with sensible defaults.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| ConfigError::Missing("DATABASE_URL"))?;
        let jwt_secret = env::var("JWT_SECRET")
            .map_err(|_| ConfigError::Missing("JWT_SECRET"))?;

        if jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid(
                "JWT_SECRET must be at least 32 characters",
            ));
        }

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid("SERVER_PORT must be a valid port number"))?;

        let agent_port = env::var("AGENT_PORT")
            .unwrap_or_else(|_| "8443".to_string())
            .parse::<u16>()
            .map_err(|_| ConfigError::Invalid("AGENT_PORT must be a valid port number"))?;

        let server_url = env::var("SERVER_URL")
            .unwrap_or_else(|_| format!("https://localhost:{agent_port}"));

        let admin_username = env::var("ADMIN_USERNAME")
            .unwrap_or_else(|_| "admin".to_string());

        let admin_password = env::var("ADMIN_PASSWORD")
            .unwrap_or_else(|_| "admin".to_string());

        let tls_cert_path = env::var("TLS_CERT_PATH").ok().filter(|s| !s.is_empty());
        let tls_key_path = env::var("TLS_KEY_PATH").ok().filter(|s| !s.is_empty());

        let dashboard_origin = env::var("NEXT_PUBLIC_API_URL")
            .unwrap_or_else(|_| format!("http://localhost:{server_port}"));

        Ok(Config {
            database_url,
            jwt_secret,
            server_port,
            agent_port,
            server_url,
            admin_username,
            admin_password,
            tls_cert_path,
            tls_key_path,
            dashboard_origin,
        })
    }
}

/// Configuration loading errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    Missing(&'static str),
    #[error("Invalid configuration: {0}")]
    Invalid(&'static str),
}
