//! Error types for the HAL crate.

use thiserror::Error;

/// Errors that can occur in HAL operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HalError {
    /// Backend is not available.
    #[error("Backend not available: {0}")]
    BackendUnavailable(String),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Authentication error (OIDC, token, etc.).
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Job submission failed.
    #[error("Job submission failed: {0}")]
    SubmissionFailed(String),

    /// Job execution failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Job was cancelled.
    #[error("Job cancelled")]
    JobCancelled,

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Invalid circuit.
    #[error("Invalid circuit: {0}")]
    InvalidCircuit(String),

    /// Network error.
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Timeout waiting for job.
    #[error("Timeout waiting for job {0}")]
    Timeout(String),

    /// Circuit exceeds backend capabilities.
    #[error("Circuit exceeds backend capabilities: {0}")]
    CircuitTooLarge(String),

    /// Unsupported feature.
    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    /// Invalid number of shots.
    #[error("Invalid shots: {0}")]
    InvalidShots(String),

    /// Generic backend error.
    #[error("Backend error: {0}")]
    Backend(String),
}

/// Result type for HAL operations.
pub type HalResult<T> = Result<T, HalError>;
