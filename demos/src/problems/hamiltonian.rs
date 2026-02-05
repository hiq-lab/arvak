//! Pauli Hamiltonian representation for quantum chemistry and optimization.
//!
//! A Hamiltonian is represented as a sum of Pauli strings:
//! H = Σᵢ cᵢ Pᵢ
//! where each Pᵢ is a tensor product of Pauli operators.

use serde::{Deserialize, Serialize};

/// A single Pauli operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pauli {
    /// Identity operator.
    I,
    /// Pauli-X operator.
    X,
    /// Pauli-Y operator.
    Y,
    /// Pauli-Z operator.
    Z,
}

impl Pauli {
    /// Get the name of this Pauli operator.
    pub fn name(&self) -> &'static str {
        match self {
            Pauli::I => "I",
            Pauli::X => "X",
            Pauli::Y => "Y",
            Pauli::Z => "Z",
        }
    }
}

impl std::fmt::Display for Pauli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A single term in a Pauli Hamiltonian.
///
/// Represents cᵢ * (P₀ ⊗ P₁ ⊗ ... ⊗ Pₙ)
/// where only non-identity Paulis are stored explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PauliTerm {
    /// The coefficient of this term.
    pub coefficient: f64,
    /// The non-identity Pauli operators, as (qubit_index, pauli).
    /// Empty means identity on all qubits.
    pub operators: Vec<(usize, Pauli)>,
}

impl PauliTerm {
    /// Create a new Pauli term.
    pub fn new(coefficient: f64, operators: Vec<(usize, Pauli)>) -> Self {
        Self {
            coefficient,
            operators,
        }
    }

    /// Create an identity term (scalar).
    pub fn identity(coefficient: f64) -> Self {
        Self::new(coefficient, vec![])
    }

    /// Create a single-qubit Z term.
    pub fn z(coefficient: f64, qubit: usize) -> Self {
        Self::new(coefficient, vec![(qubit, Pauli::Z)])
    }

    /// Create a single-qubit X term.
    pub fn x(coefficient: f64, qubit: usize) -> Self {
        Self::new(coefficient, vec![(qubit, Pauli::X)])
    }

    /// Create a single-qubit Y term.
    pub fn y(coefficient: f64, qubit: usize) -> Self {
        Self::new(coefficient, vec![(qubit, Pauli::Y)])
    }

    /// Create a ZZ term.
    pub fn zz(coefficient: f64, qubit1: usize, qubit2: usize) -> Self {
        Self::new(coefficient, vec![(qubit1, Pauli::Z), (qubit2, Pauli::Z)])
    }

    /// Create an XX term.
    pub fn xx(coefficient: f64, qubit1: usize, qubit2: usize) -> Self {
        Self::new(coefficient, vec![(qubit1, Pauli::X), (qubit2, Pauli::X)])
    }

    /// Create a YY term.
    pub fn yy(coefficient: f64, qubit1: usize, qubit2: usize) -> Self {
        Self::new(coefficient, vec![(qubit1, Pauli::Y), (qubit2, Pauli::Y)])
    }

    /// Check if this is an identity term.
    pub fn is_identity(&self) -> bool {
        self.operators.is_empty()
    }

    /// Get the qubits this term acts on.
    pub fn qubits(&self) -> impl Iterator<Item = usize> + '_ {
        self.operators.iter().map(|(q, _)| *q)
    }

    /// Get the maximum qubit index.
    pub fn max_qubit(&self) -> Option<usize> {
        self.operators.iter().map(|(q, _)| *q).max()
    }
}

impl std::fmt::Display for PauliTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.coefficient >= 0.0 {
            write!(f, "+{:.4} ", self.coefficient)?;
        } else {
            write!(f, "{:.4} ", self.coefficient)?;
        }

        if self.operators.is_empty() {
            write!(f, "I")?;
        } else {
            for (i, (qubit, pauli)) in self.operators.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{}[{}]", pauli, qubit)?;
            }
        }
        Ok(())
    }
}

/// A Hamiltonian represented as a sum of Pauli terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PauliHamiltonian {
    /// The terms in the Hamiltonian.
    pub terms: Vec<PauliTerm>,
}

impl PauliHamiltonian {
    /// Create a new Hamiltonian from a list of terms.
    pub fn new(terms: Vec<PauliTerm>) -> Self {
        Self { terms }
    }

    /// Create an empty Hamiltonian.
    pub fn empty() -> Self {
        Self { terms: vec![] }
    }

    /// Add a term to the Hamiltonian.
    pub fn add_term(&mut self, term: PauliTerm) {
        self.terms.push(term);
    }

    /// Get the number of terms.
    pub fn num_terms(&self) -> usize {
        self.terms.len()
    }

    /// Get the number of qubits needed.
    pub fn num_qubits(&self) -> usize {
        self.terms
            .iter()
            .filter_map(|t| t.max_qubit())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0)
    }

    /// Get the identity coefficient (if any).
    pub fn identity_coefficient(&self) -> f64 {
        self.terms
            .iter()
            .filter(|t| t.is_identity())
            .map(|t| t.coefficient)
            .sum()
    }

    /// Iterate over non-identity terms.
    pub fn non_identity_terms(&self) -> impl Iterator<Item = &PauliTerm> {
        self.terms.iter().filter(|t| !t.is_identity())
    }
}

impl std::fmt::Display for PauliHamiltonian {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Hamiltonian ({} terms, {} qubits):",
            self.num_terms(),
            self.num_qubits()
        )?;
        for term in &self.terms {
            writeln!(f, "  {}", term)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pauli_term_creation() {
        let term = PauliTerm::zz(-0.5, 0, 1);
        assert_eq!(term.coefficient, -0.5);
        assert_eq!(term.operators.len(), 2);
        assert!(!term.is_identity());
    }

    #[test]
    fn test_identity_term() {
        let term = PauliTerm::identity(1.0);
        assert!(term.is_identity());
        assert_eq!(term.max_qubit(), None);
    }

    #[test]
    fn test_hamiltonian() {
        let h = PauliHamiltonian::new(vec![
            PauliTerm::identity(-1.0),
            PauliTerm::z(0.5, 0),
            PauliTerm::z(-0.5, 1),
            PauliTerm::zz(-0.25, 0, 1),
        ]);

        assert_eq!(h.num_terms(), 4);
        assert_eq!(h.num_qubits(), 2);
        assert_eq!(h.identity_coefficient(), -1.0);
    }
}
