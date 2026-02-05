//! Error types for the QDMI adapter.

use crate::ffi::QdmiStatus;
use thiserror::Error;

/// Errors from QDMI adapter operations.
#[derive(Debug, Error)]
pub enum QdmiError {
    /// QDMI library returned an error status
    #[error("QDMI error: {0:?}")]
    Status(QdmiStatus),

    /// Session not initialized
    #[error("QDMI session not initialized")]
    SessionNotInitialized,

    /// No device available
    #[error("No QDMI device available")]
    NoDevice,

    /// Device not ready
    #[error("QDMI device not ready: {0}")]
    DeviceNotReady(String),

    /// Invalid parameter
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// Job not found
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job failed
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Timeout waiting for job
    #[error("Timeout waiting for job: {0}")]
    Timeout(String),

    /// Program format not supported
    #[error("Program format not supported: {0}")]
    UnsupportedFormat(String),

    /// Circuit conversion error
    #[error("Circuit conversion error: {0}")]
    CircuitConversion(String),

    /// FFI error
    #[error("FFI error: {0}")]
    Ffi(String),

    /// QDMI library not available
    #[error("QDMI library not available (system-qdmi feature not enabled)")]
    LibraryNotAvailable,
}

impl From<QdmiStatus> for QdmiError {
    fn from(status: QdmiStatus) -> Self {
        QdmiError::Status(status)
    }
}

impl From<QdmiError> for hiq_hal::error::HalError {
    fn from(err: QdmiError) -> Self {
        match err {
            QdmiError::JobFailed(msg) => hiq_hal::error::HalError::JobFailed(msg),
            QdmiError::Timeout(id) => hiq_hal::error::HalError::Timeout(id),
            QdmiError::JobNotFound(id) => hiq_hal::error::HalError::JobNotFound(id),
            QdmiError::NoDevice => {
                hiq_hal::error::HalError::BackendUnavailable("No QDMI device".into())
            }
            other => hiq_hal::error::HalError::Backend(other.to_string()),
        }
    }
}

/// Result type for QDMI operations.
pub type QdmiResult<T> = Result<T, QdmiError>;
