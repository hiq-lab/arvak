//! DDSIM adapter error types.

use arvak_hal::HalError;
use thiserror::Error;

/// Result type for DDSIM operations.
pub type DdsimResult<T> = Result<T, DdsimError>;

/// Errors specific to the DDSIM adapter.
#[derive(Debug, Error)]
pub enum DdsimError {
    /// Python or mqt-ddsim is not installed / not found on PATH.
    #[error("DDSIM not available: {0}")]
    NotAvailable(String),

    /// The DDSIM subprocess returned a non-zero exit code.
    #[error("DDSIM execution failed (exit code {code:?}): {stderr}")]
    ExecutionFailed { code: Option<i32>, stderr: String },

    /// Failed to parse the JSON output from DDSIM.
    #[error("failed to parse DDSIM output: {0}")]
    OutputParse(String),

    /// QASM serialization failed.
    #[error("QASM serialization failed: {0}")]
    QasmEmit(String),

    /// I/O error writing temp files or spawning process.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<DdsimError> for HalError {
    fn from(e: DdsimError) -> Self {
        match e {
            DdsimError::NotAvailable(msg) => HalError::BackendUnavailable(msg),
            DdsimError::ExecutionFailed { stderr, .. } => {
                HalError::Backend(format!("DDSIM: {stderr}"))
            }
            DdsimError::OutputParse(msg) => HalError::Backend(format!("DDSIM output: {msg}")),
            DdsimError::QasmEmit(msg) => HalError::InvalidCircuit(msg),
            DdsimError::Io(e) => HalError::Backend(format!("DDSIM I/O: {e}")),
        }
    }
}
