//! Hamiltonian data structures.
//!
//! A Hamiltonian is a sum of weighted Pauli strings:
//!
//!   H = Σ_k  c_k · P_k
//!
//! where each P_k is a tensor product of single-qubit Pauli operators
//! (I, X, Y, Z) and c_k ∈ ℝ.
//!
//! # Example
//!
//! ```rust
//! use arvak_sim::hamiltonian::{Hamiltonian, HamiltonianTerm, PauliOp, PauliString};
//!
//! // H = -1.0·Z₀Z₁  +  0.5·X₀
//! let h = Hamiltonian::from_terms(vec![
//!     HamiltonianTerm::new(-1.0, PauliString::from_ops(vec![(0, PauliOp::Z), (1, PauliOp::Z)])),
//!     HamiltonianTerm::new( 0.5, PauliString::from_ops(vec![(0, PauliOp::X)])),
//! ]);
//! assert_eq!(h.n_terms(), 2);
//! ```

use serde::{Deserialize, Serialize};

/// Single-qubit Pauli operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PauliOp {
    /// Identity — contributes a global phase; omit from synthesis.
    I,
    /// Pauli-X.
    X,
    /// Pauli-Y.
    Y,
    /// Pauli-Z.
    Z,
}

/// A tensor product of Pauli operators on named qubits.
///
/// Stored as a sorted `Vec<(qubit_index, PauliOp)>` with Identity terms
/// omitted.  Qubits not listed are implicitly I.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauliString {
    /// Non-identity terms, sorted by qubit index ascending.
    ops: Vec<(u32, PauliOp)>,
}

impl PauliString {
    /// Construct a PauliString from an iterator of (qubit, op) pairs.
    ///
    /// Identity operators are dropped; the remaining ops are sorted by qubit.
    pub fn from_ops(ops: impl IntoIterator<Item = (u32, PauliOp)>) -> Self {
        let mut v: Vec<(u32, PauliOp)> = ops
            .into_iter()
            .filter(|(_, op)| *op != PauliOp::I)
            .collect();
        v.sort_by_key(|(q, _)| *q);
        Self { ops: v }
    }

    /// Construct a Z⊗Z⊗...⊗Z string spanning the given qubits.
    pub fn zz(qubits: impl IntoIterator<Item = u32>) -> Self {
        let mut v: Vec<(u32, PauliOp)> = qubits.into_iter().map(|q| (q, PauliOp::Z)).collect();
        v.sort_by_key(|(q, _)| *q);
        Self { ops: v }
    }

    /// Return the non-identity (qubit, op) pairs, sorted by qubit index.
    pub fn ops(&self) -> &[(u32, PauliOp)] {
        &self.ops
    }

    /// True if there are no non-identity operators (pure global phase).
    pub fn is_identity(&self) -> bool {
        self.ops.is_empty()
    }

    /// The highest qubit index referenced, or `None` for an identity string.
    pub fn max_qubit(&self) -> Option<u32> {
        self.ops.last().map(|(q, _)| *q)
    }
}

/// A single weighted Pauli term: `coeff · pauli`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HamiltonianTerm {
    /// Real coefficient.
    pub coeff: f64,
    /// The Pauli string.
    pub pauli: PauliString,
}

impl HamiltonianTerm {
    /// Create a new term.
    pub fn new(coeff: f64, pauli: PauliString) -> Self {
        Self { coeff, pauli }
    }

    /// Shorthand: single-qubit Z term.
    pub fn z(qubit: u32, coeff: f64) -> Self {
        Self::new(coeff, PauliString::from_ops([(qubit, PauliOp::Z)]))
    }

    /// Shorthand: ZZ coupling term.
    pub fn zz(q0: u32, q1: u32, coeff: f64) -> Self {
        Self::new(
            coeff,
            PauliString::from_ops([(q0, PauliOp::Z), (q1, PauliOp::Z)]),
        )
    }

    /// Shorthand: single-qubit X term.
    pub fn x(qubit: u32, coeff: f64) -> Self {
        Self::new(coeff, PauliString::from_ops([(qubit, PauliOp::X)]))
    }
}

/// A sum-of-Pauli-strings Hamiltonian.
///
/// H = Σ_k  c_k · P_k
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hamiltonian {
    terms: Vec<HamiltonianTerm>,
}

impl Hamiltonian {
    /// Create from a list of terms.
    pub fn from_terms(terms: Vec<HamiltonianTerm>) -> Self {
        Self { terms }
    }

    /// All terms.
    pub fn terms(&self) -> &[HamiltonianTerm] {
        &self.terms
    }

    /// Number of terms.
    pub fn n_terms(&self) -> usize {
        self.terms.len()
    }

    /// Spectral norm upper bound: Σ |c_k| (used by QDrift).
    pub fn lambda(&self) -> f64 {
        self.terms.iter().map(|t| t.coeff.abs()).sum()
    }

    /// The minimum number of qubits required to represent this Hamiltonian.
    ///
    /// Returns 0 if the Hamiltonian is empty or purely identity.
    pub fn min_qubits(&self) -> u32 {
        self.terms
            .iter()
            .filter_map(|t| t.pauli.max_qubit())
            .max()
            .map_or(0, |q| q + 1)
    }
}

impl FromIterator<HamiltonianTerm> for Hamiltonian {
    fn from_iter<T: IntoIterator<Item = HamiltonianTerm>>(iter: T) -> Self {
        Self {
            terms: iter.into_iter().collect(),
        }
    }
}
