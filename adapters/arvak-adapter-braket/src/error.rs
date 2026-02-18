//! Error types for AWS Braket adapter.

use thiserror::Error;

/// Result type for Braket operations.
pub type BraketResult<T> = Result<T, BraketError>;

/// Errors that can occur when using AWS Braket.
#[derive(Debug, Error)]
pub enum BraketError {
    /// Missing AWS credentials.
    #[error("AWS credentials not found. Configure via environment, SSO, or IAM role.")]
    MissingCredentials,

    /// Missing S3 bucket configuration.
    #[error("S3 bucket not configured. Set ARVAK_BRAKET_S3_BUCKET environment variable.")]
    MissingS3Bucket,

    /// Invalid device ARN.
    #[error("Invalid device ARN: {0}")]
    InvalidDeviceArn(String),

    /// Braket API error.
    #[error("Braket API error: {0}")]
    BraketApi(String),

    /// S3 error.
    #[error("S3 error: {0}")]
    S3Error(String),

    /// Task not found.
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Task failed.
    #[error("Task failed: {0}")]
    TaskFailed(String),

    /// Task was cancelled.
    #[error("Task was cancelled: {0}")]
    TaskCancelled(String),

    /// Circuit conversion error.
    #[error("Circuit conversion error: {0}")]
    CircuitError(String),

    /// Device unavailable.
    #[error("Device not available: {0}")]
    DeviceUnavailable(String),

    /// Timeout waiting for task.
    #[error("Timeout waiting for task")]
    Timeout,

    /// JSON parsing error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Circuit too large for device.
    #[error("Circuit requires {required} qubits but device only has {available}")]
    TooManyQubits {
        /// Qubits needed.
        required: usize,
        /// Qubits available.
        available: usize,
    },

    /// Invalid parameter.
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// Result parsing error.
    #[error("Failed to parse task result: {0}")]
    ResultParseError(String),
}

impl From<BraketError> for arvak_hal::HalError {
    fn from(e: BraketError) -> Self {
        match e {
            BraketError::MissingCredentials | BraketError::MissingS3Bucket => {
                arvak_hal::HalError::AuthenticationFailed(e.to_string())
            }
            BraketError::TaskNotFound(id) => arvak_hal::HalError::JobNotFound(id),
            BraketError::TaskFailed(msg) => arvak_hal::HalError::JobFailed(msg),
            BraketError::TaskCancelled(_) => arvak_hal::HalError::JobCancelled,
            BraketError::DeviceUnavailable(msg) => arvak_hal::HalError::BackendUnavailable(msg),
            BraketError::Timeout => arvak_hal::HalError::Timeout("Braket task".to_string()),
            BraketError::TooManyQubits {
                required,
                available,
            } => arvak_hal::HalError::CircuitTooLarge(format!(
                "Circuit requires {required} qubits but device only has {available}"
            )),
            BraketError::CircuitError(msg) => arvak_hal::HalError::InvalidCircuit(msg),
            BraketError::InvalidDeviceArn(msg) => arvak_hal::HalError::Configuration(msg),
            _ => arvak_hal::HalError::Backend(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_credentials_display() {
        let err = BraketError::MissingCredentials;
        assert!(err.to_string().contains("AWS credentials"));
    }

    #[test]
    fn test_missing_s3_bucket_display() {
        let err = BraketError::MissingS3Bucket;
        assert!(err.to_string().contains("ARVAK_BRAKET_S3_BUCKET"));
    }

    #[test]
    fn test_invalid_device_arn_display() {
        let err = BraketError::InvalidDeviceArn("bad-arn".into());
        assert!(err.to_string().contains("bad-arn"));
    }

    #[test]
    fn test_task_not_found_display() {
        let err = BraketError::TaskNotFound("arn:aws:braket:us-east-1:123:task/abc".into());
        assert!(err.to_string().contains("abc"));
    }

    #[test]
    fn test_too_many_qubits_display() {
        let err = BraketError::TooManyQubits {
            required: 50,
            available: 25,
        };
        let msg = err.to_string();
        assert!(msg.contains("50"));
        assert!(msg.contains("25"));
    }

    // -- HalError conversion tests --

    #[test]
    fn test_missing_credentials_to_hal_auth() {
        let hal: arvak_hal::HalError = BraketError::MissingCredentials.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_missing_s3_to_hal_auth() {
        let hal: arvak_hal::HalError = BraketError::MissingS3Bucket.into();
        assert!(matches!(hal, arvak_hal::HalError::AuthenticationFailed(_)));
    }

    #[test]
    fn test_task_not_found_to_hal() {
        let hal: arvak_hal::HalError = BraketError::TaskNotFound("t1".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobNotFound(id) if id == "t1"));
    }

    #[test]
    fn test_task_failed_to_hal() {
        let hal: arvak_hal::HalError = BraketError::TaskFailed("boom".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobFailed(msg) if msg == "boom"));
    }

    #[test]
    fn test_task_cancelled_to_hal() {
        let hal: arvak_hal::HalError = BraketError::TaskCancelled("user".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::JobCancelled));
    }

    #[test]
    fn test_device_unavailable_to_hal() {
        let hal: arvak_hal::HalError = BraketError::DeviceUnavailable("offline".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::BackendUnavailable(_)));
    }

    #[test]
    fn test_timeout_to_hal() {
        let hal: arvak_hal::HalError = BraketError::Timeout.into();
        assert!(matches!(hal, arvak_hal::HalError::Timeout(_)));
    }

    #[test]
    fn test_too_many_qubits_to_hal() {
        let hal: arvak_hal::HalError = BraketError::TooManyQubits {
            required: 50,
            available: 25,
        }
        .into();
        assert!(matches!(hal, arvak_hal::HalError::CircuitTooLarge(_)));
    }

    #[test]
    fn test_circuit_error_to_hal() {
        let hal: arvak_hal::HalError = BraketError::CircuitError("bad gate".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::InvalidCircuit(_)));
    }

    #[test]
    fn test_braket_api_to_hal_backend() {
        let hal: arvak_hal::HalError = BraketError::BraketApi("server error".into()).into();
        assert!(matches!(hal, arvak_hal::HalError::Backend(_)));
    }
}
