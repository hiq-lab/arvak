//! Error types for the compilation crate.

use thiserror::Error;

/// Errors that can occur during compilation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CompileError {
    /// Error from the IR crate.
    #[error("IR error: {0}")]
    Ir(#[from] hiq_ir::IrError),

    /// Missing coupling map for routing.
    #[error("Missing coupling map for routing")]
    MissingCouplingMap,

    /// Missing layout for routing.
    #[error("Missing layout for routing")]
    MissingLayout,

    /// Missing basis gates.
    #[error("Missing basis gates for translation")]
    MissingBasisGates,

    /// Routing failed because qubits are not connected.
    #[error("Routing failed: qubits {qubit1} and {qubit2} not connected")]
    RoutingFailed { qubit1: u32, qubit2: u32 },

    /// Gate not in target basis.
    #[error("Gate '{0}' not in target basis")]
    GateNotInBasis(String),

    /// Pass execution failed.
    #[error("Pass '{name}' failed: {reason}")]
    PassFailed { name: String, reason: String },

    /// Invalid pass configuration.
    #[error("Invalid pass configuration: {0}")]
    InvalidConfiguration(String),

    /// Circuit too large for target.
    #[error("Circuit requires {required} qubits but target only has {available}")]
    CircuitTooLarge { required: usize, available: u32 },
}

/// Result type for compilation operations.
pub type CompileResult<T> = Result<T, CompileError>;
