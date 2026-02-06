//! Qubit register management for high-level types.

use hiq_ir::Circuit;
use hiq_ir::qubit::QubitId;
use serde::{Deserialize, Serialize};

/// Allocation strategy for qubit registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegisterAllocation {
    /// Allocate new qubits from the circuit.
    New,
    /// Use existing qubits (for ancilla reuse).
    Existing,
}

impl Default for RegisterAllocation {
    fn default() -> Self {
        Self::New
    }
}

/// A register of qubits representing a quantum value.
///
/// This is the building block for high-level quantum types.
/// It tracks which qubits belong to a logical quantum variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QubitRegister {
    /// The qubits in this register, ordered from LSB to MSB.
    qubits: Vec<QubitId>,
    /// Label for this register (for debugging/visualization).
    label: Option<String>,
    /// Whether this register owns its qubits (vs borrowing).
    owns_qubits: bool,
}

impl QubitRegister {
    /// Create a new register with freshly allocated qubits.
    pub fn new(circuit: &mut Circuit, size: usize) -> Self {
        let start = circuit.num_qubits();

        // Extend circuit to have enough qubits
        let total = start + size;
        while circuit.num_qubits() < total {
            circuit.add_qubit();
        }

        let qubits = (start..total).map(|i| QubitId(i as u32)).collect();

        Self {
            qubits,
            label: None,
            owns_qubits: true,
        }
    }

    /// Create a register from existing qubits.
    pub fn from_qubits(qubits: Vec<QubitId>) -> Self {
        Self {
            qubits,
            label: None,
            owns_qubits: false,
        }
    }

    /// Create a register with a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Get the number of qubits in this register.
    pub fn len(&self) -> usize {
        self.qubits.len()
    }

    /// Check if the register is empty.
    pub fn is_empty(&self) -> bool {
        self.qubits.is_empty()
    }

    /// Get the qubits in this register.
    pub fn qubits(&self) -> &[QubitId] {
        &self.qubits
    }

    /// Get a specific qubit by index.
    pub fn qubit(&self, index: usize) -> Option<QubitId> {
        self.qubits.get(index).copied()
    }

    /// Get the LSB qubit.
    pub fn lsb(&self) -> Option<QubitId> {
        self.qubits.first().copied()
    }

    /// Get the MSB qubit.
    pub fn msb(&self) -> Option<QubitId> {
        self.qubits.last().copied()
    }

    /// Get the label if set.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Check if this register owns its qubits.
    pub fn owns_qubits(&self) -> bool {
        self.owns_qubits
    }

    /// Split the register at an index, returning (lower, upper).
    pub fn split_at(&self, index: usize) -> (QubitRegister, QubitRegister) {
        let (lower, upper) = self.qubits.split_at(index.min(self.qubits.len()));
        (
            QubitRegister::from_qubits(lower.to_vec()),
            QubitRegister::from_qubits(upper.to_vec()),
        )
    }

    /// Concatenate two registers.
    pub fn concat(&self, other: &QubitRegister) -> QubitRegister {
        let mut qubits = self.qubits.clone();
        qubits.extend(other.qubits.iter().copied());
        QubitRegister::from_qubits(qubits)
    }

    /// Iterate over qubits.
    pub fn iter(&self) -> impl Iterator<Item = QubitId> + '_ {
        self.qubits.iter().copied()
    }
}

impl IntoIterator for QubitRegister {
    type Item = QubitId;
    type IntoIter = std::vec::IntoIter<QubitId>;

    fn into_iter(self) -> Self::IntoIter {
        self.qubits.into_iter()
    }
}

impl<'a> IntoIterator for &'a QubitRegister {
    type Item = &'a QubitId;
    type IntoIter = std::slice::Iter<'a, QubitId>;

    fn into_iter(self) -> Self::IntoIter {
        self.qubits.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_creation() {
        let mut circuit = Circuit::new("test");
        let reg = QubitRegister::new(&mut circuit, 4);

        assert_eq!(reg.len(), 4);
        assert_eq!(circuit.num_qubits(), 4);
        assert!(reg.owns_qubits());
    }

    #[test]
    fn test_register_from_qubits() {
        let qubits = vec![QubitId(0), QubitId(1), QubitId(2)];
        let reg = QubitRegister::from_qubits(qubits);

        assert_eq!(reg.len(), 3);
        assert!(!reg.owns_qubits());
    }

    #[test]
    fn test_register_split() {
        let qubits = vec![QubitId(0), QubitId(1), QubitId(2), QubitId(3)];
        let reg = QubitRegister::from_qubits(qubits);
        let (lower, upper) = reg.split_at(2);

        assert_eq!(lower.len(), 2);
        assert_eq!(upper.len(), 2);
        assert_eq!(lower.qubits(), &[QubitId(0), QubitId(1)]);
        assert_eq!(upper.qubits(), &[QubitId(2), QubitId(3)]);
    }

    #[test]
    fn test_register_concat() {
        let reg1 = QubitRegister::from_qubits(vec![QubitId(0), QubitId(1)]);
        let reg2 = QubitRegister::from_qubits(vec![QubitId(2), QubitId(3)]);
        let combined = reg1.concat(&reg2);

        assert_eq!(combined.len(), 4);
        assert_eq!(
            combined.qubits(),
            &[QubitId(0), QubitId(1), QubitId(2), QubitId(3)]
        );
    }
}
