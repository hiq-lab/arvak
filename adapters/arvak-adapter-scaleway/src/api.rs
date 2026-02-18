//! Scaleway Quantum-as-a-Service REST API client.
//!
//! Implements the Scaleway QaaS v1alpha1 API for submitting quantum circuits
//! and retrieving results. Scaleway wraps IQM and Pasqal hardware behind a
//! session-based execution model.
//!
//! ## Submission flow
//!
//! 1. Compress QASM3 circuit (zlib + base64)
//! 2. Wrap in `QuantumComputationModel` JSON
//! 3. `POST /models` → get `model.id`
//! 4. `POST /jobs` with `model_id` + parameters → get `job.id`
//! 5. Poll `GET /jobs/{id}` until terminal state
//! 6. Read results from `result_distribution` or `GET /jobs/{id}/results`

// Allow dead code for API response fields that are deserialized but not yet used.
#![allow(dead_code)]

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tracing::{debug, instrument};

use crate::error::{ScalewayError, ScalewayResult};

/// Scaleway QaaS API base URL.
pub const BASE_URL: &str = "https://api.scaleway.com";

/// QaaS API version path.
const API_PATH: &str = "/qaas/v1alpha1";

/// User agent string for Arvak submissions.
const USER_AGENT: &str = "arvak-adapter-scaleway/1.7";

/// Scaleway QaaS API client.
#[derive(Clone)]
pub struct ScalewayClient {
    /// HTTP client.
    client: Client,
    /// API base URL (default: https://api.scaleway.com).
    base_url: String,
    /// Scaleway secret key (used as X-Auth-Token).
    secret_key: String,
    /// Scaleway project ID.
    project_id: String,
}

impl std::fmt::Debug for ScalewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScalewayClient")
            .field("base_url", &self.base_url)
            .field("project_id", &self.project_id)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

impl ScalewayClient {
    /// Create a new Scaleway QaaS client.
    pub fn new(
        secret_key: impl Into<String>,
        project_id: impl Into<String>,
    ) -> ScalewayResult<Self> {
        let secret_key = secret_key.into();
        let project_id = project_id.into();

        if secret_key.is_empty() {
            return Err(ScalewayError::MissingToken);
        }
        if project_id.is_empty() {
            return Err(ScalewayError::MissingProjectId);
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(ScalewayError::Http)?;

        Ok(Self {
            client,
            base_url: BASE_URL.to_string(),
            secret_key,
            project_id,
        })
    }

    /// Override the base URL (for testing).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// Get the project ID.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Build the full API URL for an endpoint.
    fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base_url, API_PATH, path)
    }

    // ─── Session management ─────────────────────────────────────────

    /// Get session status.
    #[instrument(skip(self))]
    pub async fn get_session(&self, session_id: &str) -> ScalewayResult<SessionResponse> {
        let url = self.url(&format!("/sessions/{session_id}"));
        debug!("Getting session from {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Auth-Token", &self.secret_key)
            .send()
            .await?;

        self.handle_response(response).await
    }

    // ─── Model management ───────────────────────────────────────────

    /// Upload a computation model (compressed circuit + metadata).
    ///
    /// This must be called before `create_job`. The returned model ID is
    /// passed to `create_job` to reference the circuit.
    #[instrument(skip(self, payload))]
    pub async fn create_model(&self, payload: &str) -> ScalewayResult<ModelResponse> {
        let url = self.url("/models");
        debug!("Creating model at {}", url);

        let body = CreateModelRequest {
            project_id: self.project_id.clone(),
            payload: payload.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .header("X-Auth-Token", &self.secret_key)
            .json(&body)
            .send()
            .await?;

        self.handle_response(response).await
    }

    // ─── Job management ─────────────────────────────────────────────

    /// Submit a job referencing a previously uploaded model.
    #[instrument(skip(self, request))]
    pub async fn create_job(&self, request: &CreateJobRequest) -> ScalewayResult<JobResponse> {
        let url = self.url("/jobs");
        debug!("Creating job at {}", url);

        let response = self
            .client
            .post(&url)
            .header("X-Auth-Token", &self.secret_key)
            .json(request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get job status.
    #[instrument(skip(self))]
    pub async fn get_job(&self, job_id: &str) -> ScalewayResult<JobResponse> {
        let url = self.url(&format!("/jobs/{job_id}"));
        debug!("Getting job from {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Auth-Token", &self.secret_key)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// List results for a completed job.
    #[instrument(skip(self))]
    pub async fn list_job_results(&self, job_id: &str) -> ScalewayResult<ListJobResultsResponse> {
        let url = self.url(&format!("/jobs/{job_id}/results"));
        debug!("Getting job results from {}", url);

        let response = self
            .client
            .get(&url)
            .header("X-Auth-Token", &self.secret_key)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Cancel a running job.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> ScalewayResult<JobResponse> {
        let url = self.url(&format!("/jobs/{job_id}/cancel"));
        debug!("Cancelling job at {}", url);

        let response = self
            .client
            .post(&url)
            .header("X-Auth-Token", &self.secret_key)
            .json(&serde_json::json!({}))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Handle HTTP response, extracting JSON or returning an error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> ScalewayResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let message = response.text().await.unwrap_or_default();

            match status {
                StatusCode::UNAUTHORIZED => Err(ScalewayError::AuthFailed(message)),
                StatusCode::NOT_FOUND => Err(ScalewayError::JobNotFound(message)),
                StatusCode::FORBIDDEN if message.contains("quota") => {
                    Err(ScalewayError::QuotaExceeded(message))
                }
                StatusCode::FORBIDDEN => Err(ScalewayError::AuthFailed(message)),
                _ => Err(ScalewayError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
            }
        }
    }
}

// ─── Circuit compression ────────────────────────────────────────────

/// Compress a QASM3 string with zlib and encode as base64.
///
/// This matches the format expected by Scaleway QaaS:
/// `CompressionFormat.ZLIB_BASE64_V1` (enum value 2).
pub fn compress_qasm(qasm: &str) -> ScalewayResult<String> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(qasm.as_bytes())
        .map_err(|e| ScalewayError::QasmError(format!("zlib compression failed: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| ScalewayError::QasmError(format!("zlib finalize failed: {e}")))?;
    Ok(BASE64.encode(compressed))
}

/// Build the `QuantumComputationModel` JSON payload that Scaleway expects.
///
/// This wraps a compressed QASM3 circuit in the structure that the
/// `POST /models` endpoint accepts.
pub fn build_computation_model(compressed_qasm: &str, backend_name: &str) -> serde_json::Value {
    serde_json::json!({
        "programs": [{
            "serialization_format": 3,   // QASM_V3
            "compression_format": 2,     // ZLIB_BASE64_V1
            "serialization": compressed_qasm
        }],
        "backend": {
            "name": backend_name,
            "version": null,
            "options": {}
        },
        "client": {
            "user_agent": USER_AGENT
        },
        "noise_model": null
    })
}

/// Build the `QuantumComputationParameters` JSON string.
pub fn build_computation_parameters(shots: u32) -> String {
    serde_json::json!({
        "shots": shots,
        "options": {}
    })
    .to_string()
}

// ─── Request types ──────────────────────────────────────────────────

/// Request body for creating a model.
#[derive(Debug, Clone, Serialize)]
pub struct CreateModelRequest {
    /// Scaleway project ID.
    pub project_id: String,
    /// Serialized `QuantumComputationModel` JSON string.
    pub payload: String,
}

/// Request body for creating a job.
#[derive(Debug, Clone, Serialize)]
pub struct CreateJobRequest {
    /// Job name.
    pub name: String,
    /// Session to run the job in.
    pub session_id: String,
    /// Model ID (from `create_model` response).
    pub model_id: String,
    /// Execution parameters as JSON string (shots, options).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,
    /// Optional tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Maximum job duration (e.g., "120s").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration: Option<String>,
}

impl CreateJobRequest {
    /// Create a job request referencing a model.
    pub fn new(session_id: impl Into<String>, model_id: impl Into<String>, shots: u32) -> Self {
        Self {
            name: format!("arvak-{}", uuid::Uuid::new_v4()),
            session_id: session_id.into(),
            model_id: model_id.into(),
            parameters: Some(build_computation_parameters(shots)),
            tags: Some(vec!["arvak".into()]),
            max_duration: None,
        }
    }
}

// ─── Response types ─────────────────────────────────────────────────

/// Model upload response.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelResponse {
    /// Model ID — pass this to `create_job`.
    pub id: String,
    /// Creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Storage URL (if applicable).
    #[serde(default)]
    pub url: Option<String>,
    /// Project ID.
    #[serde(default)]
    pub project_id: Option<String>,
}

/// Session response.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionResponse {
    /// Session ID.
    pub id: String,
    /// Session name.
    #[serde(default)]
    pub name: Option<String>,
    /// Platform ID (e.g., "QPU-GARNET-20PQ").
    #[serde(default)]
    pub platform_id: Option<String>,
    /// Session status.
    pub status: String,
    /// Number of waiting jobs.
    #[serde(default)]
    pub waiting_job_count: Option<u32>,
    /// Number of finished jobs.
    #[serde(default)]
    pub finished_job_count: Option<u32>,
    /// Progress message from the platform.
    #[serde(default)]
    pub progress_message: Option<String>,
    /// Creation timestamp (RFC 3339).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Session start timestamp.
    #[serde(default)]
    pub started_at: Option<String>,
    /// Termination timestamp.
    #[serde(default)]
    pub terminated_at: Option<String>,
}

impl SessionResponse {
    /// Check if session is active and accepting jobs.
    pub fn is_running(&self) -> bool {
        self.status.to_lowercase() == "running"
    }

    /// Check if session is starting up.
    pub fn is_starting(&self) -> bool {
        self.status.to_lowercase() == "starting"
    }

    /// Check if session has stopped.
    pub fn is_stopped(&self) -> bool {
        matches!(self.status.to_lowercase().as_str(), "stopped" | "stopping")
    }
}

/// Job response (from create, get, or cancel).
#[derive(Debug, Clone, Deserialize)]
pub struct JobResponse {
    /// Job ID.
    pub id: String,
    /// Job name.
    #[serde(default)]
    pub name: Option<String>,
    /// Session this job belongs to.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Job status: waiting, running, completed, error, cancelled, cancelling.
    pub status: String,
    /// Progress message.
    #[serde(default)]
    pub progress_message: Option<String>,
    /// Job execution duration (e.g., "2.5s").
    #[serde(default)]
    pub job_duration: Option<String>,
    /// Result distribution — JSON-encoded bitstring→count map.
    /// Present inline for small results when status == "completed".
    #[serde(default)]
    pub result_distribution: Option<serde_json::Value>,
    /// Creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Start timestamp.
    #[serde(default)]
    pub started_at: Option<String>,
    /// Last update timestamp.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

impl JobResponse {
    /// Check if job is still pending (waiting or running).
    pub fn is_pending(&self) -> bool {
        matches!(self.status.to_lowercase().as_str(), "waiting" | "running")
    }

    /// Check if job completed successfully.
    pub fn is_completed(&self) -> bool {
        self.status.to_lowercase() == "completed"
    }

    /// Check if job failed.
    pub fn is_failed(&self) -> bool {
        self.status.to_lowercase() == "error"
    }

    /// Check if job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "cancelled" | "cancelling"
        )
    }
}

/// Response from listing job results.
#[derive(Debug, Clone, Deserialize)]
pub struct ListJobResultsResponse {
    /// Total number of result entries.
    #[serde(default)]
    pub total_count: u32,
    /// Result entries.
    #[serde(default)]
    pub job_results: Vec<JobResultEntry>,
}

/// A single job result entry.
#[derive(Debug, Clone, Deserialize)]
pub struct JobResultEntry {
    /// Job ID.
    #[serde(default)]
    pub job_id: Option<String>,
    /// Result data — JSON-encoded measurement counts or raw data.
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    /// URL to download large results.
    #[serde(default)]
    pub url: Option<String>,
    /// Timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_qasm_roundtrip() {
        let qasm = "OPENQASM 3.0;\ninclude \"stdgates.inc\";\nqubit[2] q;\nh q[0];\ncx q[0], q[1];\nbit[2] c;\nc = measure q;";
        let compressed = compress_qasm(qasm).unwrap();

        // Verify it's valid base64
        let decoded = BASE64.decode(&compressed).unwrap();
        assert!(!decoded.is_empty());

        // Decompress and verify roundtrip
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        let mut decoder = ZlibDecoder::new(&decoded[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).unwrap();
        assert_eq!(decompressed, qasm);
    }

    #[test]
    fn test_build_computation_model() {
        let compressed = compress_qasm("OPENQASM 3.0;").unwrap();
        let model = build_computation_model(&compressed, "garnet");

        let programs = model["programs"].as_array().unwrap();
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0]["serialization_format"], 3);
        assert_eq!(programs[0]["compression_format"], 2);
        assert_eq!(programs[0]["serialization"].as_str().unwrap(), compressed);
        assert_eq!(model["backend"]["name"], "garnet");
        assert_eq!(model["client"]["user_agent"], USER_AGENT);
    }

    #[test]
    fn test_build_computation_parameters() {
        let params = build_computation_parameters(4000);
        let parsed: serde_json::Value = serde_json::from_str(&params).unwrap();
        assert_eq!(parsed["shots"], 4000);
    }

    #[test]
    fn test_create_job_request_serialization() {
        let request = CreateJobRequest::new("session-123", "model-456", 4000);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("session-123"));
        assert!(json.contains("model-456"));
        assert!(json.contains("4000"));
        assert!(json.contains("arvak"));
        // Should NOT contain circuit field — uses model_id instead
        assert!(!json.contains("qiskit_circuit"));
    }

    #[test]
    fn test_job_response_status_methods() {
        let waiting = JobResponse {
            id: "j1".into(),
            name: None,
            session_id: None,
            status: "waiting".into(),
            progress_message: None,
            job_duration: None,
            result_distribution: None,
            created_at: None,
            started_at: None,
            updated_at: None,
            tags: None,
        };
        assert!(waiting.is_pending());
        assert!(!waiting.is_completed());

        let completed = JobResponse {
            status: "completed".into(),
            ..waiting.clone()
        };
        assert!(completed.is_completed());
        assert!(!completed.is_pending());

        let error = JobResponse {
            status: "error".into(),
            ..waiting.clone()
        };
        assert!(error.is_failed());

        let cancelled = JobResponse {
            status: "cancelled".into(),
            ..waiting
        };
        assert!(cancelled.is_cancelled());
    }

    #[test]
    fn test_session_response_status() {
        let session = SessionResponse {
            id: "s1".into(),
            name: None,
            platform_id: Some("QPU-GARNET-20PQ".into()),
            status: "running".into(),
            waiting_job_count: None,
            finished_job_count: None,
            progress_message: None,
            created_at: None,
            started_at: None,
            terminated_at: None,
        };
        assert!(session.is_running());
        assert!(!session.is_starting());
        assert!(!session.is_stopped());
    }

    #[test]
    fn test_parse_result_distribution() {
        use std::collections::HashMap;
        let json = r#"{"00": 2048, "11": 1952}"#;
        let counts: HashMap<String, u64> = serde_json::from_str(json).unwrap();
        assert_eq!(counts["00"], 2048);
        assert_eq!(counts["11"], 1952);
    }
}
