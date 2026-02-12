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

#[cfg(test)]
mod tests {
    use super::*;

    // -- Display message tests --

    #[test]
    fn test_missing_token_display() {
        let err = CudaqError::MissingToken;
        assert!(err.to_string().contains("CUDAQ_API_TOKEN"));
    }

    #[test]
    fn test_api_error_display() {
        let err = CudaqError::Api {
            code: 429,
            message: "Rate limited".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("429"));
        assert!(msg.contains("Rate limited"));
    }

    #[test]
    fn test_qasm_conversion_display() {
        let err = CudaqError::QasmConversion("unsupported gate ccx".into());
        assert!(err.to_string().contains("unsupported gate ccx"));
    }

    #[test]
    fn test_job_not_found_display() {
        let err = CudaqError::JobNotFound("cudaq-job-1".into());
        assert!(err.to_string().contains("cudaq-job-1"));
    }

    #[test]
    fn test_job_failed_display() {
        let err = CudaqError::JobFailed("GPU error".into());
        assert!(err.to_string().contains("GPU error"));
    }

    #[test]
    fn test_unavailable_display() {
        let err = CudaqError::Unavailable("tensornet".into());
        assert!(err.to_string().contains("tensornet"));
    }

    #[test]
    fn test_deserialize_display() {
        let err = CudaqError::Deserialize("unexpected field".into());
        assert!(err.to_string().contains("unexpected field"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_token_to_hal_auth_failed() {
        let hal: arvak_hal::HalError = CudaqError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_job_not_found_to_hal() {
        let hal: arvak_hal::HalError = CudaqError::JobNotFound("j1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "j1"));
    }

    #[test]
    fn test_job_failed_to_hal() {
        let hal: arvak_hal::HalError = CudaqError::JobFailed("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "err"));
    }

    #[test]
    fn test_unavailable_to_hal() {
        let hal: arvak_hal::HalError =
            CudaqError::Unavailable("target_x".into()).into();
        assert!(
            matches!(hal, arvak_hal::HalError::BackendUnavailable(msg) if msg == "target_x")
        );
    }

    #[test]
    fn test_api_error_to_hal_backend() {
        let hal: arvak_hal::HalError = CudaqError::Api {
            code: 500,
            message: "internal".into(),
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_qasm_conversion_to_hal_backend() {
        let hal: arvak_hal::HalError =
            CudaqError::QasmConversion("bad".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }

    #[test]
    fn test_deserialize_to_hal_backend() {
        let hal: arvak_hal::HalError =
            CudaqError::Deserialize("bad json".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
