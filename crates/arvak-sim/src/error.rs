//! Error types for the sim crate.

use thiserror::Error;

/// Errors produced by Hamiltonian time-evolution synthesis.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SimError {
    /// Hamiltonian contains no terms.
    #[error("Hamiltonian is empty — no terms to synthesise")]
    EmptyHamiltonian,

    /// A Pauli string references a qubit index that is out of range.
    #[error("Pauli string references qubit {qubit} but circuit only has {n_qubits} qubits")]
    QubitOutOfRange {
        /// The offending qubit index.
        qubit: u32,
        /// Number of qubits in the target circuit.
        n_qubits: u32,
    },

    /// Circuit builder returned an error.
    #[error("Circuit IR error: {0}")]
    Ir(#[from] arvak_ir::IrError),

    /// n_steps must be ≥ 1.
    #[error("n_steps must be at least 1, got {0}")]
    InvalidSteps(usize),

    /// n_samples must be ≥ 1 for QDrift.
    #[error("n_samples must be at least 1, got {0}")]
    InvalidSamples(usize),
}

/// Result type for simulation synthesis operations.
pub type SimResult<T> = Result<T, SimError>;
