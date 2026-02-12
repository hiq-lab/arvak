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

impl From<IqmError> for arvak_hal::HalError {
    fn from(e: IqmError) -> Self {
        match e {
            IqmError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            IqmError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            IqmError::Timeout(id) => arvak_hal::HalError::Timeout(id),
            IqmError::CircuitValidation(msg) => arvak_hal::HalError::InvalidCircuit(msg),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Display message tests --

    #[test]
    fn test_missing_token_display() {
        let err = IqmError::MissingToken;
        assert!(err.to_string().contains("IQM API token"));
    }

    #[test]
    fn test_auth_failed_display() {
        let err = IqmError::AuthFailed("token expired".into());
        assert!(err.to_string().contains("token expired"));
    }

    #[test]
    fn test_invalid_endpoint_display() {
        let err = IqmError::InvalidEndpoint("not-a-url".into());
        assert!(err.to_string().contains("not-a-url"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = IqmError::JobNotFound("job-42".into());
        assert!(err.to_string().contains("job-42"));
    }

    #[test]
    fn test_job_failed_display() {
        let err = IqmError::JobFailed("calibration drift".into());
        assert!(err.to_string().contains("calibration drift"));
    }

    #[test]
    fn test_circuit_validation_display() {
        let err = IqmError::CircuitValidation("unsupported gate".into());
        assert!(err.to_string().contains("unsupported gate"));
    }

    #[test]
    fn test_api_error_display() {
        let err = IqmError::ApiError {
            status: 503,
            message: "Service unavailable".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("503"));
        assert!(msg.contains("Service unavailable"));
    }

    #[test]
    fn test_timeout_display() {
        let err = IqmError::Timeout("job-99".into());
        assert!(err.to_string().contains("job-99"));
    }

    #[test]
    fn test_unsupported_display() {
        let err = IqmError::Unsupported("mid-circuit measurement".into());
        assert!(err.to_string().contains("mid-circuit measurement"));
    }

    #[test]
    fn test_qasm_error_display() {
        let err = IqmError::QasmError("emit failed".into());
        assert!(err.to_string().contains("emit failed"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = IqmError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = IqmError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = IqmError::Timeout("j42".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(id) if id == "j42"));
    }

    #[test]
    fn test_circuit_validation_to_hal() {
        let hal: arvak_hal::HalError =
            IqmError::CircuitValidation("bad gate".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::InvalidCircuit(msg) if msg == "bad gate"));
    }

    #[test]
    fn test_missing_token_to_hal_backend() {
        let hal: arvak_hal::HalError = IqmError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_auth_failed_to_hal_backend() {
        let hal: arvak_hal::HalError = IqmError::AuthFailed("bad".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_api_error_to_hal_backend() {
        let hal: arvak_hal::HalError = IqmError::ApiError {
            status: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_invalid_endpoint_to_hal_backend() {
        let hal: arvak_hal::HalError =
            IqmError::InvalidEndpoint("bad".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_unsupported_to_hal_backend() {
        let hal: arvak_hal::HalError =
            IqmError::Unsupported("op".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_qasm_error_to_hal_backend() {
        let hal: arvak_hal::HalError = IqmError::QasmError("fail".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
