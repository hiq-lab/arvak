//! REST gateway for the Arvak quantum compilation and execution service.
//!
//! Provides a JSON/HTTP interface that proxies to the existing in-process
//! compilation pipeline and job execution engine. Designed for environments
//! (such as Bloomberg BQuant) where gRPC is unavailable.

pub mod auth;
pub mod types;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderValue, Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::server::service::circuit_utils;
use crate::server::{BackendRegistry, JobStore};

use auth::AuthState;
use types::*;

// ── Shared application state ──────────────────────────────────────────────

/// Application state shared across all REST handlers.
#[derive(Clone)]
pub struct AppState {
    pub job_store: Arc<JobStore>,
    pub backends: Arc<BackendRegistry>,
    pub metrics: crate::metrics::Metrics,
    pub resources: Option<crate::resource_manager::ResourceManager>,
    pub abort_handles:
        Arc<tokio::sync::RwLock<std::collections::HashMap<String, tokio::task::AbortHandle>>>,
    pub auth: AuthState,
}

// ── Router construction ───────────────────────────────────────────────────

/// Build the Axum router for the REST gateway.
///
/// `cors_origins` is a comma-separated list of allowed origins, or `"*"`.
pub fn rest_router(state: AppState, cors_origins: &str) -> Router {
    let cors = build_cors_layer(cors_origins);

    Router::new()
        .route("/v1/health", get(health_handler))
        .route("/v1/backends", get(list_backends_handler))
        .route("/v1/backends/{id}", get(get_backend_handler))
        .route("/v1/compile", post(compile_handler))
        .route("/v1/jobs", post(submit_job_handler))
        .route("/v1/jobs/{id}", get(get_job_status_handler))
        .route("/v1/jobs/{id}/result", get(get_job_result_handler))
        .route("/v1/jobs/{id}", delete(cancel_job_handler))
        .layer(middleware::from_fn(auth::bearer_auth))
        .layer(cors)
        .layer(axum::Extension(state.auth.clone()))
        .with_state(state)
}

fn build_cors_layer(origins: &str) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    if origins == "*" {
        layer.allow_origin(tower_http::cors::Any)
    } else {
        let allowed: Vec<HeaderValue> = origins
            .split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect();
        layer.allow_origin(allowed)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn error_response(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: msg.into(),
            code: status.as_u16(),
        }),
    )
        .into_response()
}

fn job_status_string(status: &arvak_hal::job::JobStatus) -> String {
    match status {
        arvak_hal::job::JobStatus::Queued => "queued".to_string(),
        arvak_hal::job::JobStatus::Running => "running".to_string(),
        arvak_hal::job::JobStatus::Completed => "completed".to_string(),
        arvak_hal::job::JobStatus::Failed(_) => "failed".to_string(),
        arvak_hal::job::JobStatus::Cancelled => "cancelled".to_string(),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────

async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn list_backends_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Response> {
    let backend_ids = state.backends.list();
    let mut backends = Vec::new();

    for id in backend_ids {
        let backend = state.backends.get(&id).map_err(|_| {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "Backend lookup failed")
        })?;

        let caps = backend.capabilities();
        let is_available = backend.availability().await.is_ok_and(|a| a.is_available);

        let mut supported_gates = caps.gate_set.single_qubit.clone();
        supported_gates.extend(caps.gate_set.two_qubit.clone());
        supported_gates.extend(caps.gate_set.three_qubit.iter().cloned());

        backends.push(BackendSummary {
            backend_id: id,
            name: caps.name.clone(),
            is_available,
            max_qubits: caps.num_qubits,
            max_shots: caps.max_shots,
            supported_gates,
        });
    }

    Ok(Json(ListBackendsResponse { backends }))
}

async fn get_backend_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Response> {
    let backend = state
        .backends
        .get(&id)
        .map_err(|_| error_response(StatusCode::NOT_FOUND, format!("Backend not found: {id}")))?;

    let caps = backend.capabilities();
    let is_available = backend.availability().await.is_ok_and(|a| a.is_available);
    let topology_json = serde_json::to_string(&caps.topology).unwrap_or_else(|_| "{}".to_string());

    let mut supported_gates = caps.gate_set.single_qubit.clone();
    supported_gates.extend(caps.gate_set.two_qubit.clone());
    supported_gates.extend(caps.gate_set.three_qubit.iter().cloned());

    Ok(Json(BackendDetailResponse {
        backend_id: id,
        name: caps.name.clone(),
        is_available,
        max_qubits: caps.num_qubits,
        max_shots: caps.max_shots,
        supported_gates,
        topology_json,
    }))
}

async fn compile_handler(
    State(state): State<AppState>,
    Json(req): Json<CompileRequest>,
) -> Result<impl IntoResponse, Response> {
    // Parse QASM3
    let circuit = arvak_qasm3::parse(&req.qasm3)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, format!("QASM3 parse error: {e}")))?;

    // Resolve backend for compilation target
    let backend = state.backends.get(&req.backend_id).map_err(|_| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Backend not found: {}", req.backend_id),
        )
    })?;

    // Compile (CPU-bound work on spawn_blocking via circuit_utils)
    let compiled =
        circuit_utils::compile_for_backend(circuit, backend.as_ref(), req.optimization_level)
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Compilation failed: {e}"),
                )
            })?;

    let stats = CompileStats {
        num_qubits: u32::try_from(compiled.num_qubits()).unwrap_or(u32::MAX),
        depth: u32::try_from(compiled.depth()).unwrap_or(u32::MAX),
        gate_count: compiled.dag().num_ops(),
    };

    let compiled_qasm3 = arvak_qasm3::emit(&compiled).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("QASM3 emit error: {e}"),
        )
    })?;

    Ok(Json(CompileResponse {
        compiled_qasm3,
        stats,
    }))
}

async fn submit_job_handler(
    State(state): State<AppState>,
    Json(req): Json<SubmitJobRequest>,
) -> Result<impl IntoResponse, Response> {
    // Check resource limits
    if let Some(ref resources) = state.resources {
        resources
            .check_can_submit(None)
            .await
            .map_err(|e| error_response(StatusCode::TOO_MANY_REQUESTS, e.to_string()))?;
    }

    // Parse QASM3
    let circuit = arvak_qasm3::parse(&req.qasm3)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, format!("QASM3 parse error: {e}")))?;

    // Resolve backend
    let backend = state.backends.get(&req.backend_id).map_err(|_| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Backend not found: {}", req.backend_id),
        )
    })?;

    // Compile
    let circuit =
        circuit_utils::compile_for_backend(circuit, backend.as_ref(), req.optimization_level)
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Compilation failed: {e}"),
                )
            })?;

    // Create job
    let job_id = state
        .job_store
        .create_job(circuit, req.backend_id.clone(), req.shots)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Job creation failed: {e}"),
            )
        })?;

    info!(job_id = %job_id.0, backend = %req.backend_id, shots = req.shots, "REST job submitted");
    state.metrics.record_job_submitted(&req.backend_id);

    if let Some(ref resources) = state.resources {
        resources.job_submitted(None).await;
    }

    // Spawn async execution
    crate::server::service::job_execution::spawn_job_execution(
        state.job_store.clone(),
        backend,
        job_id.clone(),
        state.metrics.clone(),
        state.resources.clone(),
        state.abort_handles.clone(),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(SubmitJobResponse { job_id: job_id.0 }),
    ))
}

async fn get_job_status_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Response> {
    let job_id = arvak_hal::job::JobId::new(id);
    let job = state.job_store.get_job(&job_id).await.map_err(|_| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Job not found: {}", job_id.0),
        )
    })?;

    let error_message = match &job.status {
        arvak_hal::job::JobStatus::Failed(msg) => Some(msg.clone()),
        _ => None,
    };

    Ok(Json(JobStatusResponse {
        job_id: job.id.0,
        status: job_status_string(&job.status),
        backend_id: job.backend_id,
        shots: job.shots,
        submitted_at: job.submitted_at.timestamp(),
        started_at: job.started_at.map(|t| t.timestamp()),
        completed_at: job.completed_at.map(|t| t.timestamp()),
        error_message,
    }))
}

async fn get_job_result_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Response> {
    let job_id = arvak_hal::job::JobId::new(id.clone());

    let result = state.job_store.get_result(&job_id).await.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not completed") {
            // Job exists but hasn't finished yet — 202 Accepted signals "try again later"
            (
                StatusCode::ACCEPTED,
                axum::Json(types::ErrorResponse {
                    error: msg,
                    code: 202,
                }),
            )
                .into_response()
        } else if msg.contains("not found") {
            error_response(StatusCode::NOT_FOUND, msg)
        } else {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    let mut counts = std::collections::HashMap::new();
    for (k, v) in result.counts.iter() {
        counts.insert(k.clone(), *v);
    }

    Ok(Json(JobResultResponse {
        job_id: id,
        counts,
        shots: result.shots,
        execution_time_ms: result.execution_time_ms,
    }))
}

async fn cancel_job_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Response> {
    let job_id = arvak_hal::job::JobId::new(id);

    let job = state.job_store.get_job(&job_id).await.map_err(|_| {
        error_response(
            StatusCode::NOT_FOUND,
            format!("Job not found: {}", job_id.0),
        )
    })?;

    if job.status.is_terminal() {
        return Ok(Json(CancelJobResponse {
            success: false,
            message: format!(
                "Job already in terminal state: {}",
                job_status_string(&job.status)
            ),
        }));
    }

    // Abort running task if handle exists
    if let Some(handle) = state.abort_handles.write().await.remove(&job_id.0) {
        handle.abort();
    }

    state
        .job_store
        .update_status(&job_id, arvak_hal::job::JobStatus::Cancelled)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Cancel failed: {e}"),
            )
        })?;

    Ok(Json(CancelJobResponse {
        success: true,
        message: "Job cancelled successfully".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_serialization() {
        let resp = HealthResponse {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("healthy"));
    }

    #[test]
    fn test_error_response_serialization() {
        let resp = ErrorResponse {
            error: "not found".to_string(),
            code: 404,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("404"));
    }

    #[test]
    fn test_job_status_string() {
        assert_eq!(
            job_status_string(&arvak_hal::job::JobStatus::Queued),
            "queued"
        );
        assert_eq!(
            job_status_string(&arvak_hal::job::JobStatus::Running),
            "running"
        );
        assert_eq!(
            job_status_string(&arvak_hal::job::JobStatus::Completed),
            "completed"
        );
        assert_eq!(
            job_status_string(&arvak_hal::job::JobStatus::Failed("err".to_string())),
            "failed"
        );
        assert_eq!(
            job_status_string(&arvak_hal::job::JobStatus::Cancelled),
            "cancelled"
        );
    }
}
