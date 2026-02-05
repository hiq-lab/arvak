//! Error types for quantum types.

use thiserror::Error;

/// Errors that can occur when working with quantum types.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TypeError {
    /// Bit width mismatch between operands.
    #[error("Bit width mismatch: expected {expected}, got {got}")]
    BitWidthMismatch { expected: usize, got: usize },

    /// Invalid bit width for type.
    #[error("Invalid bit width: {0}")]
    InvalidBitWidth(usize),

    /// Overflow in arithmetic operation.
    #[error("Arithmetic overflow")]
    Overflow,

    /// Underflow in arithmetic operation.
    #[error("Arithmetic underflow")]
    Underflow,

    /// Invalid exponent range for QuantumFloat.
    #[error("Invalid exponent range: [{min}, {max}]")]
    InvalidExponentRange { min: i32, max: i32 },

    /// Not enough qubits for operation.
    #[error("Insufficient qubits: need {needed}, have {available}")]
    InsufficientQubits { needed: usize, available: usize },

    /// Array index out of bounds.
    #[error("Index {index} out of bounds for array of size {size}")]
    IndexOutOfBounds { index: usize, size: usize },

    /// Circuit error during gate application.
    #[error("Circuit error: {0}")]
    CircuitError(String),
}

/// Result type for quantum type operations.
pub type TypeResult<T> = Result<T, TypeError>;
