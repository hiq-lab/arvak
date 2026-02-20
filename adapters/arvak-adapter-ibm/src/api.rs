//! IBM Quantum Platform API client.
//!
//! This module implements the IBM Quantum Cloud REST API for:
//! - Authentication via IAM token exchange (new API key flow)
//! - Listing backends and their properties
//! - Submitting jobs (Qiskit Runtime primitives)
//! - Polling job status and retrieving results
//!
//! Supports both the new IBM Cloud API (`quantum.cloud.ibm.com/api`) and the
//! legacy endpoint (`api.quantum-computing.ibm.com`) for backward compatibility.

// Allow dead code for API response fields that are deserialized but not yet used.
// These fields are part of the IBM Quantum API contract and may be useful in the future.
#![allow(dead_code)]

use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::error::{IbmError, IbmResult};

/// Default IBM Quantum Cloud API endpoint (new).
pub const DEFAULT_ENDPOINT: &str = "https://quantum.cloud.ibm.com/api";

/// Legacy IBM Quantum API endpoint.
pub const LEGACY_ENDPOINT: &str = "https://api.quantum-computing.ibm.com";

/// IBM Cloud IAM token endpoint.
const IAM_TOKEN_URL: &str = "https://iam.cloud.ibm.com/identity/token";

/// IBM API version header value.
const IBM_API_VERSION: &str = "2026-02-01";

/// User-Agent sent with requests (Cloudflare blocks default reqwest UA).
const USER_AGENT: &str = "arvak/1.7.2 (quantum-sdk; +https://arvak.io)";

/// IBM Quantum API client.
pub struct IbmClient {
    /// HTTP client.
    client: Client,
    /// API endpoint URL.
    endpoint: String,
    /// Bearer token (either from IAM exchange or direct).
    token: String,
    /// Selected instance (hub/group/project) — legacy mode only.
    instance: Option<String>,
    /// Whether using the new Cloud API (vs legacy).
    cloud_api: bool,
}

impl fmt::Debug for IbmClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IbmClient")
            .field("endpoint", &self.endpoint)
            .field("token", &"[REDACTED]")
            .field("instance", &self.instance)
            .field("cloud_api", &self.cloud_api)
            .finish()
    }
}

/// IAM token response from `iam.cloud.ibm.com`.
#[derive(Debug, Deserialize)]
struct IamTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

impl IbmClient {
    /// Create a new IBM Quantum client using the legacy direct-token mode.
    ///
    /// This connects to the old `api.quantum-computing.ibm.com` endpoint.
    /// For the new IBM Cloud API, use [`IbmClient::connect`] instead.
    pub fn new(endpoint: impl Into<String>, token: impl Into<String>) -> IbmResult<Self> {
        let token = token.into();

        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| IbmError::InvalidToken)?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        let client = Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            endpoint: endpoint.into(),
            token,
            instance: None,
            cloud_api: false,
        })
    }

    /// Create a new IBM Quantum client using the new IBM Cloud API key flow.
    ///
    /// Exchanges the API key for an IAM bearer token and configures the
    /// Service-CRN header required by the new `quantum.cloud.ibm.com/api`.
    pub async fn connect(api_key: &str, service_crn: &str) -> IbmResult<Self> {
        // Exchange API key for IAM bearer token
        let iam_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;

        let iam_response = iam_client
            .post(IAM_TOKEN_URL)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!(
                "grant_type=urn:ibm:params:oauth:grant-type:apikey&apikey={api_key}"
            ))
            .send()
            .await
            .map_err(|e| IbmError::IamTokenExchange(e.to_string()))?;

        if !iam_response.status().is_success() {
            let status = iam_response.status();
            let body = iam_response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(IbmError::IamTokenExchange(format!(
                "IAM returned {status}: {body}"
            )));
        }

        let iam_token: IamTokenResponse = iam_response.json().await.map_err(|e| {
            IbmError::IamTokenExchange(format!("failed to parse IAM response: {e}"))
        })?;

        let bearer_token = iam_token.access_token;

        // Build default headers for the new Cloud API
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&format!("Bearer {bearer_token}"))
                .map_err(|_| IbmError::InvalidToken)?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        // Service-CRN header — required on every request to the new API
        headers.insert(
            header::HeaderName::from_static("service-crn"),
            header::HeaderValue::from_str(service_crn)
                .map_err(|_| IbmError::InvalidParameter("invalid Service-CRN value".into()))?,
        );
        // IBM-API-Version header
        headers.insert(
            header::HeaderName::from_static("ibm-api-version"),
            header::HeaderValue::from_static(IBM_API_VERSION),
        );

        let client = Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            token: bearer_token,
            instance: None,
            cloud_api: true,
        })
    }

    /// Set the instance (hub/group/project) for job submission (legacy mode).
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Whether this client uses the new Cloud API.
    pub fn is_cloud_api(&self) -> bool {
        self.cloud_api
    }

    /// Get available backends.
    ///
    /// On the new Cloud API, this fetches the device list and then retrieves
    /// configuration and status for each backend individually.
    pub async fn list_backends(&self) -> IbmResult<Vec<BackendInfo>> {
        if self.cloud_api {
            self.list_backends_cloud().await
        } else {
            self.list_backends_legacy().await
        }
    }

    /// List backends using the new Cloud API (`{"devices": [...]}`).
    async fn list_backends_cloud(&self) -> IbmResult<Vec<BackendInfo>> {
        let url = format!("{}/v1/backends", self.endpoint);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(IbmError::ApiError {
                code: None,
                message: format!("list backends failed: {body}"),
            });
        }

        let devices: DevicesResponse = response.json().await?;
        let mut backends = Vec::with_capacity(devices.devices.len());

        for device in &devices.devices {
            let device_name = &device.name;
            match self.get_backend(device_name).await {
                Ok(info) => backends.push(info),
                Err(e) => {
                    tracing::warn!("skipping backend {device_name}: {e}");
                }
            }
        }

        Ok(backends)
    }

    /// List backends using the legacy API (`{"backends": [...]}`).
    async fn list_backends_legacy(&self) -> IbmResult<Vec<BackendInfo>> {
        let url = format!("{}/v1/backends", self.endpoint);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        let backends: LegacyBackendsResponse = response.json().await?;
        Ok(backends.backends)
    }

    /// Get details for a specific backend.
    ///
    /// On the new Cloud API, fetches `/configuration` and `/status` separately
    /// and merges into a single `BackendInfo`.
    pub async fn get_backend(&self, name: &str) -> IbmResult<BackendInfo> {
        if self.cloud_api {
            self.get_backend_cloud(name).await
        } else {
            self.get_backend_legacy(name).await
        }
    }

    /// Fetch backend info from the new Cloud API.
    async fn get_backend_cloud(&self, name: &str) -> IbmResult<BackendInfo> {
        // Fetch configuration
        let config_url = format!("{}/v1/backends/{}/configuration", self.endpoint, name);
        let config_response = self.client.get(&config_url).send().await?;

        if !config_response.status().is_success() {
            if config_response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(IbmError::BackendUnavailable(name.to_string()));
            }
            let body = config_response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(IbmError::ApiError {
                code: None,
                message: format!("backend configuration failed for {name}: {body}"),
            });
        }

        let config: BackendConfigResponse = config_response.json().await?;

        // Fetch status
        let status_url = format!("{}/v1/backends/{}/status", self.endpoint, name);
        let status_response = self.client.get(&status_url).send().await?;

        let status = if status_response.status().is_success() {
            let s: BackendStatusResponse = status_response.json().await?;
            BackendStatus {
                operational: s.state,
                status_msg: Some(s.status),
                pending_jobs: Some(u32::try_from(s.length_queue).unwrap_or(u32::MAX)),
            }
        } else {
            // If status fetch fails, assume operational (config succeeded)
            BackendStatus {
                operational: true,
                status_msg: None,
                pending_jobs: None,
            }
        };

        Ok(BackendInfo {
            name: config.backend_name,
            num_qubits: config.n_qubits,
            status,
            processor_type: config.processor_type,
            basis_gates: config.basis_gates,
            coupling_map: config.coupling_map.unwrap_or_default(),
            simulator: config.simulator.unwrap_or(false),
            max_shots: config.max_shots,
            max_circuits: None,
        })
    }

    /// Fetch backend info from the legacy API.
    async fn get_backend_legacy(&self, name: &str) -> IbmResult<BackendInfo> {
        let url = format!("{}/v1/backends/{}", self.endpoint, name);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(IbmError::BackendUnavailable(name.to_string()));
            }
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        response.json().await.map_err(IbmError::from)
    }

    /// Submit a job using the Sampler primitive.
    ///
    /// Uses V2 PUB format for the Cloud API, V1 format for legacy.
    /// When `skip_transpilation` is true, tells IBM to skip its own
    /// transpilation pass (use when the circuit is already compiled).
    pub async fn submit_sampler_job(
        &self,
        backend: &str,
        circuits: Vec<String>,
        shots: u32,
        skip_transpilation: bool,
    ) -> IbmResult<SubmitResponse> {
        let url = format!("{}/v1/jobs", self.endpoint);

        let body = if self.cloud_api {
            // V2 Sampler: PUBs format — each PUB is (circuit, params, shots)
            let pubs: Vec<serde_json::Value> = circuits
                .into_iter()
                .map(|c| serde_json::json!([c, {}, shots]))
                .collect();

            let mut params = serde_json::json!({
                "version": 2,
                "pubs": pubs
            });
            // V2 Sampler requires ISA circuits by default.
            // Use optimization_level 1 to let IBM handle physical routing.
            // Arvak handles basis translation and gate optimization;
            // IBM handles qubit-to-hardware mapping.
            params["options"] = serde_json::json!({
                "optimization_level": 1
            });

            serde_json::json!({
                "program_id": "sampler",
                "backend": backend,
                "params": params
            })
        } else {
            // V1 Sampler: legacy format
            let mut request = serde_json::json!({
                "program_id": "sampler",
                "backend": backend,
                "params": {
                    "circuits": circuits,
                    "shots": shots,
                    "skip_transpilation": skip_transpilation
                }
            });
            if let Some(hub) = &self.instance {
                request["hub"] = serde_json::json!(hub);
            }
            request
        };

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(IbmError::ApiError {
                code: None,
                message: format!("job submission failed: {body}"),
            });
        }

        response.json().await.map_err(IbmError::from)
    }

    /// Get job status.
    pub async fn get_job_status(&self, job_id: &str) -> IbmResult<JobStatusResponse> {
        let url = format!("{}/v1/jobs/{}", self.endpoint, job_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(IbmError::JobNotFound(job_id.to_string()));
            }
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        response.json().await.map_err(IbmError::from)
    }

    /// Get job results.
    pub async fn get_job_results(&self, job_id: &str) -> IbmResult<JobResultResponse> {
        let url = format!("{}/v1/jobs/{}/results", self.endpoint, job_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(IbmError::JobNotFound(job_id.to_string()));
            }
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        response.json().await.map_err(IbmError::from)
    }

    /// Cancel a job.
    pub async fn cancel_job(&self, job_id: &str) -> IbmResult<()> {
        let url = format!("{}/v1/jobs/{}/cancel", self.endpoint, job_id);

        let response = self.client.post(&url).send().await?;

        if !response.status().is_success() {
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        Ok(())
    }
}

// ============================================================================
// Request types
// ============================================================================

/// Sampler job request.
#[derive(Debug, Serialize)]
struct SamplerJobRequest {
    /// Program ID (sampler or estimator).
    program_id: String,
    /// Backend name.
    backend: String,
    /// Instance (hub/group/project).
    #[serde(skip_serializing_if = "Option::is_none")]
    hub: Option<String>,
    /// Sampler parameters.
    params: SamplerParams,
}

/// Sampler primitive parameters.
#[derive(Debug, Serialize)]
struct SamplerParams {
    /// `OpenQASM` 3.0 circuits.
    circuits: Vec<String>,
    /// Number of shots.
    #[serde(skip_serializing_if = "Option::is_none")]
    shots: Option<u32>,
    /// Skip transpilation (circuit already compiled).
    #[serde(skip_serializing_if = "Option::is_none")]
    skip_transpilation: Option<bool>,
}

// ============================================================================
// Response types
// ============================================================================

/// API error response.
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    /// Error code.
    #[serde(default)]
    code: Option<String>,
    /// Error message.
    #[serde(default)]
    message: String,
}

/// New Cloud API: device list response (`{"devices": [...]}`).
#[derive(Debug, Deserialize)]
struct DevicesResponse {
    /// List of devices (objects with name + metadata).
    devices: Vec<DeviceEntry>,
}

/// A device entry in the Cloud API listing.
#[derive(Debug, Deserialize)]
struct DeviceEntry {
    /// Device name (e.g. "ibm_torino").
    name: String,
}

/// Legacy API: backends list response (`{"backends": [...]}`).
#[derive(Debug, Deserialize)]
struct LegacyBackendsResponse {
    /// List of backends.
    backends: Vec<BackendInfo>,
}

/// New Cloud API: backend configuration response from `/backends/{name}/configuration`.
#[derive(Debug, Deserialize)]
struct BackendConfigResponse {
    /// Backend name.
    backend_name: String,
    /// Number of qubits.
    n_qubits: usize,
    /// Basis gates.
    #[serde(default)]
    basis_gates: Vec<String>,
    /// Coupling map (pairs of connected qubits).
    #[serde(default)]
    coupling_map: Option<Vec<[usize; 2]>>,
    /// Processor type.
    #[serde(default)]
    processor_type: Option<ProcessorType>,
    /// Whether this is a simulator.
    #[serde(default)]
    simulator: Option<bool>,
    /// Maximum number of shots.
    #[serde(default)]
    max_shots: Option<u32>,
}

/// New Cloud API: backend status response from `/backends/{name}/status`.
#[derive(Debug, Deserialize)]
struct BackendStatusResponse {
    /// Whether the backend is operational.
    state: bool,
    /// Status string (e.g., "active").
    #[serde(default)]
    status: String,
    /// Status message.
    #[serde(default)]
    message: String,
    /// Queue length.
    #[serde(default)]
    length_queue: u64,
}

/// Backend information.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendInfo {
    /// Backend name.
    pub name: String,
    /// Number of qubits.
    pub num_qubits: usize,
    /// Backend status.
    pub status: BackendStatus,
    /// Processor type.
    #[serde(default)]
    pub processor_type: Option<ProcessorType>,
    /// Basis gates.
    #[serde(default)]
    pub basis_gates: Vec<String>,
    /// Coupling map (pairs of connected qubits).
    #[serde(default)]
    pub coupling_map: Vec<[usize; 2]>,
    /// Whether this is a simulator.
    #[serde(default)]
    pub simulator: bool,
    /// Maximum number of shots.
    #[serde(default)]
    pub max_shots: Option<u32>,
    /// Maximum number of circuits per job.
    #[serde(default)]
    pub max_circuits: Option<u32>,
}

/// Backend status.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendStatus {
    /// Whether the backend is operational.
    pub operational: bool,
    /// Status message.
    #[serde(default)]
    pub status_msg: Option<String>,
    /// Number of pending jobs.
    #[serde(default)]
    pub pending_jobs: Option<u32>,
}

/// Processor type information.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessorType {
    /// Family (e.g., "Falcon", "Eagle", "Heron").
    pub family: String,
    /// Revision.
    #[serde(default)]
    pub revision: Option<String>,
}

/// Job submission response.
#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    /// Job ID.
    pub id: String,
    /// Job status.
    #[serde(default)]
    pub status: String,
}

/// Job status response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStatusResponse {
    /// Job ID.
    pub id: String,
    /// Job status (top-level, may be mixed case on new API).
    pub status: String,
    /// Backend name.
    #[serde(default)]
    pub backend: Option<String>,
    /// Creation time.
    #[serde(default)]
    pub created: Option<String>,
    /// Completion time.
    #[serde(default)]
    pub ended: Option<String>,
    /// Error information if failed (legacy API).
    #[serde(default)]
    pub error: Option<JobError>,
    /// State object with reason (new Cloud API).
    #[serde(default)]
    pub state: Option<JobState>,
}

/// Job error information (legacy API).
#[derive(Debug, Clone, Deserialize)]
pub struct JobError {
    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
    /// Error message.
    pub message: String,
}

/// Job state with reason (new Cloud API).
#[derive(Debug, Clone, Deserialize)]
pub struct JobState {
    /// Status string.
    #[serde(default)]
    pub status: String,
    /// Reason for failure.
    #[serde(default)]
    pub reason: Option<String>,
    /// Reason code.
    #[serde(default)]
    pub reason_code: Option<u32>,
}

impl JobStatusResponse {
    /// Normalized uppercase status for comparison.
    fn normalized_status(&self) -> String {
        self.status.to_uppercase()
    }

    /// Check if job is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.normalized_status().as_str(),
            "COMPLETED" | "FAILED" | "CANCELLED" | "ERROR"
        )
    }

    /// Check if job completed successfully.
    pub fn is_completed(&self) -> bool {
        self.normalized_status() == "COMPLETED"
    }

    /// Check if job failed.
    pub fn is_failed(&self) -> bool {
        matches!(self.normalized_status().as_str(), "FAILED" | "ERROR")
    }

    /// Check if job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.normalized_status() == "CANCELLED"
    }

    /// Get the error/failure reason message.
    pub fn error_message(&self) -> Option<String> {
        // Try new API state.reason first
        if let Some(state) = &self.state {
            if let Some(reason) = &state.reason {
                return Some(reason.clone());
            }
        }
        // Fall back to legacy error.message
        self.error.as_ref().map(|e| e.message.clone())
    }
}

/// Job result response.
#[derive(Debug, Deserialize)]
pub struct JobResultResponse {
    /// Job ID (may be absent in V2 results endpoint).
    #[serde(default)]
    pub id: Option<String>,
    /// Results from sampler primitive.
    pub results: Vec<SamplerResult>,
}

/// Sampler result for one circuit.
#[derive(Debug, Deserialize)]
pub struct SamplerResult {
    /// V2 Sampler data: map of classical register names to sample data.
    /// Each register contains a `samples` array of hex strings (one per shot).
    #[serde(default)]
    pub data: Option<HashMap<String, ClassicalRegisterData>>,
    /// Quasi-probability distribution (bitstring -> probability) — V1 only.
    #[serde(default)]
    pub quasi_dists: Option<Vec<HashMap<String, f64>>>,
    /// Measurement counts (bitstring -> count) — V1 only.
    #[serde(default)]
    pub counts: Option<HashMap<String, u64>>,
    /// Metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Classical register data from V2 Sampler results.
#[derive(Debug, Deserialize)]
pub struct ClassicalRegisterData {
    /// Raw measurement samples as hex strings (e.g., `["0x0", "0x2", ...]`).
    pub samples: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_terminal() {
        let status = JobStatusResponse {
            id: "test".to_string(),
            status: "COMPLETED".to_string(),
            backend: None,
            created: None,
            ended: None,
            error: None,
            state: None,
        };
        assert!(status.is_terminal());
        assert!(status.is_completed());
        assert!(!status.is_failed());

        let failed = JobStatusResponse {
            id: "test".to_string(),
            status: "FAILED".to_string(),
            backend: None,
            created: None,
            ended: None,
            error: Some(JobError {
                code: None,
                message: "Test error".to_string(),
            }),
            state: None,
        };
        assert!(failed.is_terminal());
        assert!(failed.is_failed());
        assert_eq!(failed.error_message().unwrap(), "Test error");
    }

    #[test]
    fn test_job_status_cloud_api_mixed_case() {
        // New Cloud API returns mixed case ("Failed" not "FAILED")
        let status = JobStatusResponse {
            id: "test".to_string(),
            status: "Failed".to_string(),
            backend: None,
            created: None,
            ended: None,
            error: None,
            state: Some(JobState {
                status: "Failed".to_string(),
                reason: Some("circuit too deep".to_string()),
                reason_code: Some(1513),
            }),
        };
        assert!(status.is_terminal());
        assert!(status.is_failed());
        assert_eq!(status.error_message().unwrap(), "circuit too deep");
    }

    #[test]
    fn test_sampler_request_serialization() {
        let request = SamplerJobRequest {
            program_id: "sampler".to_string(),
            backend: "ibm_torino".to_string(),
            hub: None,
            params: SamplerParams {
                circuits: vec!["OPENQASM 3.0; qubit q; h q;".to_string()],
                shots: Some(1000),
                skip_transpilation: Some(false),
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("sampler"));
        assert!(json.contains("ibm_torino"));
        // hub should be omitted when None
        assert!(!json.contains("hub"));
    }

    #[test]
    fn test_devices_response_deserialization() {
        let json = r#"{"devices": [
            {"name": "ibm_fez", "status": {"name": "online"}},
            {"name": "ibm_marrakesh", "status": {"name": "online"}},
            {"name": "ibm_torino", "status": {"name": "online"}}
        ]}"#;
        let resp: DevicesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.devices.len(), 3);
        assert_eq!(resp.devices[2].name, "ibm_torino");
    }

    #[test]
    fn test_backend_config_response_deserialization() {
        let json = r#"{
            "backend_name": "ibm_torino",
            "n_qubits": 133,
            "basis_gates": ["cz", "id", "rx", "rz", "rzz", "sx", "x"],
            "coupling_map": [[0, 1], [1, 0], [1, 2]],
            "simulator": false
        }"#;
        let config: BackendConfigResponse = serde_json::from_str(json).unwrap();
        assert_eq!(config.backend_name, "ibm_torino");
        assert_eq!(config.n_qubits, 133);
        assert_eq!(config.basis_gates.len(), 7);
        assert!(config.coupling_map.is_some());
        assert_eq!(config.simulator, Some(false));
    }

    #[test]
    fn test_backend_status_response_deserialization() {
        let json = r#"{
            "state": true,
            "status": "active",
            "message": "ready",
            "length_queue": 0
        }"#;
        let status: BackendStatusResponse = serde_json::from_str(json).unwrap();
        assert!(status.state);
        assert_eq!(status.status, "active");
        assert_eq!(status.length_queue, 0);
    }

    #[test]
    fn test_default_endpoint_is_cloud() {
        assert!(DEFAULT_ENDPOINT.contains("quantum.cloud.ibm.com"));
    }

    #[test]
    fn test_legacy_client_is_not_cloud() {
        // Cannot actually connect, but verify the flag
        let client = IbmClient::new("https://example.com", "test-token").unwrap();
        assert!(!client.is_cloud_api());
    }
}
