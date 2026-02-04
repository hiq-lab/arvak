//! IQM Resonance API client.
//!
//! This module implements the IQM Resonance REST API for submitting
//! quantum circuits and retrieving results.

// Allow dead code for API response fields that are deserialized but not yet used.
// These fields are part of the IQM API contract and may be useful in the future.
#![allow(dead_code)]

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument};

use crate::error::{IqmError, IqmResult};

/// IQM Resonance API client.
#[derive(Debug, Clone)]
pub struct IqmClient {
    /// HTTP client.
    client: Client,
    /// API base URL.
    base_url: String,
    /// Authentication token.
    token: String,
}

impl IqmClient {
    /// Create a new IQM client.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> IqmResult<Self> {
        let base_url = base_url.into();
        let token = token.into();

        if token.is_empty() {
            return Err(IqmError::MissingToken);
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(IqmError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// Submit a circuit for execution.
    #[instrument(skip(self, request))]
    pub async fn submit_job(&self, request: &SubmitRequest) -> IqmResult<SubmitResponse> {
        let url = format!("{}/jobs", self.base_url);
        debug!("Submitting job to {}", url);

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(request)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get the status of a job.
    #[instrument(skip(self))]
    pub async fn get_job_status(&self, job_id: &str) -> IqmResult<JobStatusResponse> {
        let url = format!("{}/jobs/{}/status", self.base_url, job_id);
        debug!("Getting job status from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get the result of a completed job.
    #[instrument(skip(self))]
    pub async fn get_job_result(&self, job_id: &str) -> IqmResult<JobResultResponse> {
        let url = format!("{}/jobs/{}", self.base_url, job_id);
        debug!("Getting job result from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Cancel a job.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> IqmResult<()> {
        let url = format!("{}/jobs/{}/cancel", self.base_url, job_id);
        debug!("Cancelling job at {}", url);

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            Err(IqmError::ApiError { status, message })
        }
    }

    /// Get backend information.
    #[instrument(skip(self))]
    pub async fn get_backend_info(&self, backend_name: &str) -> IqmResult<BackendInfo> {
        let url = format!("{}/quantum-computers/{}", self.base_url, backend_name);
        debug!("Getting backend info from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// List available backends.
    #[instrument(skip(self))]
    pub async fn list_backends(&self) -> IqmResult<Vec<BackendInfo>> {
        let url = format!("{}/quantum-computers", self.base_url);
        debug!("Listing backends from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Handle HTTP response, extracting JSON or returning error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> IqmResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let message = response.text().await.unwrap_or_default();

            match status {
                StatusCode::UNAUTHORIZED => Err(IqmError::AuthFailed(message)),
                StatusCode::NOT_FOUND => Err(IqmError::JobNotFound(message)),
                _ => Err(IqmError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
            }
        }
    }
}

/// Request to submit a job.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitRequest {
    /// Target quantum computer.
    pub backend: String,
    /// Circuit in OpenQASM 3 format.
    pub program: String,
    /// Number of shots.
    pub shots: u32,
    /// Optional job name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional job tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Execution options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ExecutionOptions>,
}

impl SubmitRequest {
    /// Create a new submit request.
    pub fn new(backend: impl Into<String>, program: impl Into<String>, shots: u32) -> Self {
        Self {
            backend: backend.into(),
            program: program.into(),
            shots,
            name: None,
            tags: None,
            options: None,
        }
    }

    /// Set job name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set job tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Set execution options.
    pub fn with_options(mut self, options: ExecutionOptions) -> Self {
        self.options = Some(options);
        self
    }
}

/// Execution options for job submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Circuit optimization level (0-3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimization_level: Option<u8>,
    /// Maximum execution time in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    /// Use error mitigation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_mitigation: Option<bool>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            optimization_level: Some(1),
            max_execution_time: None,
            error_mitigation: None,
        }
    }
}

/// Response from job submission.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitResponse {
    /// Job identifier.
    pub id: String,
    /// Job status.
    pub status: String,
    /// Estimated queue position.
    #[serde(default)]
    pub queue_position: Option<u32>,
    /// Estimated wait time in seconds.
    #[serde(default)]
    pub estimated_wait_time: Option<u64>,
}

/// Job status response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStatusResponse {
    /// Job identifier.
    pub id: String,
    /// Current status.
    pub status: String,
    /// Status message.
    #[serde(default)]
    pub message: Option<String>,
    /// Progress (0.0 to 1.0).
    #[serde(default)]
    pub progress: Option<f64>,
}

impl JobStatusResponse {
    /// Check if job is pending.
    pub fn is_pending(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "pending" | "queued" | "running" | "executing"
        )
    }

    /// Check if job completed successfully.
    pub fn is_completed(&self) -> bool {
        self.status.to_lowercase() == "completed" || self.status.to_lowercase() == "ready"
    }

    /// Check if job failed.
    pub fn is_failed(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "failed" | "error" | "aborted"
        )
    }

    /// Check if job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status.to_lowercase() == "cancelled"
    }
}

/// Job result response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobResultResponse {
    /// Job identifier.
    pub id: String,
    /// Final status.
    pub status: String,
    /// Measurement results.
    #[serde(default)]
    pub measurements: Option<Vec<MeasurementResult>>,
    /// Aggregated counts.
    #[serde(default)]
    pub counts: Option<HashMap<String, u64>>,
    /// Error message if failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Execution metadata.
    #[serde(default)]
    pub metadata: Option<JobMetadata>,
}

/// Single measurement result.
#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementResult {
    /// Register name.
    pub register: String,
    /// Bit values as list of lists (shots x bits).
    pub values: Vec<Vec<u8>>,
}

/// Job execution metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct JobMetadata {
    /// Execution time in milliseconds.
    #[serde(default)]
    pub execution_time_ms: Option<u64>,
    /// Time spent in queue in seconds.
    #[serde(default)]
    pub queue_time_s: Option<u64>,
    /// Backend used.
    #[serde(default)]
    pub backend: Option<String>,
    /// Number of shots executed.
    #[serde(default)]
    pub shots: Option<u32>,
    /// Calibration data snapshot timestamp.
    #[serde(default)]
    pub calibration_timestamp: Option<String>,
}

/// Backend information.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendInfo {
    /// Backend name.
    pub name: String,
    /// Number of qubits.
    pub num_qubits: u32,
    /// Backend status.
    pub status: String,
    /// Native gate set.
    #[serde(default)]
    pub native_gates: Vec<String>,
    /// Connectivity (coupling map).
    #[serde(default)]
    pub connectivity: Vec<(u32, u32)>,
    /// Maximum shots per job.
    #[serde(default)]
    pub max_shots: Option<u32>,
    /// Maximum circuit depth.
    #[serde(default)]
    pub max_circuit_depth: Option<u32>,
    /// Backend version/generation.
    #[serde(default)]
    pub generation: Option<String>,
}

impl BackendInfo {
    /// Check if the backend is online.
    pub fn is_online(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "online" | "available" | "ready"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_request() {
        let request = SubmitRequest::new("garnet", "OPENQASM 3.0; qubit[2] q;", 1000)
            .with_name("test-job")
            .with_tags(vec!["test".into()]);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("garnet"));
        assert!(json.contains("1000"));
    }

    #[test]
    fn test_job_status_response() {
        let status = JobStatusResponse {
            id: "job-123".into(),
            status: "running".into(),
            message: None,
            progress: Some(0.5),
        };

        assert!(status.is_pending());
        assert!(!status.is_completed());
    }
}
