//! Basis translation passes.

use std::f64::consts::PI;

use hiq_ir::{
    CircuitDag, Gate, GateKind, Instruction, InstructionKind, ParameterExpression, StandardGate,
};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Basis translation pass.
///
/// Translates gates to the target basis gate set.
/// Currently supports translation to:
/// - IQM basis: PRX + CZ
/// - IBM basis: RZ + SX + X + CX
pub struct BasisTranslation;

impl Pass for BasisTranslation {
    fn name(&self) -> &str {
        "BasisTranslation"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let basis_gates = properties
            .basis_gates
            .as_ref()
            .ok_or(CompileError::MissingBasisGates)?;

        // Collect nodes that need translation
        let nodes_to_translate: Vec<_> = dag
            .topological_ops()
            .filter_map(|(idx, inst)| {
                if let Some(gate) = inst.as_gate() {
                    if !is_in_basis(gate, basis_gates) {
                        return Some((idx, inst.clone()));
                    }
                }
                None
            })
            .collect();

        // Translate each gate
        for (node_idx, instruction) in nodes_to_translate {
            let replacement = translate_gate(&instruction, basis_gates)?;
            if !replacement.is_empty() {
                dag.substitute_node(node_idx, replacement)?;
            }
        }

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties.basis_gates.is_some()
    }
}

/// Check if a gate is in the target basis.
fn is_in_basis(gate: &Gate, basis: &crate::property::BasisGates) -> bool {
    basis.contains(gate.name())
}

/// Translate a gate instruction to the target basis.
fn translate_gate(
    instruction: &Instruction,
    basis: &crate::property::BasisGates,
) -> CompileResult<Vec<Instruction>> {
    let gate = match &instruction.kind {
        InstructionKind::Gate(g) => g,
        _ => return Ok(vec![instruction.clone()]),
    };

    let _qubit = instruction.qubits[0];

    // Check if it's IQM basis (PRX + CZ)
    let is_iqm = basis.contains("prx") && basis.contains("cz");
    // Check if it's IBM basis (RZ + SX + X + CX)
    let is_ibm = basis.contains("rz") && basis.contains("sx") && basis.contains("cx");

    match &gate.kind {
        GateKind::Standard(std_gate) => {
            if is_iqm {
                translate_to_iqm(std_gate, &instruction.qubits)
            } else if is_ibm {
                translate_to_ibm(std_gate, &instruction.qubits)
            } else {
                // Unknown basis, return as-is
                Ok(vec![instruction.clone()])
            }
        }
        GateKind::Custom(_) => {
            // Custom gates need to be decomposed first
            Err(CompileError::GateNotInBasis(gate.name().to_string()))
        }
    }
}

/// Translate a standard gate to IQM basis (PRX + CZ).
fn translate_to_iqm(
    gate: &StandardGate,
    qubits: &[hiq_ir::QubitId],
) -> CompileResult<Vec<Instruction>> {
    let q0 = qubits[0];

    Ok(match gate {
        // Identity - no operation needed
        StandardGate::I => vec![],

        // X = PRX(π, 0)
        StandardGate::X => vec![Instruction::single_qubit_gate(
            StandardGate::PRX(PI.into(), 0.0.into()),
            q0,
        )],

        // Y = PRX(π, π/2)
        StandardGate::Y => vec![Instruction::single_qubit_gate(
            StandardGate::PRX(PI.into(), (PI / 2.0).into()),
            q0,
        )],

        // Z = PRX(π, 0) · PRX(π, π/2) · PRX(π, 0) = phase gate
        // Simplified: Z can be implemented via virtual Z (absorbed into subsequent PRX)
        // For now, decompose as: PRX(π, π/2) · PRX(π, 0)
        StandardGate::Z => vec![
            Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), (PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), 0.0.into()), q0),
        ],

        // H = PRX(π, π/4) · PRX(π/2, -π/2)
        StandardGate::H => vec![
            Instruction::single_qubit_gate(
                StandardGate::PRX((PI / 2.0).into(), (-PI / 2.0).into()),
                q0,
            ),
            Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), (PI / 4.0).into()), q0),
        ],

        // Rx(θ) = PRX(θ, 0)
        StandardGate::Rx(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::PRX(theta.clone(), 0.0.into()),
            q0,
        )],

        // Ry(θ) = PRX(θ, π/2)
        StandardGate::Ry(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::PRX(theta.clone(), (PI / 2.0).into()),
            q0,
        )],

        // Rz(θ) = virtual Z (absorbed) or PRX decomposition
        // Rz(θ) can be commuted through PRX gates, but for correctness:
        // Rz(θ) = PRX(π, θ/2) · PRX(π, 0)
        StandardGate::Rz(theta) => {
            let half_theta = theta.clone() / ParameterExpression::constant(2.0);
            vec![
                Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), half_theta), q0),
                Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), 0.0.into()), q0),
            ]
        }

        // CX = H · CZ · H (on target)
        StandardGate::CX => {
            let q1 = qubits[1];
            // H on target
            let h_gates = translate_to_iqm(&StandardGate::H, &[q1])?;
            let mut result = h_gates.clone();
            // CZ
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            // H on target
            result.extend(h_gates);
            result
        }

        // CZ is native
        StandardGate::CZ => {
            let q1 = qubits[1];
            vec![Instruction::two_qubit_gate(StandardGate::CZ, q0, q1)]
        }

        // PRX is native
        StandardGate::PRX(theta, phi) => vec![Instruction::single_qubit_gate(
            StandardGate::PRX(theta.clone(), phi.clone()),
            q0,
        )],

        // SWAP = CZ · (H⊗H) · CZ · (H⊗H) · CZ
        StandardGate::Swap => {
            let q1 = qubits[1];
            let h0 = translate_to_iqm(&StandardGate::H, &[q0])?;
            let h1 = translate_to_iqm(&StandardGate::H, &[q1])?;
            let cz = Instruction::two_qubit_gate(StandardGate::CZ, q0, q1);

            let mut result = vec![];
            result.push(cz.clone());
            result.extend(h0.clone());
            result.extend(h1.clone());
            result.push(cz.clone());
            result.extend(h0.clone());
            result.extend(h1.clone());
            result.push(cz);
            result
        }

        // Other gates - return error for now
        other => {
            return Err(CompileError::GateNotInBasis(format!("{:?}", other)));
        }
    })
}

/// Translate a standard gate to IBM basis (RZ + SX + X + CX).
fn translate_to_ibm(
    gate: &StandardGate,
    qubits: &[hiq_ir::QubitId],
) -> CompileResult<Vec<Instruction>> {
    let q0 = qubits[0];

    Ok(match gate {
        // Identity
        StandardGate::I => vec![],

        // X is native
        StandardGate::X => vec![Instruction::single_qubit_gate(StandardGate::X, q0)],

        // Y = Rz(π) · X
        StandardGate::Y => vec![
            Instruction::single_qubit_gate(StandardGate::Rz(PI.into()), q0),
            Instruction::single_qubit_gate(StandardGate::X, q0),
        ],

        // Z = Rz(π)
        StandardGate::Z => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(PI.into()),
            q0,
        )],

        // H = Rz(π/2) · SX · Rz(π/2)
        StandardGate::H => vec![
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
        ],

        // SX is native
        StandardGate::SX => vec![Instruction::single_qubit_gate(StandardGate::SX, q0)],

        // Rx(θ) = Rz(-π/2) · SX · Rz(π/2) for θ = π/2
        // General: Rz(-π/2) · SX · Rz(θ-π/2) · SX · Rz(-π/2)
        // Simplified for now
        StandardGate::Rx(theta) => {
            // Rx(θ) = Rz(-π/2) · X · Rz(θ) · X · Rz(-π/2) for general θ
            // Or use euler decomposition
            vec![
                Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
                Instruction::single_qubit_gate(StandardGate::SX, q0),
                Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
                Instruction::single_qubit_gate(StandardGate::SX, q0),
                Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
            ]
        }

        // Ry(θ) = SX · Rz(θ) · SXdg
        StandardGate::Ry(theta) => vec![
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::SXdg, q0),
        ],

        // Rz is native
        StandardGate::Rz(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(theta.clone()),
            q0,
        )],

        // CX is native
        StandardGate::CX => {
            let q1 = qubits[1];
            vec![Instruction::two_qubit_gate(StandardGate::CX, q0, q1)]
        }

        // CZ = H · CX · H (on target)
        StandardGate::CZ => {
            let q1 = qubits[1];
            let h_gates = translate_to_ibm(&StandardGate::H, &[q1])?;
            let mut result = h_gates.clone();
            result.push(Instruction::two_qubit_gate(StandardGate::CX, q0, q1));
            result.extend(h_gates);
            result
        }

        // Other gates
        other => {
            return Err(CompileError::GateNotInBasis(format!("{:?}", other)));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::{BasisGates, CouplingMap};
    use hiq_ir::{Circuit, QubitId};

    #[test]
    fn test_iqm_translation_h() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::star(5), BasisGates::iqm());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // H decomposes to 2 PRX gates
        assert_eq!(dag.num_ops(), 2);
    }

    #[test]
    fn test_iqm_translation_cx() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::star(5), BasisGates::iqm());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // CX = H · CZ · H, where H = 2 PRX gates
        // So CX = 2 PRX + CZ + 2 PRX = 5 gates
        assert_eq!(dag.num_ops(), 5);
    }

    #[test]
    fn test_ibm_translation_h() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::ibm());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // H = Rz · SX · Rz = 3 gates
        assert_eq!(dag.num_ops(), 3);
    }
}
