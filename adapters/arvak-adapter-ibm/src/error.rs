//! Error types for IBM Quantum adapter.

use thiserror::Error;

/// Result type for IBM operations.
pub type IbmResult<T> = Result<T, IbmError>;

/// Errors that can occur when using IBM Quantum.
#[derive(Debug, Error)]
pub enum IbmError {
    /// Missing API token.
    #[error("IBM Quantum API token not found. Set IBM_QUANTUM_TOKEN environment variable.")]
    MissingToken,

    /// Invalid API token.
    #[error("Invalid IBM Quantum API token")]
    InvalidToken,

    /// HTTP request failed.
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// API returned an error.
    #[error("IBM Quantum API error: {message}")]
    ApiError {
        /// Error code from API.
        code: Option<String>,
        /// Error message.
        message: String,
    },

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Job was cancelled.
    #[error("Job was cancelled: {0}")]
    JobCancelled(String),

    /// Circuit conversion error.
    #[error("Circuit conversion error: {0}")]
    CircuitError(String),

    /// Backend not available.
    #[error("Backend not available: {0}")]
    BackendUnavailable(String),

    /// Timeout waiting for job.
    #[error("Timeout waiting for job")]
    Timeout,

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Circuit too large for backend.
    #[error("Circuit requires {required} qubits but backend only has {available}")]
    TooManyQubits {
        /// Qubits needed.
        required: usize,
        /// Qubits available.
        available: usize,
    },

    /// Invalid parameter.
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

impl From<IbmError> for hiq_hal::HalError {
    fn from(e: IbmError) -> Self {
        match e {
            IbmError::MissingToken | IbmError::InvalidToken => {
                hiq_hal::HalError::AuthenticationFailed(e.to_string())
            }
            IbmError::JobNotFound(id) => hiq_hal::HalError::JobNotFound(id),
            IbmError::JobFailed(msg) => hiq_hal::HalError::JobFailed(msg),
            IbmError::JobCancelled(_) => hiq_hal::HalError::JobCancelled,
            IbmError::BackendUnavailable(msg) => hiq_hal::HalError::BackendUnavailable(msg),
            IbmError::Timeout => hiq_hal::HalError::Timeout("IBM job".to_string()),
            IbmError::TooManyQubits {
                required,
                available,
            } => hiq_hal::HalError::CircuitTooLarge(format!(
                "Circuit requires {} qubits but backend only has {}",
                required, available
            )),
            _ => hiq_hal::HalError::Backend(e.to_string()),
        }
    }
}
