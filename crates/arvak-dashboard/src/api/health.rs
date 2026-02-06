//! Health check endpoint.

use axum::Json;

use crate::dto::HealthResponse;

/// GET /api/health - Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::default())
}
