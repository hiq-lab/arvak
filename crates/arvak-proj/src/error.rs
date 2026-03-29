//! Error types for the projector simulator.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjError {
    #[error("frequency extraction failed: {0}")]
    FrequencyExtraction(String),

    #[error("MPS bond dimension overflow at bond {bond}: required {required}, max {max}")]
    BondOverflow {
        bond: usize,
        required: usize,
        max: usize,
    },

    #[error("SVD failed at bond {0}")]
    SvdFailed(usize),

    #[error("qubit index {0} out of range (n_qubits = {1})")]
    QubitOutOfRange(usize, usize),

    #[error("empty circuit")]
    EmptyCircuit,
}

pub type Result<T> = std::result::Result<T, ProjError>;
