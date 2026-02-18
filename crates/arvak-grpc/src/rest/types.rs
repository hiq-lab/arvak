//! JSON request/response types for the REST gateway.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Requests ──────────────────────────────────────────────────────────────

/// POST /v1/compile
#[derive(Debug, Deserialize)]
pub struct CompileRequest {
    /// OpenQASM 3 circuit string.
    pub qasm3: String,
    /// Target backend ID (used to determine coupling map / basis gates).
    pub backend_id: String,
    /// Optimization level (0–3). 0 = no compilation.
    #[serde(default = "default_optimization_level")]
    pub optimization_level: u32,
}

/// POST /v1/jobs
#[derive(Debug, Deserialize)]
pub struct SubmitJobRequest {
    /// OpenQASM 3 circuit string.
    pub qasm3: String,
    /// Target backend ID.
    pub backend_id: String,
    /// Number of shots.
    #[serde(default = "default_shots")]
    pub shots: u32,
    /// Optimization level (0–3).
    #[serde(default = "default_optimization_level")]
    pub optimization_level: u32,
}

fn default_shots() -> u32 {
    1024
}

fn default_optimization_level() -> u32 {
    1
}

// ── Responses ─────────────────────────────────────────────────────────────

/// GET /v1/health
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Backend summary in list response.
#[derive(Debug, Serialize)]
pub struct BackendSummary {
    pub backend_id: String,
    pub name: String,
    pub is_available: bool,
    pub max_qubits: u32,
    pub max_shots: u32,
    pub supported_gates: Vec<String>,
}

/// GET /v1/backends
#[derive(Debug, Serialize)]
pub struct ListBackendsResponse {
    pub backends: Vec<BackendSummary>,
}

/// GET /v1/backends/{id}
#[derive(Debug, Serialize)]
pub struct BackendDetailResponse {
    pub backend_id: String,
    pub name: String,
    pub is_available: bool,
    pub max_qubits: u32,
    pub max_shots: u32,
    pub supported_gates: Vec<String>,
    pub topology_json: String,
}

/// POST /v1/compile response
#[derive(Debug, Serialize)]
pub struct CompileResponse {
    pub compiled_qasm3: String,
    pub stats: CompileStats,
}

#[derive(Debug, Serialize)]
pub struct CompileStats {
    pub num_qubits: u32,
    pub depth: u32,
    pub gate_count: usize,
}

/// POST /v1/jobs response
#[derive(Debug, Serialize)]
pub struct SubmitJobResponse {
    pub job_id: String,
}

/// GET /v1/jobs/{id} response
#[derive(Debug, Serialize)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub status: String,
    pub backend_id: String,
    pub shots: u32,
    pub submitted_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// GET /v1/jobs/{id}/result response
#[derive(Debug, Serialize)]
pub struct JobResultResponse {
    pub job_id: String,
    pub counts: HashMap<String, u64>,
    pub shots: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
}

/// DELETE /v1/jobs/{id} response
#[derive(Debug, Serialize)]
pub struct CancelJobResponse {
    pub success: bool,
    pub message: String,
}

/// Generic error body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}
