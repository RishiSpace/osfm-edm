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

        Ok(AuthUser {
            user_id: token_data.claims.sub,
            username: token_data.claims.username,
            role: token_data.claims.role,
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
