//! Error types for the Quandela adapter.

use thiserror::Error;

/// Result type for Quandela operations.
pub type QuandelaResult<T> = Result<T, QuandelaError>;

/// Errors that can occur when interacting with Quandela Altair.
#[derive(Debug, Error)]
pub enum QuandelaError {
    /// Missing Quandela API key (`QUANDELA_API_KEY` not set).
    #[error("Missing Quandela API key: set QUANDELA_API_KEY environment variable")]
    MissingApiKey,

    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned an error response.
    #[error("API error: {0}")]
    ApiError(String),

    /// Circuit contains a gate not in the Quandela / perceval-interop gate set.
    #[error("Unsupported gate: {0}")]
    QasmError(String),
}

impl From<QuandelaError> for arvak_hal::HalError {
    fn from(e: QuandelaError) -> Self {
        match e {
            QuandelaError::MissingApiKey => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_api_key_display() {
        let err = QuandelaError::MissingApiKey;
        assert!(err.to_string().contains("QUANDELA_API_KEY"));
    }

    #[test]
    fn test_api_error_display() {
        let err = QuandelaError::ApiError("service unavailable".into());
        assert!(err.to_string().contains("service unavailable"));
    }

    #[test]
    fn test_qasm_error_display() {
        let err = QuandelaError::QasmError("rxx".into());
        assert!(err.to_string().contains("rxx"));
    }

    #[test]
    fn test_missing_api_key_to_hal() {
        let hal: arvak_hal::HalError = QuandelaError::MissingApiKey.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_api_error_to_hal() {
        let hal: arvak_hal::HalError = QuandelaError::ApiError("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
