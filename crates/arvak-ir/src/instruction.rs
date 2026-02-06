//! Circuit instructions combining gates with operands.

use serde::{Deserialize, Serialize};

use crate::gate::{Gate, StandardGate};
use crate::qubit::{ClbitId, QubitId};

/// The kind of instruction in a circuit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InstructionKind {
    /// A quantum gate operation.
    Gate(Gate),
    /// Measurement operation.
    Measure,
    /// Reset qubit to |0⟩.
    Reset,
    /// Barrier (synchronization point).
    Barrier,
    /// Delay instruction.
    Delay {
        /// Duration in device-specific units.
        duration: u64,
    },
}

/// A complete instruction with operands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instruction {
    /// The kind of instruction.
    pub kind: InstructionKind,
    /// Qubits this instruction operates on.
    pub qubits: Vec<QubitId>,
    /// Classical bits this instruction operates on (for measure).
    pub clbits: Vec<ClbitId>,
}

impl Instruction {
    /// Create a gate instruction.
    pub fn gate(gate: impl Into<Gate>, qubits: impl IntoIterator<Item = QubitId>) -> Self {
        Self {
            kind: InstructionKind::Gate(gate.into()),
            qubits: qubits.into_iter().collect(),
            clbits: vec![],
        }
    }

    /// Create a single-qubit gate instruction.
    pub fn single_qubit_gate(gate: StandardGate, qubit: QubitId) -> Self {
        Self::gate(gate, [qubit])
    }

    /// Create a two-qubit gate instruction.
    pub fn two_qubit_gate(gate: StandardGate, q1: QubitId, q2: QubitId) -> Self {
        Self::gate(gate, [q1, q2])
    }

    /// Create a measurement instruction.
    pub fn measure(qubit: QubitId, clbit: ClbitId) -> Self {
        Self {
            kind: InstructionKind::Measure,
            qubits: vec![qubit],
            clbits: vec![clbit],
        }
    }

    /// Create a multi-qubit measurement instruction.
    pub fn measure_all(
        qubits: impl IntoIterator<Item = QubitId>,
        clbits: impl IntoIterator<Item = ClbitId>,
    ) -> Self {
        Self {
            kind: InstructionKind::Measure,
            qubits: qubits.into_iter().collect(),
            clbits: clbits.into_iter().collect(),
        }
    }

    /// Create a reset instruction.
    pub fn reset(qubit: QubitId) -> Self {
        Self {
            kind: InstructionKind::Reset,
            qubits: vec![qubit],
            clbits: vec![],
        }
    }

    /// Create a barrier instruction.
    pub fn barrier(qubits: impl IntoIterator<Item = QubitId>) -> Self {
        Self {
            kind: InstructionKind::Barrier,
            qubits: qubits.into_iter().collect(),
            clbits: vec![],
        }
    }

    /// Create a delay instruction.
    pub fn delay(qubit: QubitId, duration: u64) -> Self {
        Self {
            kind: InstructionKind::Delay { duration },
            qubits: vec![qubit],
            clbits: vec![],
        }
    }

    /// Check if this is a gate instruction.
    pub fn is_gate(&self) -> bool {
        matches!(self.kind, InstructionKind::Gate(_))
    }

    /// Check if this is a measurement.
    pub fn is_measure(&self) -> bool {
        matches!(self.kind, InstructionKind::Measure)
    }

    /// Check if this is a reset.
    pub fn is_reset(&self) -> bool {
        matches!(self.kind, InstructionKind::Reset)
    }

    /// Check if this is a barrier.
    pub fn is_barrier(&self) -> bool {
        matches!(self.kind, InstructionKind::Barrier)
    }

    /// Get the gate if this is a gate instruction.
    pub fn as_gate(&self) -> Option<&Gate> {
        match &self.kind {
            InstructionKind::Gate(g) => Some(g),
            _ => None,
        }
    }

    /// Get mutable reference to the gate.
    pub fn gate_mut(&mut self) -> Option<&mut Gate> {
        match &mut self.kind {
            InstructionKind::Gate(g) => Some(g),
            _ => None,
        }
    }

    /// Get the name of the instruction.
    pub fn name(&self) -> &str {
        match &self.kind {
            InstructionKind::Gate(g) => g.name(),
            InstructionKind::Measure => "measure",
            InstructionKind::Reset => "reset",
            InstructionKind::Barrier => "barrier",
            InstructionKind::Delay { .. } => "delay",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_instruction() {
        let inst = Instruction::single_qubit_gate(StandardGate::H, QubitId(0));
        assert!(inst.is_gate());
        assert_eq!(inst.qubits.len(), 1);
        assert_eq!(inst.name(), "h");
    }

    #[test]
    fn test_measure_instruction() {
        let inst = Instruction::measure(QubitId(0), ClbitId(0));
        assert!(inst.is_measure());
        assert_eq!(inst.qubits.len(), 1);
        assert_eq!(inst.clbits.len(), 1);
    }

    #[test]
    fn test_barrier_instruction() {
        let inst = Instruction::barrier([QubitId(0), QubitId(1), QubitId(2)]);
        assert!(inst.is_barrier());
        assert_eq!(inst.qubits.len(), 3);
    }
}
