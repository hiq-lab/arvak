//! Error types for the AQT adapter.

use thiserror::Error;

/// Result type for AQT operations.
pub type AqtResult<T> = Result<T, AqtError>;

/// Errors that can occur when interacting with AQT.
#[derive(Debug, Error)]
pub enum AqtError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Missing AQT API token.
    #[error("Missing AQT token: set AQT_TOKEN environment variable")]
    MissingToken,

    /// API returned an error response.
    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job execution failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Timeout waiting for job completion.
    #[error("Timeout waiting for job: {0}")]
    Timeout(String),

    /// Circuit contains a gate not supported by AQT.
    #[error("Unsupported gate: {0}")]
    UnsupportedGate(String),

    /// Circuit contains an unbound symbolic parameter.
    #[error("Symbolic (unbound) parameter in gate: {0}")]
    SymbolicParameter(String),

    /// Circuit exceeds AQT resource limits.
    #[error("Circuit too large: {0}")]
    CircuitTooLarge(String),
}

impl From<AqtError> for arvak_hal::HalError {
    fn from(e: AqtError) -> Self {
        match e {
            AqtError::MissingToken => arvak_hal::HalError::AuthenticationFailed(e.to_string()),
            AqtError::ApiError { status: 401, .. } => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            AqtError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            AqtError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            AqtError::Timeout(id) => arvak_hal::HalError::Timeout(id),
            AqtError::CircuitTooLarge(msg) => arvak_hal::HalError::CircuitTooLarge(msg),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_token_display() {
        let err = AqtError::MissingToken;
        assert!(err.to_string().contains("AQT_TOKEN"));
    }

    #[test]
    fn test_api_error_display() {
        let err = AqtError::ApiError {
            status: 503,
            message: "Service unavailable".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("503"));
        assert!(msg.contains("Service unavailable"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = AqtError::JobNotFound("job-42".into());
        assert!(err.to_string().contains("job-42"));
    }

    #[test]
    fn test_job_failed_display() {
        let err = AqtError::JobFailed("circuit invalid".into());
        assert!(err.to_string().contains("circuit invalid"));
    }

    #[test]
    fn test_timeout_display() {
        let err = AqtError::Timeout("job-99".into());
        assert!(err.to_string().contains("job-99"));
    }

    #[test]
    fn test_unsupported_gate_display() {
        let err = AqtError::UnsupportedGate("ccx".into());
        assert!(err.to_string().contains("ccx"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_token_to_hal() {
        let hal: arvak_hal::HalError = AqtError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = AqtError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = AqtError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = AqtError::Timeout("j42".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(id) if id == "j42"));
    }

    #[test]
    fn test_circuit_too_large_to_hal() {
        let hal: arvak_hal::HalError = AqtError::CircuitTooLarge("20 qubits max".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::CircuitTooLarge(_)));
    }

    #[test]
    fn test_api_error_to_hal() {
        let hal: arvak_hal::HalError = AqtError::ApiError {
            status: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
