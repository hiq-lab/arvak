//! Error types for the Quandela adapter.

use thiserror::Error;

/// Result type for Quandela operations.
pub type QuandelaResult<T> = Result<T, QuandelaError>;

/// Errors that can occur when interacting with Quandela / Perceval cloud.
#[derive(Debug, Error)]
pub enum QuandelaError {
    /// Missing cloud token (`PCVL_CLOUD_TOKEN` not set).
    #[error("Missing Quandela cloud token: set PCVL_CLOUD_TOKEN environment variable")]
    MissingToken,

    /// Perceval bridge subprocess failed or returned an error.
    #[error("Perceval bridge error: {0}")]
    BridgeError(String),

    /// HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// API returned an error response.
    #[error("API error: {0}")]
    ApiError(String),

    /// Circuit contains a gate not in the Quandela / perceval-interop gate set.
    #[error("Unsupported gate: {0}")]
    UnsupportedGate(String),

    /// Circuit serialization error.
    #[error("Circuit serialization error: {0}")]
    Serialization(String),
}

impl From<QuandelaError> for arvak_hal::HalError {
    fn from(e: QuandelaError) -> Self {
        match e {
            QuandelaError::MissingToken => arvak_hal::HalError::AuthenticationFailed(e.to_string()),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

impl From<serde_json::Error> for QuandelaError {
    fn from(e: serde_json::Error) -> Self {
        QuandelaError::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_token_display() {
        let err = QuandelaError::MissingToken;
        assert!(err.to_string().contains("PCVL_CLOUD_TOKEN"));
    }

    #[test]
    fn test_bridge_error_display() {
        let err = QuandelaError::BridgeError("script not found".into());
        assert!(err.to_string().contains("script not found"));
    }

    #[test]
    fn test_api_error_display() {
        let err = QuandelaError::ApiError("service unavailable".into());
        assert!(err.to_string().contains("service unavailable"));
    }

    #[test]
    fn test_unsupported_gate_display() {
        let err = QuandelaError::UnsupportedGate("rxx".into());
        assert!(err.to_string().contains("rxx"));
    }

    #[test]
    fn test_missing_token_to_hal() {
        let hal: arvak_hal::HalError = QuandelaError::MissingToken.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_bridge_error_to_hal() {
        let hal: arvak_hal::HalError = QuandelaError::BridgeError("err".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
