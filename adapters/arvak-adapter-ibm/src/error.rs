//! Error types for IBM Quantum adapter.

use thiserror::Error;

/// Result type for IBM operations.
pub type IbmResult<T> = Result<T, IbmError>;

/// Errors that can occur when using IBM Quantum.
#[derive(Debug, Error)]
pub enum IbmError {
    /// Missing API token.
    #[error(
        "IBM Quantum API token not found. Set IBM_API_KEY or IBM_QUANTUM_TOKEN environment variable."
    )]
    MissingToken,

    /// Invalid API token.
    #[error("Invalid IBM Quantum API token")]
    InvalidToken,

    /// IAM token exchange failed.
    #[error("IAM token exchange failed: {0}")]
    IamTokenExchange(String),

    /// Missing service CRN.
    #[error("IBM_SERVICE_CRN environment variable is required when using IBM_API_KEY")]
    MissingServiceCrn,

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

impl From<IbmError> for arvak_hal::HalError {
    fn from(e: IbmError) -> Self {
        match e {
            IbmError::MissingToken
            | IbmError::InvalidToken
            | IbmError::IamTokenExchange(_)
            | IbmError::MissingServiceCrn => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            IbmError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            IbmError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            IbmError::JobCancelled(_) => arvak_hal::HalError::JobCancelled,
            IbmError::BackendUnavailable(msg) => arvak_hal::HalError::BackendUnavailable(msg),
            IbmError::Timeout => arvak_hal::HalError::Timeout("IBM job".to_string()),
            IbmError::TooManyQubits {
                required,
                available,
            } => arvak_hal::HalError::CircuitTooLarge(format!(
                "Circuit requires {required} qubits but backend only has {available}"
            )),
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
        let err = IbmError::MissingToken;
        assert!(err.to_string().contains("IBM_API_KEY"));
    }

    #[test]
    fn test_invalid_token_display() {
        let err = IbmError::InvalidToken;
        assert!(err.to_string().contains("Invalid"));
    }

    #[test]
    fn test_api_error_display() {
        let err = IbmError::ApiError {
            code: Some("ERR_401".into()),
            message: "Unauthorized".into(),
        };
        assert!(err.to_string().contains("Unauthorized"));
    }

    #[test]
    fn test_api_error_no_code_display() {
        let err = IbmError::ApiError {
            code: None,
            message: "Something went wrong".into(),
        };
        assert!(err.to_string().contains("Something went wrong"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = IbmError::JobNotFound("abc123".into());
        assert!(err.to_string().contains("abc123"));
    }

    #[test]
    fn test_job_failed_display() {
        let err = IbmError::JobFailed("circuit too deep".into());
        assert!(err.to_string().contains("circuit too deep"));
    }

    #[test]
    fn test_job_cancelled_display() {
        let err = IbmError::JobCancelled("user request".into());
        assert!(err.to_string().contains("user request"));
    }

    #[test]
    fn test_circuit_error_display() {
        let err = IbmError::CircuitError("invalid gate".into());
        assert!(err.to_string().contains("invalid gate"));
    }

    #[test]
    fn test_backend_unavailable_display() {
        let err = IbmError::BackendUnavailable("ibm_brisbane".into());
        assert!(err.to_string().contains("ibm_brisbane"));
    }

    #[test]
    fn test_timeout_display() {
        let err = IbmError::Timeout;
        assert!(err.to_string().contains("Timeout"));
    }

    #[test]
    fn test_too_many_qubits_display() {
        let err = IbmError::TooManyQubits {
            required: 50,
            available: 27,
        };
        let msg = err.to_string();
        assert!(msg.contains("50"));
        assert!(msg.contains("27"));
    }

    #[test]
    fn test_invalid_parameter_display() {
        let err = IbmError::InvalidParameter("shots must be positive".into());
        assert!(err.to_string().contains("shots must be positive"));
    }

    #[test]
    fn test_iam_token_exchange_display() {
        let err = IbmError::IamTokenExchange("401 Unauthorized".into());
        assert!(err.to_string().contains("401 Unauthorized"));
    }

    #[test]
    fn test_missing_service_crn_display() {
        let err = IbmError::MissingServiceCrn;
        assert!(err.to_string().contains("IBM_SERVICE_CRN"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_token_to_hal_auth_failed() {
        let hal: arvak_hal::HalError = IbmError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_invalid_token_to_hal_auth_failed() {
        let hal: arvak_hal::HalError = IbmError::InvalidToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = IbmError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = IbmError::JobFailed("boom".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "boom"));
    }

    #[test]
    fn test_job_cancelled_to_hal() {
        let hal: arvak_hal::HalError = IbmError::JobCancelled("user".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobCancelled));
    }

    #[test]
    fn test_backend_unavailable_to_hal() {
        let hal: arvak_hal::HalError = IbmError::BackendUnavailable("ibm_kyoto".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::BackendUnavailable(msg) if msg == "ibm_kyoto"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = IbmError::Timeout.into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(_)));
    }

    #[test]
    fn test_too_many_qubits_to_hal() {
        let hal: arvak_hal::HalError = IbmError::TooManyQubits {
            required: 50,
            available: 27,
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::CircuitTooLarge(_)));
    }

    #[test]
    fn test_circuit_error_to_hal_backend() {
        let hal: arvak_hal::HalError = IbmError::CircuitError("bad".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_api_error_to_hal_backend() {
        let hal: arvak_hal::HalError = IbmError::ApiError {
            code: None,
            message: "server error".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_invalid_parameter_to_hal_backend() {
        let hal: arvak_hal::HalError = IbmError::InvalidParameter("bad param".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_iam_token_exchange_to_hal_auth_failed() {
        let hal: arvak_hal::HalError = IbmError::IamTokenExchange("fail".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_missing_service_crn_to_hal_auth_failed() {
        let hal: arvak_hal::HalError = IbmError::MissingServiceCrn.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }
}
