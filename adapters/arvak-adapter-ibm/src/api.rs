//! IBM Quantum Platform API client.
//!
//! This module implements the IBM Quantum Platform REST API for:
//! - Authentication and token management
//! - Listing backends and their properties
//! - Submitting jobs (Qiskit Runtime primitives)
//! - Polling job status and retrieving results

// Allow dead code for API response fields that are deserialized but not yet used.
// These fields are part of the IBM Quantum API contract and may be useful in the future.
#![allow(dead_code)]

use reqwest::{Client, header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::error::{IbmError, IbmResult};

/// Default IBM Quantum API endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://api.quantum-computing.ibm.com";

/// IBM Quantum API client.
pub struct IbmClient {
    /// HTTP client.
    client: Client,
    /// API endpoint URL.
    endpoint: String,
    /// API token.
    token: String,
    /// Selected instance (hub/group/project).
    instance: Option<String>,
}

impl fmt::Debug for IbmClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IbmClient")
            .field("endpoint", &self.endpoint)
            .field("token", &"[REDACTED]")
            .field("instance", &self.instance)
            .finish()
    }
}

impl IbmClient {
    /// Create a new IBM Quantum client.
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
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            endpoint: endpoint.into(),
            token,
            instance: None,
        })
    }

    /// Set the instance (hub/group/project) for job submission.
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Get available backends.
    pub async fn list_backends(&self) -> IbmResult<Vec<BackendInfo>> {
        let url = format!("{}/v1/backends", self.endpoint);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        let backends: BackendsResponse = response.json().await?;
        Ok(backends.backends)
    }

    /// Get details for a specific backend.
    pub async fn get_backend(&self, name: &str) -> IbmResult<BackendInfo> {
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
    pub async fn submit_sampler_job(
        &self,
        backend: &str,
        circuits: Vec<String>,
        shots: u32,
    ) -> IbmResult<SubmitResponse> {
        let url = format!("{}/v1/jobs", self.endpoint);

        // Build the Sampler primitive request
        let request = SamplerJobRequest {
            program_id: "sampler".to_string(),
            backend: backend.to_string(),
            hub: self.instance.clone(),
            params: SamplerParams {
                circuits,
                shots: Some(shots),
                skip_transpilation: Some(false),
            },
        };

        let response = self.client.post(&url).json(&request).send().await?;

        if !response.status().is_success() {
            let error: ApiErrorResponse = response.json().await?;
            return Err(IbmError::ApiError {
                code: error.code,
                message: error.message,
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
    code: Option<String>,
    /// Error message.
    message: String,
}

/// Backends list response.
#[derive(Debug, Deserialize)]
struct BackendsResponse {
    /// List of backends.
    backends: Vec<BackendInfo>,
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
    pub status: String,
}

/// Job status response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStatusResponse {
    /// Job ID.
    pub id: String,
    /// Job status.
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
    /// Error information if failed.
    #[serde(default)]
    pub error: Option<JobError>,
}

/// Job error information.
#[derive(Debug, Clone, Deserialize)]
pub struct JobError {
    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
    /// Error message.
    pub message: String,
}

impl JobStatusResponse {
    /// Check if job is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "COMPLETED" | "FAILED" | "CANCELLED" | "ERROR"
        )
    }

    /// Check if job completed successfully.
    pub fn is_completed(&self) -> bool {
        self.status == "COMPLETED"
    }

    /// Check if job failed.
    pub fn is_failed(&self) -> bool {
        matches!(self.status.as_str(), "FAILED" | "ERROR")
    }

    /// Check if job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status == "CANCELLED"
    }
}

/// Job result response.
#[derive(Debug, Deserialize)]
pub struct JobResultResponse {
    /// Job ID.
    pub id: String,
    /// Results from sampler primitive.
    pub results: Vec<SamplerResult>,
}

/// Sampler result for one circuit.
#[derive(Debug, Deserialize)]
pub struct SamplerResult {
    /// Quasi-probability distribution (bitstring -> probability).
    #[serde(default)]
    pub quasi_dists: Option<Vec<HashMap<String, f64>>>,
    /// Measurement counts (bitstring -> count).
    #[serde(default)]
    pub counts: Option<HashMap<String, u64>>,
    /// Metadata.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
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
        };
        assert!(failed.is_terminal());
        assert!(failed.is_failed());
    }

    #[test]
    fn test_sampler_request_serialization() {
        let request = SamplerJobRequest {
            program_id: "sampler".to_string(),
            backend: "ibm_brisbane".to_string(),
            hub: Some("ibm-q/open/main".to_string()),
            params: SamplerParams {
                circuits: vec!["OPENQASM 3.0; qubit q; h q;".to_string()],
                shots: Some(1000),
                skip_transpilation: Some(false),
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("sampler"));
        assert!(json.contains("ibm_brisbane"));
    }
}
