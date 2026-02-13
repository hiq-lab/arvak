//! Circuit instructions combining gates with operands.

use serde::{Deserialize, Serialize};

use crate::gate::{Gate, StandardGate};
use crate::noise::{NoiseModel, NoiseRole};
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
    /// Shuttle qubit between zones (neutral-atom architectures).
    Shuttle {
        /// Source zone index.
        from_zone: u32,
        /// Destination zone index.
        to_zone: u32,
    },
    /// Noise channel operation.
    ///
    /// Represents a non-unitary noise process applied to a qubit.
    /// The [`NoiseRole`] determines whether the compiler may optimize
    /// around this channel (`Deficit`) or must preserve it (`Resource`).
    NoiseChannel {
        /// The noise model describing the physical process.
        model: NoiseModel,
        /// Semantic role: deficit (mitigate) or resource (preserve).
        role: NoiseRole,
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
    ///
    /// Returns an error if the number of qubits and classical bits do not match.
    pub fn measure_all(
        qubits: impl IntoIterator<Item = QubitId>,
        clbits: impl IntoIterator<Item = ClbitId>,
    ) -> crate::error::IrResult<Self> {
        let qubits: Vec<_> = qubits.into_iter().collect();
        let clbits: Vec<_> = clbits.into_iter().collect();
        if qubits.len() != clbits.len() {
            return Err(crate::error::IrError::InvalidDag(format!(
                "measure_all: qubit count ({}) does not match clbit count ({})",
                qubits.len(),
                clbits.len(),
            )));
        }
        Ok(Self {
            kind: InstructionKind::Measure,
            qubits,
            clbits,
        })
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

    /// Create a shuttle instruction (neutral-atom: move qubit between zones).
    pub fn shuttle(qubit: QubitId, from_zone: u32, to_zone: u32) -> Self {
        Self {
            kind: InstructionKind::Shuttle { from_zone, to_zone },
            qubits: vec![qubit],
            clbits: vec![],
        }
    }

    /// Create a noise channel instruction.
    pub fn noise_channel(model: NoiseModel, role: NoiseRole, qubit: QubitId) -> Self {
        Self {
            kind: InstructionKind::NoiseChannel { model, role },
            qubits: vec![qubit],
            clbits: vec![],
        }
    }

    /// Create a deficit noise channel (hardware noise to mitigate).
    pub fn channel_noise(model: NoiseModel, qubit: QubitId) -> Self {
        Self::noise_channel(model, NoiseRole::Deficit, qubit)
    }

    /// Create a resource noise channel (protocol noise to preserve).
    pub fn channel_resource(model: NoiseModel, qubit: QubitId) -> Self {
        Self::noise_channel(model, NoiseRole::Resource, qubit)
    }

    /// Check if this is a noise channel instruction.
    pub fn is_noise_channel(&self) -> bool {
        matches!(self.kind, InstructionKind::NoiseChannel { .. })
    }

    /// Check if this is a resource noise channel (must be preserved).
    pub fn is_noise_resource(&self) -> bool {
        matches!(
            self.kind,
            InstructionKind::NoiseChannel {
                role: NoiseRole::Resource,
                ..
            }
        )
    }

    /// Check if this is a shuttle instruction.
    pub fn is_shuttle(&self) -> bool {
        matches!(self.kind, InstructionKind::Shuttle { .. })
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
            InstructionKind::Shuttle { .. } => "shuttle",
            InstructionKind::NoiseChannel { role, .. } => match role {
                NoiseRole::Deficit => "noise_deficit",
                NoiseRole::Resource => "noise_resource",
            },
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

    #[test]
    fn test_noise_channel_instruction() {
        use crate::noise::NoiseModel;

        let inst = Instruction::channel_resource(NoiseModel::Depolarizing { p: 0.03 }, QubitId(0));
        assert!(inst.is_noise_channel());
        assert!(inst.is_noise_resource());
        assert_eq!(inst.name(), "noise_resource");
        assert_eq!(inst.qubits.len(), 1);

        let deficit =
            Instruction::channel_noise(NoiseModel::AmplitudeDamping { gamma: 0.01 }, QubitId(1));
        assert!(deficit.is_noise_channel());
        assert!(!deficit.is_noise_resource());
        assert_eq!(deficit.name(), "noise_deficit");
    }

    #[test]
    fn test_shuttle_instruction() {
        let inst = Instruction::shuttle(QubitId(0), 0, 1);
        assert!(inst.is_shuttle());
        assert_eq!(inst.name(), "shuttle");
        assert_eq!(inst.qubits.len(), 1);
        match inst.kind {
            InstructionKind::Shuttle { from_zone, to_zone } => {
                assert_eq!(from_zone, 0);
                assert_eq!(to_zone, 1);
            }
            _ => panic!("Expected Shuttle"),
        }
    }
}
