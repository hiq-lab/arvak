//! Error types for the IR crate.

use crate::qubit::{ClbitId, QubitId};
use thiserror::Error;

/// Errors that can occur in IR operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IrError {
    /// Qubit not found in circuit.
    #[error("Qubit {qubit:?} not found in circuit{}", format_gate_context(.gate_name))]
    QubitNotFound {
        /// The qubit that was not found.
        qubit: QubitId,
        /// Optional gate name for context.
        gate_name: Option<String>,
    },

    /// Classical bit not found in circuit.
    #[error("Classical bit {clbit:?} not found in circuit{}", format_gate_context(.gate_name))]
    ClbitNotFound {
        /// The classical bit that was not found.
        clbit: ClbitId,
        /// Optional gate name for context.
        gate_name: Option<String>,
    },

    /// Invalid DAG structure.
    #[error("Invalid DAG structure: {0}")]
    InvalidDag(String),

    /// Invalid node index.
    #[error("Invalid node index")]
    InvalidNode,

    /// Gate requires different number of qubits.
    #[error("Gate '{gate_name}' requires {expected} qubits, got {got}")]
    QubitCountMismatch {
        /// Name of the gate.
        gate_name: String,
        /// Expected number of qubits.
        expected: u32,
        /// Actual number of qubits provided.
        got: u32,
    },

    /// Parameter is unbound.
    #[error("Parameter '{0}' is unbound")]
    UnboundParameter(String),

    /// Cannot perform operation on parameterized circuit.
    #[error("Cannot perform operation on parameterized circuit")]
    ParameterizedCircuit,

    /// Duplicate qubit in operation.
    #[error("Duplicate qubit {qubit:?} in operation{}", format_gate_context(.gate_name))]
    DuplicateQubit {
        /// The duplicate qubit.
        qubit: QubitId,
        /// Optional gate name for context.
        gate_name: Option<String>,
    },
}

/// Helper function to format optional gate context.
#[allow(clippy::ref_option)]
fn format_gate_context(gate_name: &Option<String>) -> String {
    match gate_name {
        Some(name) => format!(" (gate: {name})"),
        None => String::new(),
    }
}

/// Result type for IR operations.
pub type IrResult<T> = Result<T, IrError>;
