//! Evaluator error types.

use thiserror::Error;

/// Result type for evaluator operations.
pub type EvalResult<T> = Result<T, EvalError>;

/// Errors that can occur during evaluation.
#[derive(Debug, Error)]
pub enum EvalError {
    /// Input parsing failed.
    #[error("Parse error: {0}")]
    Parse(String),

    /// Compilation error.
    #[error("Compilation error: {0}")]
    Compilation(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(String),

    /// Serialization error.
    #[error("Export error: {0}")]
    Export(String),
}

impl From<arvak_qasm3::ParseError> for EvalError {
    fn from(e: arvak_qasm3::ParseError) -> Self {
        EvalError::Parse(e.to_string())
    }
}

impl From<arvak_compile::CompileError> for EvalError {
    fn from(e: arvak_compile::CompileError) -> Self {
        EvalError::Compilation(e.to_string())
    }
}

impl From<serde_json::Error> for EvalError {
    fn from(e: serde_json::Error) -> Self {
        EvalError::Export(e.to_string())
    }
}
