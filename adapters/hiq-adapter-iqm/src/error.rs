//! Error types for the IQM adapter.

use thiserror::Error;

/// Result type for IQM operations.
pub type IqmResult<T> = Result<T, IqmError>;

/// Errors that can occur when interacting with IQM.
#[derive(Debug, Error)]
pub enum IqmError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// Missing API token.
    #[error("Missing IQM API token")]
    MissingToken,

    /// Invalid endpoint URL.
    #[error("Invalid endpoint URL: {0}")]
    InvalidEndpoint(String),

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job execution failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Circuit validation error.
    #[error("Circuit validation error: {0}")]
    CircuitValidation(String),

    /// API error response.
    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    /// Timeout waiting for job.
    #[error("Timeout waiting for job: {0}")]
    Timeout(String),

    /// Unsupported operation.
    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    /// QASM generation error.
    #[error("QASM generation error: {0}")]
    QasmError(String),
}

impl From<IqmError> for hiq_hal::HalError {
    fn from(e: IqmError) -> Self {
        match e {
            IqmError::JobNotFound(id) => hiq_hal::HalError::JobNotFound(id),
            IqmError::JobFailed(msg) => hiq_hal::HalError::JobFailed(msg),
            IqmError::Timeout(id) => hiq_hal::HalError::Timeout(id),
            IqmError::CircuitValidation(msg) => hiq_hal::HalError::InvalidCircuit(msg),
            _ => hiq_hal::HalError::Backend(e.to_string()),
        }
    }
}
