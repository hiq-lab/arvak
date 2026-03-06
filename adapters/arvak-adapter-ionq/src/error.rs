//! Error types for the IonQ adapter.

use thiserror::Error;

/// Result type for IonQ operations.
pub type IonQResult<T> = Result<T, IonQError>;

/// Errors that can occur when interacting with IonQ.
#[derive(Debug, Error)]
pub enum IonQError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Missing IonQ API key.
    #[error("Missing IonQ API key: set IONQ_API_KEY environment variable")]
    MissingApiKey,

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

    /// Circuit contains a gate not supported by IonQ QIS gateset.
    #[error("Unsupported gate: {0}")]
    UnsupportedGate(String),

    /// Circuit contains an unbound symbolic parameter.
    #[error("Symbolic (unbound) parameter in gate: {0}")]
    SymbolicParameter(String),

    /// Circuit exceeds IonQ resource limits.
    #[error("Circuit too large: {0}")]
    CircuitTooLarge(String),
}

impl From<IonQError> for arvak_hal::HalError {
    fn from(e: IonQError) -> Self {
        match e {
            IonQError::MissingApiKey => arvak_hal::HalError::AuthenticationFailed(e.to_string()),
            IonQError::ApiError { status: 401, .. } | IonQError::ApiError { status: 403, .. } => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            IonQError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            IonQError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            IonQError::Timeout(id) => arvak_hal::HalError::Timeout(id),
            IonQError::CircuitTooLarge(msg) => arvak_hal::HalError::CircuitTooLarge(msg),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_api_key_display() {
        let err = IonQError::MissingApiKey;
        assert!(err.to_string().contains("IONQ_API_KEY"));
    }

    #[test]
    fn test_api_error_display() {
        let err = IonQError::ApiError {
            status: 503,
            message: "Service unavailable".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("503"));
        assert!(msg.contains("Service unavailable"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = IonQError::JobNotFound("job-42".into());
        assert!(err.to_string().contains("job-42"));
    }

    #[test]
    fn test_unsupported_gate_display() {
        let err = IonQError::UnsupportedGate("custom_gate".into());
        assert!(err.to_string().contains("custom_gate"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_api_key_to_hal() {
        let hal: arvak_hal::HalError = IonQError::MissingApiKey.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = IonQError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = IonQError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = IonQError::Timeout("j42".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(id) if id == "j42"));
    }

    #[test]
    fn test_circuit_too_large_to_hal() {
        let hal: arvak_hal::HalError = IonQError::CircuitTooLarge("too many qubits".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::CircuitTooLarge(_)));
    }

    #[test]
    fn test_api_error_401_to_hal() {
        let hal: arvak_hal::HalError = IonQError::ApiError {
            status: 401,
            message: "unauthorized".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_api_error_500_to_hal() {
        let hal: arvak_hal::HalError = IonQError::ApiError {
            status: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
