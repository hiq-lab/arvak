//! Error handling for the HPC scheduler.

use thiserror::Error;

/// Result type for scheduler operations.
pub type SchedResult<T> = Result<T, SchedError>;

/// Errors that can occur during scheduler operations.
#[derive(Error, Debug)]
pub enum SchedError {
    /// Job not found in the scheduler.
    #[error("Job not found: {0}")]
    JobNotFound(String),

    /// Workflow not found in the scheduler.
    #[error("Workflow not found: {0}")]
    WorkflowNotFound(String),

    /// Invalid job state for the requested operation.
    #[error("Invalid job state: expected {expected}, found {found}")]
    InvalidJobState { expected: String, found: String },

    /// Job dependency cycle detected.
    #[error("Dependency cycle detected in workflow")]
    DependencyCycle,

    /// Invalid dependency reference.
    #[error("Invalid dependency: job {0} not found")]
    InvalidDependency(String),

    /// SLURM submission failed.
    #[error("SLURM submission failed: {0}")]
    SlurmSubmitError(String),

    /// SLURM command execution failed.
    #[error("SLURM command failed: {command} - {message}")]
    SlurmCommandError { command: String, message: String },

    /// SLURM job not found.
    #[error("SLURM job not found: {0}")]
    SlurmJobNotFound(String),

    /// PBS submission failed.
    #[error("PBS submission failed: {0}")]
    PbsSubmitError(String),

    /// PBS command execution failed.
    #[error("PBS command failed: {command} - {message}")]
    PbsCommandError { command: String, message: String },

    /// PBS job not found.
    #[error("PBS job not found: {0}")]
    PbsJobNotFound(String),

    /// No suitable backend found for the job requirements.
    #[error("No matching backend found: {0}")]
    NoMatchingBackend(String),

    /// Backend error during execution.
    #[error("Backend error: {0}")]
    BackendError(String),

    /// Persistence error.
    #[error("Persistence error: {0}")]
    PersistenceError(String),

    /// SQLite database error.
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// IO error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Parse error from QASM.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Timeout waiting for job completion.
    #[error("Job timeout: {0}")]
    Timeout(String),

    /// Job was cancelled.
    #[error("Job cancelled: {0}")]
    Cancelled(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Internal scheduler error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<hiq_hal::HalError> for SchedError {
    fn from(e: hiq_hal::HalError) -> Self {
        SchedError::BackendError(e.to_string())
    }
}

impl From<rusqlite::Error> for SchedError {
    fn from(e: rusqlite::Error) -> Self {
        SchedError::DatabaseError(e.to_string())
    }
}

impl From<hiq_qasm3::ParseError> for SchedError {
    fn from(e: hiq_qasm3::ParseError) -> Self {
        SchedError::ParseError(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SchedError::JobNotFound("job-123".to_string());
        assert_eq!(err.to_string(), "Job not found: job-123");

        let err = SchedError::InvalidJobState {
            expected: "Pending".to_string(),
            found: "Running".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid job state: expected Pending, found Running"
        );

        let err = SchedError::DependencyCycle;
        assert_eq!(err.to_string(), "Dependency cycle detected in workflow");
    }
}
