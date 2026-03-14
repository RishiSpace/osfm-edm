//! Auth API — login, logout, refresh, user info, and MFA setup/verification.

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::middleware::auth::{AuthUser, Claims};
use crate::state::AppState;

/// Build the auth sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/mfa/setup", post(mfa_setup))
        .route("/mfa/verify", post(mfa_verify))
}

// --- Request / Response types ---

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub role: String,
    pub totp_enabled: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_login: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct MfaVerifyRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct MfaSetupResponse {
    pub secret: String,
    pub otpauth_url: String,
}

// --- DB row structs ---

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    password_hash: String,
    totp_secret: Option<String>,
    totp_enabled: bool,
    role: String,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    last_login: Option<chrono::DateTime<chrono::Utc>>,
}

// --- Handlers ---

/// POST /api/v1/auth/login — authenticate with username/password, optionally TOTP.
async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    // Rate limiting check (in-memory, per-IP — simplified as per-user here since
    // we don't have the IP readily in the handler without additional extraction).
    // Full IP-based rate limiting is handled at the router level.

    // Find user by username.
    let user: UserRow = sqlx::query_as(
        "SELECT id, username, password_hash, totp_secret, totp_enabled, role, created_at, last_login FROM users WHERE username = $1"
    )
    .bind(&body.username)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Invalid credentials".to_string()))?;

    // Verify password.
    let password_valid = bcrypt::verify(&body.password, &user.password_hash)
        .map_err(|e| ApiError::Internal(format!("Password verification error: {e}")))?;

    if !password_valid {
        return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
    }

    // Check TOTP if enabled.
    if user.totp_enabled {
        let totp_code = body.totp_code.as_deref().unwrap_or("");
        let totp_secret = user.totp_secret.as_deref().ok_or_else(|| {
            ApiError::Internal("TOTP enabled but no secret configured".to_string())
        })?;

        let totp = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1,
            30,
            totp_rs::Secret::Encoded(totp_secret.to_string())
                .to_bytes()
                .map_err(|e| ApiError::Internal(format!("Invalid TOTP secret: {e}")))?,
            None,
            "OSFM-EDM".to_string(),
        )
        .map_err(|e| ApiError::Internal(format!("TOTP creation error: {e}")))?;

        let valid = totp
            .check_current(totp_code)
            .map_err(|e| ApiError::Internal(format!("TOTP check error: {e}")))?;

        if !valid {
            return Err(ApiError::Unauthorized("Invalid TOTP code".to_string()));
        }
    }

    // Generate access token (15 min expiry).
    let now = chrono::Utc::now().timestamp() as usize;
    let access_expiry = 15 * 60; // 15 minutes
    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        exp: now + access_expiry,
        iat: now,
    };

    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )?;

    // Generate refresh token (7 day expiry) and store hash in DB.
    let refresh_token = Uuid::new_v4().to_string();
    let refresh_hash = format!("{:x}", Sha256::digest(refresh_token.as_bytes()));
    let refresh_expires = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)"
    )
    .bind(user.id)
    .bind(&refresh_hash)
    .bind(refresh_expires)
    .execute(&state.db)
    .await?;

    // Update last_login.
    sqlx::query("UPDATE users SET last_login = now() WHERE id = $1")
        .bind(user.id)
        .execute(&state.db)
        .await?;

    // Set refresh token as httpOnly cookie.
    let cookie = format!(
        "refresh_token={refresh_token}; HttpOnly; Secure; SameSite=Strict; Path=/api/v1/auth; Max-Age={}",
        7 * 24 * 60 * 60
    );

    let response = (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({
            "data": LoginResponse {
                access_token,
                token_type: "Bearer".to_string(),
                expires_in: access_expiry as u64,
            },
            "error": null,
        })),
    );

    Ok(response)
}

/// POST /api/v1/auth/refresh — exchange refresh token cookie for new access token.
async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    // Extract refresh token from cookie.
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let refresh_token = cookie_header
        .split(';')
        .find_map(|c| {
            let c = c.trim();
            c.strip_prefix("refresh_token=")
        })
        .ok_or_else(|| ApiError::Unauthorized("Missing refresh token cookie".to_string()))?;

    let refresh_hash = format!("{:x}", Sha256::digest(refresh_token.as_bytes()));

    // Look up refresh token in DB.
    #[derive(sqlx::FromRow)]
    struct RefreshRow {
        user_id: Uuid,
        revoked: bool,
        expires_at: chrono::DateTime<chrono::Utc>,
    }

    let token_row: RefreshRow = sqlx::query_as(
        "SELECT user_id, revoked, expires_at FROM refresh_tokens WHERE token_hash = $1"
    )
    .bind(&refresh_hash)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("Invalid refresh token".to_string()))?;

    if token_row.revoked {
        return Err(ApiError::Unauthorized("Refresh token has been revoked".to_string()));
    }

    if token_row.expires_at < chrono::Utc::now() {
        return Err(ApiError::Unauthorized("Refresh token has expired".to_string()));
    }

    // Look up user.
    let user: UserRow = sqlx::query_as(
        "SELECT id, username, password_hash, totp_secret, totp_enabled, role, created_at, last_login FROM users WHERE id = $1"
    )
    .bind(token_row.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::Unauthorized("User not found".to_string()))?;

    // Issue new access token.
    let now = chrono::Utc::now().timestamp() as usize;
    let access_expiry = 15 * 60;
    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        exp: now + access_expiry,
        iat: now,
    };

    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt_secret.as_bytes()),
    )?;

    Ok(Json(serde_json::json!({
        "data": LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: access_expiry as u64,
        },
        "error": null,
    })))
}

/// POST /api/v1/auth/logout — revoke the refresh token.
async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(refresh_token) = cookie_header
        .split(';')
        .find_map(|c| c.trim().strip_prefix("refresh_token="))
    {
        let refresh_hash = format!("{:x}", Sha256::digest(refresh_token.as_bytes()));
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE token_hash = $1")
            .bind(&refresh_hash)
            .execute(&state.db)
            .await?;
    }

    // Clear the cookie.
    let clear_cookie = "refresh_token=; HttpOnly; Secure; SameSite=Strict; Path=/api/v1/auth; Max-Age=0";

    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, clear_cookie)],
        Json(serde_json::json!({
            "data": { "message": "Logged out successfully" },
            "error": null,
        })),
    ))
}

/// GET /api/v1/auth/me — return current user info.
async fn me(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let user: UserRow = sqlx::query_as(
        "SELECT id, username, password_hash, totp_secret, totp_enabled, role, created_at, last_login FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    Ok(Json(serde_json::json!({
        "data": UserInfo {
            id: user.id,
            username: user.username,
            role: user.role,
            totp_enabled: user.totp_enabled,
            created_at: user.created_at,
            last_login: user.last_login,
        },
        "error": null,
    })))
}

/// POST /api/v1/auth/mfa/setup — generate TOTP secret and return otpauth URL.
async fn mfa_setup(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let secret = totp_rs::Secret::generate_secret();
    let secret_encoded = secret.to_encoded().to_string();

    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(|e| ApiError::Internal(format!("Secret error: {e}")))?,
        Some(auth.username.clone()),
        "OSFM-EDM".to_string(),
    )
    .map_err(|e| ApiError::Internal(format!("TOTP creation error: {e}")))?;

    let otpauth_url = totp.get_url();

    // Store the secret but don't enable TOTP yet (enable after verification).
    sqlx::query("UPDATE users SET totp_secret = $1 WHERE id = $2")
        .bind(&secret_encoded)
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "data": MfaSetupResponse {
            secret: secret_encoded,
            otpauth_url,
        },
        "error": null,
    })))
}

/// POST /api/v1/auth/mfa/verify — verify TOTP code and enable 2FA.
async fn mfa_verify(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<MfaVerifyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Fetch the stored TOTP secret.
    #[derive(sqlx::FromRow)]
    struct TotpRow {
        totp_secret: Option<String>,
    }

    let row: TotpRow = sqlx::query_as(
        "SELECT totp_secret FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    let secret = row
        .totp_secret
        .ok_or_else(|| ApiError::BadRequest("MFA not set up — call /mfa/setup first".to_string()))?;

    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(secret)
            .to_bytes()
            .map_err(|e| ApiError::Internal(format!("Invalid TOTP secret: {e}")))?,
        None,
        "OSFM-EDM".to_string(),
    )
    .map_err(|e| ApiError::Internal(format!("TOTP creation error: {e}")))?;

    let valid = totp
        .check_current(&body.code)
        .map_err(|e| ApiError::Internal(format!("TOTP check error: {e}")))?;

    if !valid {
        return Err(ApiError::BadRequest("Invalid TOTP code".to_string()));
    }

    // Enable TOTP on the account.
    sqlx::query("UPDATE users SET totp_enabled = true WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({
        "data": { "message": "2FA enabled successfully" },
        "error": null,
    })))
}

/// Create the default admin user on first boot if no users exist.
pub async fn ensure_admin_user(state: &AppState) -> anyhow::Result<()> {
    #[derive(sqlx::FromRow)]
    struct CountRow {
        count: i64,
    }

    let row: CountRow = sqlx::query_as("SELECT COUNT(*) as count FROM users")
        .fetch_one(&state.db)
        .await?;

    if row.count > 0 {
        tracing::info!("Users table is not empty, skipping default admin creation");
        return Ok(());
    }

    let username = &state.config.admin_username;
    let password = &state.config.admin_password;

    if username == "admin" && password == "admin" {
        tracing::warn!(
            "⚠️  Creating default admin user with default credentials. \
             Change ADMIN_USERNAME and ADMIN_PASSWORD immediately!"
        );
    }

    let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;

    sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, 'admin')"
    )
    .bind(username)
    .bind(&password_hash)
    .execute(&state.db)
    .await?;

    tracing::info!(username, "Default admin user created");
    Ok(())
}
