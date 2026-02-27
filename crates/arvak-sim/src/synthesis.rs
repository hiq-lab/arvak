//! Pauli-string exponentiation.
//!
//! Implements the standard gate synthesis for
//!
//!   exp(-i · coeff · t · P)
//!
//! where P is a tensor product of Pauli operators, using the circuit identity:
//!
//!   exp(-i θ/2 · Z⊗Z⊗...⊗Z) = CNOT_ladder · Rz(θ) · CNOT_ladder†
//!
//! with basis rotations applied before/after to handle X and Y factors:
//!   X → H · Z · H
//!   Y → Sdg · H · Z · H · S
//!   Z → identity
//!
//! Gate count per term: 2·(k-1) CX + 2·k basis gates + 1 Rz,
//! where k = number of non-identity qubits.

use arvak_ir::{Circuit, QubitId};

use crate::error::SimResult;
use crate::hamiltonian::{HamiltonianTerm, PauliOp};

/// Append the circuit for `exp(-i · coeff · t · P)` to `circuit`.
///
/// `n_qubits` is the total width of the circuit (used for bounds checking
/// only; the circuit must already have been allocated with that many qubits).
///
/// If the Pauli string is the identity operator the function is a no-op
/// (global phase — unobservable).
pub fn append_exp_pauli(
    circuit: &mut Circuit,
    term: &HamiltonianTerm,
    t: f64,
    n_qubits: u32,
) -> SimResult<()> {
    use crate::error::SimError;

    let ops = term.pauli.ops();
    if ops.is_empty() {
        // Pure global phase — nothing to do.
        return Ok(());
    }

    // Bounds check.
    for &(q, _) in ops {
        if q >= n_qubits {
            return Err(SimError::QubitOutOfRange { qubit: q, n_qubits });
        }
    }

    // θ = 2 · coeff · t  (Rz(θ) implements exp(-i θ/2 Z))
    let theta = 2.0 * term.coeff * t;

    // --- Step 1: basis rotations (diagonalise each Pauli into Z) ---
    basis_change(circuit, ops, false)?;

    // --- Step 2: CNOT ladder collapsing parity onto the last qubit ---
    let qubits: Vec<u32> = ops.iter().map(|(q, _)| *q).collect();
    cnot_ladder(circuit, &qubits)?;

    // --- Step 3: Rz(θ) on the last qubit ---
    let target = QubitId(*qubits.last().expect("non-empty checked above"));
    circuit.rz(theta, target)?;

    // --- Step 4: undo CNOT ladder ---
    cnot_ladder_reverse(circuit, &qubits)?;

    // --- Step 5: undo basis rotations ---
    basis_change(circuit, ops, true)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Apply basis-change gates for each Pauli operator.
///
/// For the forward pass (`undo = false`):
///   X → H
///   Y → Sdg · H
///   Z → (nothing)
///
/// For the reverse pass (`undo = true`):
///   X → H  (H is self-inverse)
///   Y → H · S
///   Z → (nothing)
fn basis_change(circuit: &mut Circuit, ops: &[(u32, PauliOp)], undo: bool) -> SimResult<()> {
    for &(q, op) in ops {
        let qid = QubitId(q);
        match (op, undo) {
            (PauliOp::X, _) => {
                circuit.h(qid)?;
            }
            (PauliOp::Y, false) => {
                circuit.sdg(qid)?;
                circuit.h(qid)?;
            }
            (PauliOp::Y, true) => {
                circuit.h(qid)?;
                circuit.s(qid)?;
            }
            (PauliOp::Z | PauliOp::I, _) => {}
        }
    }
    Ok(())
}

/// Apply a forward CNOT ladder: CX(q[0],q[1]), CX(q[1],q[2]), …, CX(q[k-2], q[k-1]).
///
/// This ladder parity-encodes the XOR of all qubits into the last qubit,
/// enabling a single Rz to implement the tensor-product Pauli rotation.
fn cnot_ladder(circuit: &mut Circuit, qubits: &[u32]) -> SimResult<()> {
    for window in qubits.windows(2) {
        let ctrl = QubitId(window[0]);
        let tgt = QubitId(window[1]);
        circuit.cx(ctrl, tgt)?;
    }
    Ok(())
}

/// Apply the reverse CNOT ladder (identical to forward — CNOT is self-inverse
/// up to ordering, and the ladder is its own inverse when run backwards).
fn cnot_ladder_reverse(circuit: &mut Circuit, qubits: &[u32]) -> SimResult<()> {
    for window in qubits.windows(2).rev() {
        let ctrl = QubitId(window[0]);
        let tgt = QubitId(window[1]);
        circuit.cx(ctrl, tgt)?;
    }
    Ok(())
}
