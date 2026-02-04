//! Backend trait and configuration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use hiq_ir::Circuit;

use crate::capability::Capabilities;
use crate::error::HalResult;
use crate::job::{JobId, JobStatus};
use crate::result::ExecutionResult;

/// Configuration for a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    /// Name of the backend.
    pub name: String,
    /// API endpoint URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Authentication token.
    #[serde(skip_serializing)]
    pub token: Option<String>,
    /// Additional configuration.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl BackendConfig {
    /// Create a new backend configuration.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            endpoint: None,
            token: None,
            extra: serde_json::Map::new(),
        }
    }

    /// Set the endpoint URL.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the authentication token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Add extra configuration.
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }
}

/// Trait for quantum backends.
///
/// This trait defines the interface that all quantum backends must implement.
/// It supports job submission, status checking, result retrieval, and cancellation.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Get the name of this backend.
    fn name(&self) -> &str;

    /// Get the capabilities of this backend.
    async fn capabilities(&self) -> HalResult<Capabilities>;

    /// Check if the backend is available.
    async fn is_available(&self) -> HalResult<bool>;

    /// Submit a circuit for execution.
    ///
    /// Returns a job ID that can be used to check status and retrieve results.
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId>;

    /// Get the status of a job.
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus>;

    /// Get the result of a completed job.
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult>;

    /// Cancel a running job.
    async fn cancel(&self, job_id: &JobId) -> HalResult<()>;

    /// Wait for a job to complete and return its result.
    ///
    /// This is a convenience method that polls the job status
    /// until it reaches a terminal state.
    async fn wait(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        use crate::error::HalError;
        use std::time::Duration;
        use tokio::time::sleep;

        let poll_interval = Duration::from_millis(500);
        let max_polls = 600; // 5 minutes max

        for _ in 0..max_polls {
            let status = self.status(job_id).await?;

            match status {
                JobStatus::Completed => return self.result(job_id).await,
                JobStatus::Failed(msg) => return Err(HalError::JobFailed(msg)),
                JobStatus::Cancelled => return Err(HalError::JobCancelled),
                JobStatus::Queued | JobStatus::Running => {
                    sleep(poll_interval).await;
                }
            }
        }

        Err(HalError::Timeout(job_id.0.clone()))
    }
}

/// Trait for creating backends from configuration.
pub trait BackendFactory: Backend + Sized {
    /// Create a backend from configuration.
    fn from_config(config: BackendConfig) -> HalResult<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config() {
        let config = BackendConfig::new("test")
            .with_endpoint("https://api.example.com")
            .with_token("secret-token")
            .with_extra("timeout", serde_json::json!(30));

        assert_eq!(config.name, "test");
        assert_eq!(config.endpoint, Some("https://api.example.com".to_string()));
        assert_eq!(config.token, Some("secret-token".to_string()));
        assert!(config.extra.contains_key("timeout"));
    }
}
