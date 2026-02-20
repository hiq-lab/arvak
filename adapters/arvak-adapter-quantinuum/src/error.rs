//! Error types for the Quantinuum adapter.

use thiserror::Error;

/// Result type for Quantinuum operations.
pub type QuantinuumResult<T> = Result<T, QuantinuumError>;

/// Errors that can occur when interacting with Quantinuum.
#[derive(Debug, Error)]
pub enum QuantinuumError {
    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Missing email credential.
    #[error("Missing Quantinuum email: set QUANTINUUM_EMAIL environment variable")]
    MissingEmail,

    /// Missing password credential.
    #[error("Missing Quantinuum password: set QUANTINUUM_PASSWORD environment variable")]
    MissingPassword,

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// JWT token expired and re-authentication failed.
    #[error("Token expired")]
    TokenExpired,

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job execution failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// API error response.
    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    /// Timeout waiting for job.
    #[error("Timeout waiting for job: {0}")]
    Timeout(String),

    /// QASM generation error.
    #[error("QASM generation error: {0}")]
    QasmError(String),
}

impl From<QuantinuumError> for arvak_hal::HalError {
    fn from(e: QuantinuumError) -> Self {
        match e {
            QuantinuumError::MissingEmail | QuantinuumError::MissingPassword => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            QuantinuumError::AuthFailed(_) => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            QuantinuumError::TokenExpired => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            QuantinuumError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            QuantinuumError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            QuantinuumError::Timeout(id) => arvak_hal::HalError::Timeout(id),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_email_display() {
        let err = QuantinuumError::MissingEmail;
        assert!(err.to_string().contains("QUANTINUUM_EMAIL"));
    }

    #[test]
    fn test_missing_password_display() {
        let err = QuantinuumError::MissingPassword;
        assert!(err.to_string().contains("QUANTINUUM_PASSWORD"));
    }

    #[test]
    fn test_auth_failed_display() {
        let err = QuantinuumError::AuthFailed("invalid credentials".into());
        assert!(err.to_string().contains("invalid credentials"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = QuantinuumError::JobNotFound("job-42".into());
        assert!(err.to_string().contains("job-42"));
    }

    #[test]
    fn test_job_failed_display() {
        let err = QuantinuumError::JobFailed("circuit too deep".into());
        assert!(err.to_string().contains("circuit too deep"));
    }

    #[test]
    fn test_api_error_display() {
        let err = QuantinuumError::ApiError {
            status: 503,
            message: "Service unavailable".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("503"));
        assert!(msg.contains("Service unavailable"));
    }

    #[test]
    fn test_timeout_display() {
        let err = QuantinuumError::Timeout("job-99".into());
        assert!(err.to_string().contains("job-99"));
    }

    #[test]
    fn test_qasm_error_display() {
        let err = QuantinuumError::QasmError("emit failed".into());
        assert!(err.to_string().contains("emit failed"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_email_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::MissingEmail.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_auth_failed_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::AuthFailed("bad".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_token_expired_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::TokenExpired.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::Timeout("j42".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(id) if id == "j42"));
    }

    #[test]
    fn test_api_error_to_hal() {
        let hal: arvak_hal::HalError = QuantinuumError::ApiError {
            status: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
