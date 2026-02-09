//! Health check and metrics HTTP endpoints.
//!
//! This module provides HTTP endpoints for monitoring:
//! - /health - Basic liveness check
//! - /health/ready - Readiness check with backend validation
//! - /metrics - Prometheus metrics in text format

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

use crate::metrics::Metrics;
use crate::server::BackendRegistry;

/// Server uptime tracker.
static START_TIME: std::sync::OnceLock<SystemTime> = std::sync::OnceLock::new();

/// Initialize the start time (call once at server startup).
pub fn init_start_time() {
    START_TIME.get_or_init(SystemTime::now);
}

/// Get server uptime in seconds.
fn get_uptime_seconds() -> u64 {
    START_TIME
        .get()
        .and_then(|start| SystemTime::now().duration_since(*start).ok())
        .map_or(0, |d| d.as_secs())
}

/// Shared state for health check handlers.
#[derive(Clone)]
pub struct HealthState {
    pub backends: Arc<BackendRegistry>,
    pub metrics: Metrics,
}

impl HealthState {
    pub fn new(backends: Arc<BackendRegistry>, metrics: Metrics) -> Self {
        Self { backends, metrics }
    }
}

/// Response for /health endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

/// Response for /health/ready endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub backends: Vec<BackendStatus>,
    pub active_jobs: u64,
    pub queued_jobs: u64,
}

/// Backend status information.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackendStatus {
    pub backend_id: String,
    pub available: bool,
}

/// Handler for GET /health
///
/// Returns basic liveness information. This endpoint should always return 200
/// if the server is running, regardless of backend availability.
async fn health_handler() -> impl IntoResponse {
    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: get_uptime_seconds(),
    };

    Json(response)
}

/// Handler for GET /health/ready
///
/// Returns readiness status, indicating whether the server is ready to accept
/// traffic. Checks backend availability and service capacity.
async fn readiness_handler(State(state): State<HealthState>) -> Response {
    // Check all backends
    let backend_ids = state.backends.list();
    let backends: Vec<BackendStatus> = backend_ids
        .iter()
        .map(|id| {
            let available = state.backends.get(id).is_ok();
            BackendStatus {
                backend_id: id.clone(),
                available,
            }
        })
        .collect();

    // Get current job metrics
    let snapshot = state.metrics.snapshot();

    // Consider ready if at least one backend is available
    let ready = backends.iter().any(|b| b.available);

    let response = ReadinessResponse {
        ready,
        backends,
        active_jobs: snapshot.active_jobs,
        queued_jobs: snapshot.queued_jobs,
    };

    let status_code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response)).into_response()
}

/// Handler for GET /metrics
///
/// Returns Prometheus metrics in text format for scraping.
async fn metrics_handler(State(state): State<HealthState>) -> impl IntoResponse {
    match state.metrics.export() {
        Ok(metrics) => (StatusCode::OK, metrics).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to export metrics".to_string(),
        )
            .into_response(),
    }
}

/// Create the health check HTTP router.
pub fn create_health_router(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/health/ready", get(readiness_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
}

/// Start the health check HTTP server on the specified port.
///
/// This runs the HTTP server in the background and returns immediately.
pub async fn start_health_server(
    port: u16,
    state: HealthState,
) -> Result<(), Box<dyn std::error::Error>> {
    init_start_time();

    let app = create_health_router(state);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("Health check server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_uptime() {
        init_start_time();
        let uptime = get_uptime_seconds();
        // Uptime should be a valid value (not panicking)
        assert!(uptime < 1000000); // Less than ~11 days is reasonable for test
    }

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            uptime_seconds: 42,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_readiness_response() {
        let response = ReadinessResponse {
            ready: true,
            backends: vec![BackendStatus {
                backend_id: "simulator".to_string(),
                available: true,
            }],
            active_jobs: 5,
            queued_jobs: 10,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("simulator"));
        assert!(json.contains("true"));
    }
}
