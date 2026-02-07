//! Error types for the CUDA-Q adapter.

use thiserror::Error;

pub type CudaqResult<T> = Result<T, CudaqError>;

#[derive(Debug, Error)]
pub enum CudaqError {
    #[error("Missing CUDA-Q API token (set CUDAQ_API_TOKEN)")]
    MissingToken,

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({code}): {message}")]
    Api { code: u16, message: String },

    #[error("QASM3 conversion failed: {0}")]
    QasmConversion(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Backend unavailable: {0}")]
    Unavailable(String),

    #[error("Deserialization failed: {0}")]
    Deserialize(String),
}

impl From<CudaqError> for arvak_hal::error::HalError {
    fn from(e: CudaqError) -> Self {
        match e {
            CudaqError::MissingToken => {
                arvak_hal::error::HalError::AuthenticationFailed(e.to_string())
            }
            CudaqError::JobNotFound(id) => arvak_hal::error::HalError::JobNotFound(id),
            CudaqError::JobFailed(msg) => arvak_hal::error::HalError::JobFailed(msg),
            CudaqError::Unavailable(msg) => arvak_hal::error::HalError::BackendUnavailable(msg),
            other => arvak_hal::error::HalError::Backend(other.to_string()),
        }
    }
}
