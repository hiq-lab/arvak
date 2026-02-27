//! Trotter-Suzuki product-formula synthesis.
//!
//! Approximates `exp(-i H t)` by splitting the evolution into `n_steps`
//! slices, each evolved exactly under each term in sequence.
//!
//! # First-order Trotter (Lie-Trotter)
//!
//!   exp(-i H t) ≈ [∏_k exp(-i c_k P_k t/n)]^n
//!
//! Error: O(t² / n).
//!
//! # Second-order Trotter (Suzuki-Trotter)
//!
//!   exp(-i H t) ≈ [S₂(t/n)]^n
//!   S₂(τ) = [∏_k exp(-i c_k P_k τ/2)] · [∏_k exp(-i c_{n-k} P_{n-k} τ/2)]
//!
//! Error: O(t³ / n²).

use arvak_ir::{Circuit, QubitId};
use tracing::debug;

use crate::error::{SimError, SimResult};
use crate::hamiltonian::Hamiltonian;
use crate::synthesis::append_exp_pauli;

/// Trotter product-formula time-evolution synthesiser.
pub struct TrotterEvolution {
    hamiltonian: Hamiltonian,
    /// Total evolution time t.
    t: f64,
    /// Number of Trotter steps (repetitions).
    n_steps: usize,
    /// Number of qubits; if None, inferred from the Hamiltonian.
    n_qubits: Option<u32>,
}

impl TrotterEvolution {
    /// Construct a new first- or second-order Trotter synthesiser.
    ///
    /// # Arguments
    /// * `hamiltonian` — the Hamiltonian H = Σ c_k P_k
    /// * `t`          — total evolution time
    /// * `n_steps`    — number of Trotter slices (higher → more accurate)
    pub fn new(hamiltonian: Hamiltonian, t: f64, n_steps: usize) -> Self {
        Self {
            hamiltonian,
            t,
            n_steps,
            n_qubits: None,
        }
    }

    /// Override the circuit width (number of qubits).
    ///
    /// By default the width is inferred from the highest qubit index in the
    /// Hamiltonian.  Use this method to pad extra ancilla qubits.
    #[must_use]
    pub fn with_n_qubits(mut self, n: u32) -> Self {
        self.n_qubits = Some(n);
        self
    }

    /// Synthesise a first-order Trotter circuit.
    ///
    /// Each Trotter slice applies every term once with time step `t / n_steps`.
    pub fn first_order(&self) -> SimResult<Circuit> {
        self.validate()?;
        let n_qubits = self.effective_n_qubits();
        let step_t = self.t / self.n_steps as f64;

        let mut circuit = Circuit::with_size("trotter1", n_qubits, 0);
        debug!(
            n_terms = self.hamiltonian.n_terms(),
            n_steps = self.n_steps,
            n_qubits,
            "synthesising first-order Trotter circuit"
        );

        for _ in 0..self.n_steps {
            for term in self.hamiltonian.terms() {
                append_exp_pauli(&mut circuit, term, step_t, n_qubits)?;
            }
        }

        // Append identity gate on every qubit that would otherwise have no
        // instructions, so downstream passes can reason about the full width.
        ensure_all_qubits_touched(&mut circuit, n_qubits)?;

        Ok(circuit)
    }

    /// Synthesise a second-order Suzuki-Trotter circuit.
    ///
    /// Each slice is a symmetric product: forward half-step then reverse
    /// half-step, giving O(t³/n²) error.
    pub fn second_order(&self) -> SimResult<Circuit> {
        self.validate()?;
        let n_qubits = self.effective_n_qubits();
        let half_t = self.t / (2.0 * self.n_steps as f64);

        let mut circuit = Circuit::with_size("trotter2", n_qubits, 0);
        debug!(
            n_terms = self.hamiltonian.n_terms(),
            n_steps = self.n_steps,
            n_qubits,
            "synthesising second-order Trotter circuit"
        );

        for _ in 0..self.n_steps {
            // Forward sweep: exp(-i c_k P_k τ/2)  for k = 0..n
            for term in self.hamiltonian.terms() {
                append_exp_pauli(&mut circuit, term, half_t, n_qubits)?;
            }
            // Reverse sweep: exp(-i c_k P_k τ/2)  for k = n-1..0
            for term in self.hamiltonian.terms().iter().rev() {
                append_exp_pauli(&mut circuit, term, half_t, n_qubits)?;
            }
        }

        ensure_all_qubits_touched(&mut circuit, n_qubits)?;
        Ok(circuit)
    }

    fn validate(&self) -> SimResult<()> {
        if self.hamiltonian.n_terms() == 0 {
            return Err(SimError::EmptyHamiltonian);
        }
        if self.n_steps == 0 {
            return Err(SimError::InvalidSteps(0));
        }
        Ok(())
    }

    fn effective_n_qubits(&self) -> u32 {
        self.n_qubits
            .unwrap_or_else(|| self.hamiltonian.min_qubits())
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Apply an Rz(0) on any qubit that has not yet been touched, so the circuit
/// DAG contains all qubits in its wire list.
fn ensure_all_qubits_touched(circuit: &mut Circuit, n_qubits: u32) -> SimResult<()> {
    for q in 0..n_qubits {
        // Rz(0) = identity — no physical effect, keeps the qubit live.
        circuit.rz(0.0f64, QubitId(q))?;
    }
    Ok(())
}
