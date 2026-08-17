//! Enrollment API — token generation and agent enrollment.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::AuthUser;
use crate::state::AppState;

/// Build the enrollment sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/token", post(create_enrollment_token))
        .route("/", post(enroll_device))
}

// --- Request / Response types ---

#[derive(Debug, Serialize)]
pub struct EnrollmentTokenResponse {
    pub token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    pub token: String,
    pub hostname: String,
    pub os: String,
    pub os_version: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    pub device_id: Uuid,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub server_url: String,
    /// Per-device secret token used to authenticate WebSocket connections.
    /// Returned once here; only its SHA-256 hash is stored server-side.
    pub device_token: String,
    /// Base64-encoded Ed25519 public key the agent uses to verify signed jobs.
    pub server_signing_pubkey: String,
}

// --- Handlers ---

/// POST /api/v1/enroll/token — generate a one-time enrollment token (admin only).
async fn create_enrollment_token(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<impl IntoResponse> {
    auth.require_admin()?;

    let token = Uuid::new_v4().to_string();
    let token_hash = hex_sha256(token.as_bytes());
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    sqlx::query(
        "INSERT INTO enrollment_tokens (token, created_by, expires_at) VALUES ($1, $2, $3)"
    )
    .bind(&token_hash)
    .bind(auth.user_id)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    tracing::info!(user = %auth.username, "Enrollment token created");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": EnrollmentTokenResponse { token, expires_at },
            "error": null,
        })),
    ))
}

/// POST /api/v1/enroll — enroll a device using a one-time token (no auth required).
async fn enroll_device(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EnrollRequest>,
) -> ApiResult<impl IntoResponse> {
    // Validate the OS field.
    if !["windows", "linux", "macos"].contains(&body.os.as_str()) {
        return Err(ApiError::BadRequest(
            "os must be one of: windows, linux, macos".to_string(),
        ));
    }

    // Validate and consume the enrollment token.
    #[derive(sqlx::FromRow)]
    struct TokenRow {
        id: Uuid,
        used: bool,
        expires_at: chrono::DateTime<chrono::Utc>,
    }

    let token_hash = hex_sha256(body.token.as_bytes());
    let token_row: TokenRow = sqlx::query_as(
        "SELECT id, used, expires_at FROM enrollment_tokens WHERE token = $1"
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::BadRequest("Invalid enrollment token".to_string()))?;

    if token_row.used {
        return Err(ApiError::Conflict("Enrollment token has already been used".to_string()));
    }

    if token_row.expires_at < chrono::Utc::now() {
        return Err(ApiError::BadRequest("Enrollment token has expired".to_string()));
    }

    // Generate the per-device auth token (256-bit). Only the SHA-256 hash is
    // stored; the plaintext token is returned once to the enrolling agent.
    let device_token = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let device_token_hash = hex_sha256(device_token.as_bytes());

    // Create the device record.
    #[derive(sqlx::FromRow)]
    struct DeviceRow {
        id: Uuid,
    }

    let device: DeviceRow = sqlx::query_as(
        "INSERT INTO devices (hostname, os, os_version, arch, auth_token_hash) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind(&body.hostname)
    .bind(&body.os)
    .bind(&body.os_version)
    .bind(&body.arch)
    .bind(&device_token_hash)
    .fetch_one(&state.db)
    .await?;

    // Issue a device certificate.
    let ca = state.ca.as_ref().ok_or_else(|| {
        ApiError::Internal("PKI not initialized".to_string())
    })?;

    let (cert_pem, key_pem) = ca
        .issue_device_cert(device.id)
        .map_err(|e| ApiError::Internal(format!("Failed to issue device cert: {e}")))?;

    let fingerprint = crate::services::pki::fingerprint_der_pem(&cert_pem);

    // Store certificate.
    let expires_at = chrono::Utc::now() + chrono::Duration::days(365);
    sqlx::query(
        "INSERT INTO certificates (device_id, fingerprint, pem, expires_at) VALUES ($1, $2, $3, $4)"
    )
    .bind(device.id)
    .bind(&fingerprint)
    .bind(&cert_pem)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    // Mark token as used.
    sqlx::query(
        "UPDATE enrollment_tokens SET used = true, used_at = now(), used_by = $1 WHERE id = $2"
    )
    .bind(device.id)
    .bind(token_row.id)
    .execute(&state.db)
    .await?;

    // The server signing pubkey lets the agent verify dispatched jobs.
    let server_signing_pubkey = state
        .job_signer
        .as_ref()
        .map(|s| s.public_key_b64().to_string())
        .unwrap_or_default();

    tracing::info!(device_id = %device.id, hostname = %body.hostname, "Device enrolled");

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "data": EnrollResponse {
                device_id: device.id,
                cert_pem,
                key_pem,
                ca_pem: ca.ca_cert_pem.clone(),
                server_url: state.config.server_url.clone(),
                device_token,
                server_signing_pubkey,
            },
            "error": null,
        })),
    ))
}

/// Lowercase-hex SHA-256 of the given bytes. Used for device auth tokens —
/// the DB stores only the hash, never the token itself.
fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
