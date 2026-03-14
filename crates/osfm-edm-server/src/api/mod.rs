//! API module — all REST API route handlers.

pub mod auth;
pub mod devices;
pub mod enroll;
pub mod groups;
pub mod jobs;
pub mod patches;
pub mod policies;
pub mod reports;
pub mod settings;
pub mod software;

use std::sync::Arc;

use axum::Router;

use crate::state::AppState;

/// Build the combined API router with all sub-routes.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/enroll", enroll::router())
        .nest("/devices", devices::router())
}
