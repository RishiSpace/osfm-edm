//! Audit middleware — logs all state-changing API requests to the audit_log table.

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use uuid::Uuid;

use crate::state::AppState;

/// Axum middleware that writes an audit log entry for every state-changing request.
/// State-changing methods: POST, PATCH, PUT, DELETE.
pub async fn audit_layer(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let uri = request.uri().path().to_string();

    // Only audit state-changing methods.
    let should_audit = matches!(
        method,
        Method::POST | Method::PATCH | Method::PUT | Method::DELETE
    );

    // Extract user ID from the auth header if present (best-effort).
    let user_id = extract_user_id_from_request(&request, &state);

    // Extract client IP.
    let ip_address = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    // Execute the actual handler.
    let response = next.run(request).await;

    // Write audit log entry asynchronously (fire-and-forget).
    if should_audit {
        let action = format!("{method} {uri}");
        let status = response.status().as_u16();
        let detail = serde_json::json!({ "status": status });

        let db = state.db.clone();
        tokio::spawn(async move {
            let result = sqlx::query(
                "INSERT INTO audit_log (user_id, action, detail, ip_address) VALUES ($1, $2, $3, $4)"
            )
            .bind(user_id)
            .bind(&action)
            .bind(&detail)
            .bind(ip_address.as_deref())
            .execute(&db)
            .await;

            if let Err(e) = result {
                tracing::error!(error = %e, action, "Failed to write audit log");
            }
        });
    }

    response
}

/// Best-effort extraction of user ID from the JWT in the Authorization header.
/// Does not fail the request if the token is missing or invalid.
fn extract_user_id_from_request(request: &Request<Body>, state: &AppState) -> Option<Uuid> {
    let auth_header = request
        .headers()
        .get("Authorization")?
        .to_str()
        .ok()?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    let token = &auth_header[7..];
    let token_data = jsonwebtoken::decode::<crate::middleware::auth::Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .ok()?;

    Some(token_data.claims.sub)
}
