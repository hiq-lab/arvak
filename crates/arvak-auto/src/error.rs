//! Error types for automatic uncomputation.

use thiserror::Error;

/// Errors that can occur during automatic uncomputation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UncomputeError {
    /// Cannot uncompute a qubit that is still entangled with output.
    #[error("Qubit {0} is entangled with output and cannot be uncomputed")]
    EntangledWithOutput(u32),

    /// Cannot uncompute a qubit that was measured.
    #[error("Qubit {0} was measured and cannot be uncomputed")]
    MeasuredQubit(u32),

    /// Cannot invert a non-unitary operation.
    #[error("Cannot invert non-unitary operation: {0}")]
    NonUnitaryOperation(String),

    /// Circuit analysis failed.
    #[error("Circuit analysis failed: {0}")]
    AnalysisFailed(String),

    /// No uncomputation context was established.
    #[error("No uncomputation context - call UncomputeContext::begin() first")]
    NoContext,

    /// Context mismatch (nested contexts incorrectly closed).
    #[error("Context mismatch: expected {expected}, got {got}")]
    ContextMismatch { expected: String, got: String },

    /// The gate cannot be inverted.
    #[error("Gate {0} cannot be inverted")]
    NonInvertibleGate(String),

    /// The gate IS mathematically invertible, but inversion is not yet implemented.
    #[error("Gate {0} inversion not yet implemented (gate is invertible; requires decomposition)")]
    InversionNotImplemented(String),

    /// Dependency cycle detected.
    #[error("Dependency cycle detected involving qubit {0}")]
    DependencyCycle(u32),

    /// Circuit error during uncomputation.
    #[error("Circuit error: {0}")]
    CircuitError(String),
}

/// Result type for uncomputation operations.
pub type UncomputeResult<T> = Result<T, UncomputeError>;
