//! Error types for the gRPC service.

use thiserror::Error;
use tonic::Status;

/// Result type for gRPC service operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the gRPC service.
#[derive(Debug, Error)]
pub enum Error {
    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Backend not found.
    #[error("Backend not found: {0}")]
    BackendNotFound(String),

    /// Invalid circuit format.
    #[error("Invalid circuit: {0}")]
    InvalidCircuit(String),

    /// Job is not in a terminal state.
    #[error("Job is not completed: {0}")]
    JobNotCompleted(String),

    /// Job execution failed.
    #[error("Job execution failed: {0}")]
    JobFailed(String),

    /// Backend error.
    #[error("Backend error: {0}")]
    Backend(#[from] arvak_hal::error::HalError),

    /// QASM parsing error.
    #[error("QASM parsing error: {0}")]
    QasmParse(String),

    /// JSON parsing error.
    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Storage error.
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        match err {
            Error::JobNotFound(msg) => Status::not_found(msg),
            Error::BackendNotFound(msg) => Status::not_found(msg),
            Error::InvalidCircuit(msg) => Status::invalid_argument(msg),
            Error::JobNotCompleted(msg) => Status::failed_precondition(msg),
            Error::JobFailed(msg) => Status::aborted(msg),
            Error::Backend(e) => Status::internal(format!("Backend error: {}", e)),
            Error::QasmParse(msg) => Status::invalid_argument(format!("QASM parse error: {}", msg)),
            Error::JsonParse(e) => Status::invalid_argument(format!("JSON parse error: {}", e)),
            Error::StorageError(msg) => Status::internal(format!("Storage error: {}", msg)),
            Error::Internal(msg) => Status::internal(msg),
        }
    }
}

impl From<arvak_qasm3::ParseError> for Error {
    fn from(err: arvak_qasm3::ParseError) -> Self {
        Error::QasmParse(err.to_string())
    }
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::StorageError(err.to_string())
    }
}
