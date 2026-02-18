//! Error types for the Scaleway QaaS adapter.

use thiserror::Error;

/// Result type for Scaleway operations.
pub type ScalewayResult<T> = Result<T, ScalewayError>;

/// Errors that can occur when interacting with Scaleway QaaS.
#[derive(Debug, Error)]
pub enum ScalewayError {
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
    #[error("Missing Scaleway secret key (set SCALEWAY_SECRET_KEY)")]
    MissingToken,

    /// Missing project ID.
    #[error("Missing Scaleway project ID (set SCALEWAY_PROJECT_ID)")]
    MissingProjectId,

    /// Missing session ID.
    #[error("No active session — create one via Scaleway console or API")]
    MissingSession,

    /// Job not found.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Job execution failed.
    #[error("Job failed: {0}")]
    JobFailed(String),

    /// Circuit validation error.
    #[error("Circuit validation error: {0}")]
    CircuitValidation(String),

    /// Session not ready.
    #[error("Session not ready (status: {0})")]
    SessionNotReady(String),

    /// Quota exceeded.
    #[error("Scaleway quota exceeded: {0}")]
    QuotaExceeded(String),

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

impl From<ScalewayError> for arvak_hal::HalError {
    fn from(e: ScalewayError) -> Self {
        match e {
            ScalewayError::MissingToken | ScalewayError::MissingProjectId => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            ScalewayError::AuthFailed(ref _msg) => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            ScalewayError::JobNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            ScalewayError::JobFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            ScalewayError::Timeout(id) => arvak_hal::HalError::Timeout(id),
            ScalewayError::CircuitValidation(msg) => arvak_hal::HalError::InvalidCircuit(msg),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_token_display() {
        let err = ScalewayError::MissingToken;
        assert!(err.to_string().contains("SCALEWAY_SECRET_KEY"));
    }

    #[test]
    fn test_missing_project_display() {
        let err = ScalewayError::MissingProjectId;
        assert!(err.to_string().contains("SCALEWAY_PROJECT_ID"));
    }

    #[test]
    fn test_auth_failed_display() {
        let err = ScalewayError::AuthFailed("token expired".into());
        assert!(err.to_string().contains("token expired"));
    }

    #[test]
    fn test_session_not_ready_display() {
        let err = ScalewayError::SessionNotReady("starting".into());
        assert!(err.to_string().contains("starting"));
    }

    #[test]
    fn test_quota_exceeded_display() {
        let err = ScalewayError::QuotaExceeded("3 sessions max".into());
        assert!(err.to_string().contains("3 sessions max"));
    }

    #[test]
    fn test_api_error_display() {
        let err = ScalewayError::ApiError {
            status: 403,
            message: "quota exceeded".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("403"));
        assert!(msg.contains("quota exceeded"));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = ScalewayError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = ScalewayError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = ScalewayError::Timeout("j42".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(id) if id == "j42"));
    }

    #[test]
    fn test_missing_token_to_hal_auth() {
        let hal: arvak_hal::HalError = ScalewayError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_api_error_to_hal_backend() {
        let hal: arvak_hal::HalError = ScalewayError::ApiError {
            status: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_qasm_error_to_hal_backend() {
        let hal: arvak_hal::HalError = ScalewayError::QasmError("fail".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
