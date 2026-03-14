//! Crate-level error types — all server errors flow through ApiError.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Top-level error enum for the osfm-edm-server crate.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("Bcrypt error: {0}")]
    Bcrypt(#[from] bcrypt::BcryptError),
}

impl ApiError {
    /// Map error variant to the appropriate HTTP status code.
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Json(_) => StatusCode::BAD_REQUEST,
            ApiError::Jwt(_) => StatusCode::UNAUTHORIZED,
            ApiError::Bcrypt(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Map error variant to a machine-readable error code.
    fn error_code(&self) -> &'static str {
        match self {
            ApiError::NotFound(_) => "NOT_FOUND",
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::Unauthorized(_) => "UNAUTHORIZED",
            ApiError::Forbidden(_) => "FORBIDDEN",
            ApiError::Conflict(_) => "CONFLICT",
            ApiError::Internal(_) => "INTERNAL_ERROR",
            ApiError::Database(_) => "DATABASE_ERROR",
            ApiError::Json(_) => "INVALID_JSON",
            ApiError::Jwt(_) => "INVALID_TOKEN",
            ApiError::Bcrypt(_) => "INTERNAL_ERROR",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.to_string();

        // Log internal errors at error level, client errors at warn level.
        if status.is_server_error() {
            tracing::error!(error = %message, code, "Server error");
        } else {
            tracing::warn!(error = %message, code, "Client error");
        }

        let body = json!({
            "data": null,
            "error": {
                "code": code,
                "message": message,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

/// Convenience type alias for handler return types.
pub type ApiResult<T> = Result<T, ApiError>;
