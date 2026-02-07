//! CUDA-Q REST API client.
//!
//! Communicates with NVIDIA CUDA-Q cloud services for quantum circuit
//! execution on GPU-accelerated simulators and hardware backends.

#![allow(dead_code)]

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument};

use crate::error::{CudaqError, CudaqResult};

/// CUDA-Q REST API client.
#[derive(Debug, Clone)]
pub struct CudaqClient {
    client: Client,
    base_url: String,
    token: String,
}

impl CudaqClient {
    /// Create a new CUDA-Q client.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> CudaqResult<Self> {
        let base_url = base_url.into();
        let token = token.into();

        if token.is_empty() {
            return Err(CudaqError::MissingToken);
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(CudaqError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// Submit a circuit for execution.
    #[instrument(skip(self, request))]
    pub async fn submit_job(&self, request: &SubmitRequest) -> CudaqResult<SubmitResponse> {
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
    pub async fn get_job_status(&self, job_id: &str) -> CudaqResult<JobStatusResponse> {
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
    pub async fn get_job_result(&self, job_id: &str) -> CudaqResult<JobResultResponse> {
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
    pub async fn cancel_job(&self, job_id: &str) -> CudaqResult<()> {
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
            let code = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            Err(CudaqError::Api { code, message })
        }
    }

    /// Get available targets (backends/simulators).
    #[instrument(skip(self))]
    pub async fn get_targets(&self) -> CudaqResult<Vec<TargetInfo>> {
        let url = format!("{}/targets", self.base_url);
        debug!("Getting targets from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Get info about a specific target.
    #[instrument(skip(self))]
    pub async fn get_target(&self, target: &str) -> CudaqResult<TargetInfo> {
        let url = format!("{}/targets/{}", self.base_url, target);
        debug!("Getting target info from {}", url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await?;

        self.handle_response(response).await
    }

    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> CudaqResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let code = status.as_u16();
            let message = response.text().await.unwrap_or_default();

            match status {
                StatusCode::UNAUTHORIZED => Err(CudaqError::MissingToken),
                StatusCode::NOT_FOUND => Err(CudaqError::JobNotFound(message)),
                _ => Err(CudaqError::Api { code, message }),
            }
        }
    }
}

// ── Request / Response types ──────────────────────────────────────────

/// Job submission request.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitRequest {
    /// Target backend or simulator.
    pub target: String,
    /// Circuit program (OpenQASM 3 or QIR).
    pub program: String,
    /// Program format.
    pub format: ProgramFormat,
    /// Number of shots.
    pub shots: u32,
    /// Number of qubits (hint for simulator allocation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_qubits: Option<u32>,
    /// Execution options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ExecutionOptions>,
}

impl SubmitRequest {
    pub fn new(
        target: impl Into<String>,
        program: impl Into<String>,
        format: ProgramFormat,
        shots: u32,
    ) -> Self {
        Self {
            target: target.into(),
            program: program.into(),
            format,
            shots,
            num_qubits: None,
            options: None,
        }
    }

    pub fn with_num_qubits(mut self, n: u32) -> Self {
        self.num_qubits = Some(n);
        self
    }

    pub fn with_options(mut self, options: ExecutionOptions) -> Self {
        self.options = Some(options);
        self
    }
}

/// Supported program formats.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProgramFormat {
    Qasm3,
    Qasm2,
    Qir,
}

/// Execution options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Noise model name (e.g., "depolarizing", "amplitude_damping").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_model: Option<String>,
    /// Seed for deterministic simulation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Number of GPUs to use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_gpus: Option<u32>,
}

/// Job submission response.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitResponse {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub target: Option<String>,
}

/// Job status response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobStatusResponse {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
}

impl JobStatusResponse {
    pub fn is_pending(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "pending" | "queued" | "running" | "executing" | "submitted"
        )
    }

    pub fn is_completed(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "completed" | "ready" | "done"
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "failed" | "error" | "aborted"
        )
    }

    pub fn is_cancelled(&self) -> bool {
        self.status.to_lowercase() == "cancelled"
    }
}

/// Job result response.
#[derive(Debug, Clone, Deserialize)]
pub struct JobResultResponse {
    pub id: String,
    pub status: String,
    /// Histogram counts: bitstring → count.
    #[serde(default)]
    pub counts: Option<HashMap<String, u64>>,
    /// Error message if failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Execution metadata.
    #[serde(default)]
    pub metadata: Option<JobMetadata>,
}

/// Job execution metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct JobMetadata {
    #[serde(default)]
    pub execution_time_ms: Option<u64>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub num_gpus: Option<u32>,
    #[serde(default)]
    pub shots: Option<u32>,
    #[serde(default)]
    pub simulator_version: Option<String>,
}

/// Target (backend/simulator) information.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetInfo {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Maximum number of qubits.
    pub num_qubits: u32,
    /// Target status.
    pub status: String,
    /// Whether this is a simulator.
    #[serde(default)]
    pub is_simulator: bool,
    /// Supported program formats.
    #[serde(default)]
    pub supported_formats: Vec<ProgramFormat>,
    /// Native gate set names.
    #[serde(default)]
    pub native_gates: Vec<String>,
    /// Maximum shots.
    #[serde(default)]
    pub max_shots: Option<u32>,
}

impl TargetInfo {
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
    fn test_submit_request_serialization() {
        let request = SubmitRequest::new(
            "nvidia-mqpu",
            "OPENQASM 3.0; qubit[2] q;",
            ProgramFormat::Qasm3,
            1000,
        )
        .with_num_qubits(2);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("nvidia-mqpu"));
        assert!(json.contains("qasm3"));
        assert!(json.contains("1000"));
    }

    #[test]
    fn test_job_status_response() {
        let status = JobStatusResponse {
            id: "job-123".into(),
            status: "executing".into(),
            message: None,
            progress: Some(0.7),
        };

        assert!(status.is_pending());
        assert!(!status.is_completed());
        assert!(!status.is_failed());
    }

    #[test]
    fn test_program_format_serde() {
        let json = serde_json::to_string(&ProgramFormat::Qasm3).unwrap();
        assert_eq!(json, "\"qasm3\"");

        let parsed: ProgramFormat = serde_json::from_str("\"qir\"").unwrap();
        assert_eq!(parsed, ProgramFormat::Qir);
    }
}
