//! Auth middleware — JWT token extraction, validation, and user injection.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// JWT claims embedded in access tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — user ID.
    pub sub: Uuid,
    /// Username.
    pub username: String,
    /// User role (admin / viewer).
    pub role: String,
    /// Expiration timestamp (epoch seconds).
    pub exp: usize,
    /// Issued at timestamp.
    pub iat: usize,
}

/// Authenticated user extracted from JWT in request headers.
/// Use as an Axum extractor in handlers that require authentication.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
}

impl AuthUser {
    /// True when the user holds the `admin` role.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Require the `admin` role for privileged (state-changing) operations.
    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!(
                "User '{}' (role: {}) does not have admin privileges",
                self.username, self.role
            )))
        }
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = Arc::<AppState>::from_ref(state);

        // Extract the Bearer token from the Authorization header.
        let token = extract_bearer_token(&parts.headers)?;

        // Decode and validate the JWT.
        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(app_state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| ApiError::Unauthorized(format!("Invalid token: {e}")))?;

        // Role and existence come from the DB — a stale JWT cannot keep a
        // demoted or deleted user privileged.
        #[derive(sqlx::FromRow)]
        struct RoleRow {
            username: String,
            role: String,
        }
        let row: RoleRow = sqlx::query_as("SELECT username, role FROM users WHERE id = $1")
            .bind(token_data.claims.sub)
            .fetch_optional(&app_state.db)
            .await?
            .ok_or_else(|| ApiError::Unauthorized("User no longer exists".to_string()))?;

        Ok(AuthUser {
            user_id: token_data.claims.sub,
            username: row.username,
            role: row.role,
        })
    }
}

/// Extract Bearer token from Authorization header.
fn extract_bearer_token(headers: &HeaderMap) -> Result<String, ApiError> {
    let auth_header = headers
        .get("Authorization")
        .ok_or_else(|| ApiError::Unauthorized("Missing Authorization header".to_string()))?
        .to_str()
        .map_err(|_| ApiError::Unauthorized("Invalid Authorization header encoding".to_string()))?;

    if !auth_header.starts_with("Bearer ") {
        return Err(ApiError::Unauthorized(
            "Authorization header must use Bearer scheme".to_string(),
        ));
    }

    Ok(auth_header[7..].to_string())
}
