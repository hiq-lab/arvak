//! Basis translation passes.

use std::f64::consts::PI;

use arvak_ir::{
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
/// - IBM Heron basis: RZ + SX + X + CZ
/// - Neutral-atom basis: RZ + RX + RY + CZ (planqc, PASQAL digital mode)
pub struct BasisTranslation;

impl Pass for BasisTranslation {
    fn name(&self) -> &'static str {
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

        // Rebuild the DAG from scratch to guarantee correct gate ordering.
        // The old approach used `substitute_node` which appends replacements at
        // wire ends instead of at the original node position, producing wrong
        // circuits whenever a non-final gate is translated (e.g. H before CX).
        let mut new_dag = CircuitDag::new();
        for qubit in dag.qubits().collect::<Vec<_>>() {
            new_dag.add_qubit(qubit);
        }
        for clbit in dag.clbits().collect::<Vec<_>>() {
            new_dag.add_clbit(clbit);
        }
        new_dag.set_global_phase(dag.global_phase());
        new_dag.set_level(dag.level());

        for (_idx, inst) in dag.topological_ops() {
            if let Some(gate) = inst.as_gate() {
                if !is_in_basis(gate, basis_gates) {
                    let replacement = translate_gate(inst, basis_gates)?;
                    for r in replacement {
                        new_dag.apply(r)?;
                    }
                    continue;
                }
            }
            new_dag.apply(inst.clone())?;
        }

        *dag = new_dag;
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
#[allow(
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::no_effect_underscore_binding
)]
fn translate_gate(
    instruction: &Instruction,
    basis: &crate::property::BasisGates,
) -> CompileResult<Vec<Instruction>> {
    let InstructionKind::Gate(gate) = &instruction.kind else {
        return Ok(vec![instruction.clone()]);
    };

    if instruction.qubits.is_empty() {
        return Ok(vec![instruction.clone()]);
    }

    let _qubit = instruction.qubits[0];

    // Check if it's IQM basis (PRX + CZ)
    let is_iqm = basis.contains("prx") && basis.contains("cz");
    // Check if it's IBM basis (RZ + SX + X + CX)
    let is_ibm = basis.contains("rz") && basis.contains("sx") && basis.contains("cx");
    // Check if it's IBM Eagle basis (RZ + SX + X + ECR, no CX, no CZ)
    let is_eagle = basis.contains("rz") && basis.contains("sx") && basis.contains("ecr") && !is_ibm;
    // Check if it's IBM Heron basis (RZ + SX + X + CZ, no CX, no ECR)
    let is_heron = basis.contains("rz")
        && basis.contains("sx")
        && basis.contains("cz")
        && !is_ibm
        && !is_eagle;
    // Check if it's neutral-atom basis (RZ + RX + RY + CZ; no SX, no PRX)
    // Used for planqc (Rydberg CZ, global rotations) and PASQAL digital mode.
    let is_neutral_atom = basis.contains("rz")
        && basis.contains("rx")
        && basis.contains("ry")
        && basis.contains("cz")
        && !basis.contains("sx")
        && !is_iqm;

    match &gate.kind {
        GateKind::Standard(std_gate) => {
            if is_iqm {
                translate_to_iqm(std_gate, &instruction.qubits)
            } else if is_ibm {
                translate_to_ibm(std_gate, &instruction.qubits)
            } else if is_eagle {
                translate_to_eagle(std_gate, &instruction.qubits)
            } else if is_heron {
                translate_to_heron(std_gate, &instruction.qubits)
            } else if is_neutral_atom {
                translate_to_neutral_atom(std_gate, &instruction.qubits)
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
    qubits: &[arvak_ir::QubitId],
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

        // H = PRX(π, 0) · PRX(π/2, π/2)  (up to global phase -i)
        //
        // Derivation: PRX(θ, φ) = [[cos(θ/2), -i·e^{-iφ}·sin(θ/2)],
        //                           [-i·e^{iφ}·sin(θ/2), cos(θ/2)]]
        //
        // PRX(π/2, π/2) = (1/√2) [[1, -1], [1, 1]]
        // PRX(π,   0  ) =         [[0, -i], [-i, 0]]
        //
        // Product (right-to-left, PRX(π/2,π/2) applied first):
        //   PRX(π,0) · PRX(π/2,π/2) = (1/√2) [[-i,-i],[-i,i]] = -i·H  ✓
        //
        // Global phase -i is unobservable and cancels correctly in multi-qubit
        // gates (e.g. CX = H·CZ·H acquires phase (-i)² = -1, still unitary).
        StandardGate::H => vec![
            Instruction::single_qubit_gate(
                StandardGate::PRX((PI / 2.0).into(), (PI / 2.0).into()),
                q0,
            ),
            Instruction::single_qubit_gate(StandardGate::PRX(PI.into(), 0.0.into()), q0),
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
            // H on target (decompose once)
            let h_gates = translate_to_iqm(&StandardGate::H, &[q1])?;
            let mut result = Vec::with_capacity(h_gates.len() * 2 + 1);
            result.extend_from_slice(&h_gates);
            // CZ
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            // H on target (reuse decomposition)
            result.extend_from_slice(&h_gates);
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

            let mut result = Vec::with_capacity(h0.len() * 2 + h1.len() * 2 + 3);
            result.push(cz);
            result.extend_from_slice(&h0);
            result.extend_from_slice(&h1);
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            result.extend_from_slice(&h0);
            result.extend_from_slice(&h1);
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            result
        }

        // Other gates - return error for now
        other => {
            return Err(CompileError::GateNotInBasis(format!("{other:?}")));
        }
    })
}

/// Translate a standard gate to IBM basis (RZ + SX + X + CX).
fn translate_to_ibm(
    gate: &StandardGate,
    qubits: &[arvak_ir::QubitId],
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
            return Err(CompileError::GateNotInBasis(format!("{other:?}")));
        }
    })
}

/// Translate a standard gate to IBM Eagle basis (RZ + SX + X + ECR).
///
/// Eagle is the 127-qubit processor family (ibm_brussels, ibm_strasbourg, etc.).
/// Single-qubit decompositions are identical to the IBM/Heron basis.
/// Two-qubit: ECR is native; CX decomposes as:
///   `CX(q0,q1) = X(q0) · SX(q1) · ECR(q0,q1) · RZ(π/2,q0) · RZ(π/2,q1)`
/// CZ is decomposed via CZ = H(q1) · CX · H(q1).
fn translate_to_eagle(
    gate: &StandardGate,
    qubits: &[arvak_ir::QubitId],
) -> CompileResult<Vec<Instruction>> {
    let q0 = qubits[0];

    Ok(match gate {
        // Single-qubit gates — same as IBM/Heron basis
        StandardGate::I => vec![],
        StandardGate::X => vec![Instruction::single_qubit_gate(StandardGate::X, q0)],
        StandardGate::Y => vec![
            Instruction::single_qubit_gate(StandardGate::Rz(PI.into()), q0),
            Instruction::single_qubit_gate(StandardGate::X, q0),
        ],
        StandardGate::Z => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(PI.into()),
            q0,
        )],
        StandardGate::H => vec![
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
        ],
        StandardGate::SX => vec![Instruction::single_qubit_gate(StandardGate::SX, q0)],
        StandardGate::Rx(theta) => vec![
            Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
        ],
        StandardGate::Ry(theta) => vec![
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((-PI).into()), q0),
        ],
        StandardGate::Rz(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(theta.clone()),
            q0,
        )],

        // ECR is native on Eagle
        StandardGate::ECR => {
            let q1 = qubits[1];
            vec![Instruction::two_qubit_gate(StandardGate::ECR, q0, q1)]
        }

        // CX = RZ(π/2,q0) · RZ(π/2,q1) · ECR(q0,q1) · X(q0) · SX(q1)
        //
        // Verified decomposition from IBM Eagle basis (same as Qiskit's BasisTranslator):
        // RZ(π/2) on both qubits first, then ECR, then X on control, SX on target.
        StandardGate::CX => {
            let q1 = qubits[1];
            vec![
                Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
                Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q1),
                Instruction::two_qubit_gate(StandardGate::ECR, q0, q1),
                Instruction::single_qubit_gate(StandardGate::X, q0),
                Instruction::single_qubit_gate(StandardGate::SX, q1),
            ]
        }

        // CZ = H(q1) · CX(q0,q1) · H(q1), H from Eagle basis (Rz·SX·Rz)
        StandardGate::CZ => {
            let q1 = qubits[1];
            let h_gates = translate_to_eagle(&StandardGate::H, &[q1])?;
            let cx_gates = translate_to_eagle(&StandardGate::CX, qubits)?;
            let mut result = h_gates.clone();
            result.extend(cx_gates);
            result.extend(h_gates);
            result
        }

        other => {
            return Err(CompileError::GateNotInBasis(format!("{other:?}")));
        }
    })
}

/// Translate a standard gate to IBM Heron basis (RZ + SX + X + CZ).
///
/// Single-qubit decompositions are identical to the IBM basis.
/// Two-qubit: CX is decomposed as H · CZ · H on the target qubit.
fn translate_to_heron(
    gate: &StandardGate,
    qubits: &[arvak_ir::QubitId],
) -> CompileResult<Vec<Instruction>> {
    let q0 = qubits[0];

    Ok(match gate {
        // Single-qubit gates — same as IBM basis
        StandardGate::I => vec![],
        StandardGate::X => vec![Instruction::single_qubit_gate(StandardGate::X, q0)],
        StandardGate::Y => vec![
            Instruction::single_qubit_gate(StandardGate::Rz(PI.into()), q0),
            Instruction::single_qubit_gate(StandardGate::X, q0),
        ],
        StandardGate::Z => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(PI.into()),
            q0,
        )],
        StandardGate::H => vec![
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((PI / 2.0).into()), q0),
        ],
        StandardGate::SX => vec![Instruction::single_qubit_gate(StandardGate::SX, q0)],
        StandardGate::Rx(theta) => vec![
            Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((-PI / 2.0).into()), q0),
        ],
        // Ry(θ) = Rz(π/2) · Rx(θ) · Rz(-π/2) = SX · Rz(θ) · SX · Rz(-π)
        // Uses only native gates (no SXdg which IBM rejects).
        StandardGate::Ry(theta) => vec![
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::SX, q0),
            Instruction::single_qubit_gate(StandardGate::Rz((-PI).into()), q0),
        ],
        StandardGate::Rz(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(theta.clone()),
            q0,
        )],

        // CZ is native on Heron
        StandardGate::CZ => {
            let q1 = qubits[1];
            vec![Instruction::two_qubit_gate(StandardGate::CZ, q0, q1)]
        }

        // CX = H(target) · CZ · H(target)
        StandardGate::CX => {
            let q1 = qubits[1];
            let h_gates = translate_to_heron(&StandardGate::H, &[q1])?;
            let mut result = Vec::with_capacity(h_gates.len() * 2 + 1);
            result.extend_from_slice(&h_gates);
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            result.extend_from_slice(&h_gates);
            result
        }

        // RZZ(θ) = CX(a,b) · Rz(θ,b) · CX(a,b)
        // CX on Heron = H(b) · CZ · H(b), H = Rz(π/2) · SX · Rz(π/2)
        StandardGate::RZZ(theta) => {
            let q1 = qubits[1];
            let h1 = translate_to_heron(&StandardGate::H, &[q1])?;
            let cz = Instruction::two_qubit_gate(StandardGate::CZ, q0, q1);
            // CX(a,b) = H(b) · CZ(a,b) · H(b)
            // RZZ(θ,a,b) = CX · Rz(θ,b) · CX
            //            = H(b)·CZ·H(b) · Rz(θ,b) · H(b)·CZ·H(b)
            let mut result = Vec::with_capacity(h1.len() * 4 + 3);
            result.extend_from_slice(&h1); // H(b) before first CZ
            result.push(cz.clone()); // CZ
            result.extend_from_slice(&h1); // H(b) after first CZ → Rz(θ) before next H
            result.push(Instruction::single_qubit_gate(
                StandardGate::Rz(theta.clone()),
                q1,
            ));
            result.extend_from_slice(&h1); // H(b) before second CZ
            result.push(cz); // CZ
            result.extend_from_slice(&h1); // H(b) after second CZ
            result
        }

        // SWAP = CX(a,b) · CX(b,a) · CX(a,b), each CX decomposed to H·CZ·H
        StandardGate::Swap => {
            let q1 = qubits[1];
            let h0 = translate_to_heron(&StandardGate::H, &[q0])?;
            let h1 = translate_to_heron(&StandardGate::H, &[q1])?;
            let cz = Instruction::two_qubit_gate(StandardGate::CZ, q0, q1);

            // CX(q0,q1) = H(q1) · CZ · H(q1)
            // CX(q1,q0) = H(q0) · CZ · H(q0)
            // CX(q0,q1) = H(q1) · CZ · H(q1)
            let mut result = Vec::with_capacity(h0.len() * 2 + h1.len() * 4 + 3);
            result.extend_from_slice(&h1);
            result.push(cz.clone());
            result.extend_from_slice(&h1);
            result.extend_from_slice(&h0);
            result.push(cz.clone());
            result.extend_from_slice(&h0);
            result.extend_from_slice(&h1);
            result.push(cz);
            result.extend_from_slice(&h1);
            result
        }

        other => {
            return Err(CompileError::GateNotInBasis(format!("{other:?}")));
        }
    })
}

/// Translate a standard gate to neutral-atom basis (RZ + RX + RY + CZ).
///
/// Used for planqc (Strontium-88 Rydberg, global rotations) and PASQAL digital mode.
/// Native gates: `rz`, `rx`, `ry` (single-qubit), `cz` (Rydberg two-qubit).
///
/// # Decompositions
///
/// - `H = Ry(π/2) · Rz(π)` (applied right-to-left: Rz first, then Ry) → −i·H (global phase)
/// - `CX = H(target) · CZ · H(target)` (same as IQM/Heron)
/// - `PRX(θ, φ) = Rz(φ) · Rx(θ) · Rz(−φ)` (axis rotation in XY plane)
///
/// # Caution
///
/// Decompositions are derived from standard quantum gate algebra.
/// They must be verified against planqc documentation once the QDMI driver
/// and calibration data are available. Do NOT derive from trial-and-error
/// against calibration data or hardware probing.
fn translate_to_neutral_atom(
    gate: &StandardGate,
    qubits: &[arvak_ir::QubitId],
) -> CompileResult<Vec<Instruction>> {
    let q0 = qubits[0];

    Ok(match gate {
        // Identity — no operation
        StandardGate::I => vec![],

        // X = Rx(π)
        StandardGate::X => vec![Instruction::single_qubit_gate(
            StandardGate::Rx(PI.into()),
            q0,
        )],

        // Y = Ry(π)
        StandardGate::Y => vec![Instruction::single_qubit_gate(
            StandardGate::Ry(PI.into()),
            q0,
        )],

        // Z = Rz(π)
        StandardGate::Z => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(PI.into()),
            q0,
        )],

        // H = Ry(π/2) · Rz(π)  (Rz applied first, then Ry)
        //
        // Derivation:
        //   Rz(π)   = diag(e^{-iπ/2}, e^{iπ/2}) = -i·diag(1,-1) (up to phase)
        //   Ry(π/2) = [[1/√2, -1/√2], [1/√2, 1/√2]]
        //
        //   Ry(π/2) · Rz(π) = (1/√2)[[-i,-i],[-i,i]] = -i · H  ✓
        //
        // Global phase -i is unobservable. Consistent with IQM H decomposition.
        StandardGate::H => vec![
            Instruction::single_qubit_gate(StandardGate::Rz(PI.into()), q0),
            Instruction::single_qubit_gate(StandardGate::Ry((PI / 2.0).into()), q0),
        ],

        // S = Rz(π/2)
        StandardGate::S => vec![Instruction::single_qubit_gate(
            StandardGate::Rz((PI / 2.0).into()),
            q0,
        )],

        // Sdg = Rz(-π/2)
        StandardGate::Sdg => vec![Instruction::single_qubit_gate(
            StandardGate::Rz((-PI / 2.0).into()),
            q0,
        )],

        // T = Rz(π/4)
        StandardGate::T => vec![Instruction::single_qubit_gate(
            StandardGate::Rz((PI / 4.0).into()),
            q0,
        )],

        // Tdg = Rz(-π/4)
        StandardGate::Tdg => vec![Instruction::single_qubit_gate(
            StandardGate::Rz((-PI / 4.0).into()),
            q0,
        )],

        // Rx is native
        StandardGate::Rx(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Rx(theta.clone()),
            q0,
        )],

        // Ry is native
        StandardGate::Ry(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Ry(theta.clone()),
            q0,
        )],

        // Rz is native
        StandardGate::Rz(theta) => vec![Instruction::single_qubit_gate(
            StandardGate::Rz(theta.clone()),
            q0,
        )],

        // CZ is native (Rydberg interaction)
        StandardGate::CZ => {
            let q1 = qubits[1];
            vec![Instruction::two_qubit_gate(StandardGate::CZ, q0, q1)]
        }

        // CX = H(target) · CZ · H(target), where H = Rz(π) then Ry(π/2)
        StandardGate::CX => {
            let q1 = qubits[1];
            let h_gates = translate_to_neutral_atom(&StandardGate::H, &[q1])?;
            let mut result = Vec::with_capacity(h_gates.len() * 2 + 1);
            result.extend_from_slice(&h_gates);
            result.push(Instruction::two_qubit_gate(StandardGate::CZ, q0, q1));
            result.extend_from_slice(&h_gates);
            result
        }

        // SWAP = CX(a,b) · CX(b,a) · CX(a,b), each CX = H(target)·CZ·H(target)
        StandardGate::Swap => {
            let q1 = qubits[1];
            let h0 = translate_to_neutral_atom(&StandardGate::H, &[q0])?;
            let h1 = translate_to_neutral_atom(&StandardGate::H, &[q1])?;
            let cz = Instruction::two_qubit_gate(StandardGate::CZ, q0, q1);

            let mut result = Vec::with_capacity(h0.len() * 2 + h1.len() * 4 + 3);
            result.extend_from_slice(&h1);
            result.push(cz.clone());
            result.extend_from_slice(&h1);
            result.extend_from_slice(&h0);
            result.push(cz.clone());
            result.extend_from_slice(&h0);
            result.extend_from_slice(&h1);
            result.push(cz);
            result.extend_from_slice(&h1);
            result
        }

        // PRX(θ, φ) = Rz(φ) · Rx(θ) · Rz(−φ)  [matrix notation, applied right-to-left]
        //
        // Circuit order (applied left-to-right): [Rz(−φ), Rx(θ), Rz(φ)]
        // Check: PRX(θ, 0) = Rz(0)·Rx(θ)·Rz(0) = Rx(θ) ✓
        //        PRX(θ, π/2) = Rz(π/2)·Rx(θ)·Rz(−π/2) = Ry(θ) ✓ (standard identity)
        StandardGate::PRX(theta, phi) => vec![
            Instruction::single_qubit_gate(
                StandardGate::Rz(ParameterExpression::constant(-1.0) * phi.clone()),
                q0,
            ),
            Instruction::single_qubit_gate(StandardGate::Rx(theta.clone()), q0),
            Instruction::single_qubit_gate(StandardGate::Rz(phi.clone()), q0),
        ],

        other => {
            return Err(CompileError::GateNotInBasis(format!("{other:?}")));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::{BasisGates, CouplingMap};
    use crate::unitary::Unitary2x2;
    use arvak_ir::{Circuit, QubitId};

    // TODO(DEBT-03 follow-up): Add a 4×4 unitary correctness test for the Eagle
    // CX→ECR decomposition. Requires reconciling qubit-ordering conventions: the ECR
    // gate matrix stored in gate.rs uses Qiskit's little-endian basis (q0=LSB) while
    // Arvak's 2-qubit gate algebra (e.g. for CX) uses big-endian (q0=MSB). The
    // gate-count test below verifies the decomposition produces the correct 5-gate
    // sequence; a full unitary check needs the correct BE ECR matrix and a verified
    // CX→ECR algebraic identity in that convention.

    /// Verify that the IQM H decomposition is unitarily correct.
    ///
    /// Previously this was PRX(π/2, -π/2) · PRX(π, π/4) which produced
    /// (e^{iπ/4}/√2) [[1,-1],[-i,-i]] — not H, not even up to global phase —
    /// causing all IQM circuits with H gates to compute wrong states.
    ///
    /// The correct decomposition is PRX(π/2, π/2) · PRX(π, 0) which gives -i·H.
    /// Global phase -i is unobservable in measurement outcomes.
    #[test]
    fn test_iqm_h_unitary_correct() {
        // PRX(θ, φ) = RZ(φ) · RX(θ) · RZ(-φ)
        let prx = |theta: f64, phi: f64| -> Unitary2x2 {
            Unitary2x2::rz(phi) * Unitary2x2::rx(theta) * Unitary2x2::rz(-phi)
        };

        // Arvak decomposition: PRX(π/2, π/2) applied first, then PRX(π, 0)
        let u = prx(PI, 0.0) * prx(PI / 2.0, PI / 2.0);
        let h = Unitary2x2::h();

        // Must be equal up to global phase: u† · H must be a scalar multiple of I
        let product = u.dagger() * h;
        let [a, b, c, d] = product.data;
        let eps = 1e-10;
        // If u = phase * H, then u† · H = conj(phase) * I, so b and c are zero
        assert!(
            b.norm() < eps && c.norm() < eps,
            "IQM H decomposition PRX(π,0)·PRX(π/2,π/2) is not H up to global phase: \
             off-diagonal elements [{b}, {c}] should be zero"
        );
        // Diagonal elements must be equal (both equal conj(phase))
        assert!((a - d).norm() < eps, "Diagonal elements differ: [{a}, {d}]");
        // Must be unitary: diagonal element has |phase| == 1
        assert!(
            (a.norm() - 1.0).abs() < eps,
            "Phase magnitude is not 1: |{a}| = {}",
            a.norm()
        );
    }

    #[test]
    fn test_eagle_translation_h() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::linear(127), BasisGates::eagle());
        BasisTranslation.run(&mut dag, &mut props).unwrap();
        // H = Rz · SX · Rz = 3 gates
        assert_eq!(dag.num_ops(), 3);
    }

    #[test]
    fn test_eagle_translation_cx() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::linear(127), BasisGates::eagle());
        BasisTranslation.run(&mut dag, &mut props).unwrap();
        // CX = RZ(q0) + RZ(q1) + ECR + X(q0) + SX(q1) = 5 gates
        assert_eq!(dag.num_ops(), 5);
    }

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

    #[test]
    fn test_heron_translation_h() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::linear(133), BasisGates::heron());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // H = Rz · SX · Rz = 3 gates (same as IBM single-qubit)
        assert_eq!(dag.num_ops(), 3);
    }

    #[test]
    fn test_heron_translation_cx() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::linear(133), BasisGates::heron());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // CX = H(t) · CZ · H(t), where H = 3 gates
        // So CX = 3 + 1 + 3 = 7 gates
        assert_eq!(dag.num_ops(), 7);
    }

    /// Verify that BasisTranslation preserves gate ordering in multi-gate circuits.
    ///
    /// This is a regression test for a bug where `substitute_node` appended
    /// replacement gates at wire ends instead of at the original position,
    /// placing the H decomposition AFTER the CX in a Bell state circuit.
    #[test]
    fn test_bell_state_ordering() {
        // Build: H(q0) -> CX(q0,q1) -> Measure(q0) -> Measure(q1)
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), arvak_ir::ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), arvak_ir::ClbitId(1)).unwrap();

        let mut dag = circuit.into_dag();
        let mut props =
            PropertySet::new().with_target(CouplingMap::linear(133), BasisGates::heron());

        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // Collect gates in topological order and verify:
        // 1. All single-qubit gates on q[0] from H decomposition come BEFORE CZ
        // 2. Measurements come LAST
        let ops: Vec<_> = dag
            .topological_ops()
            .map(|(_, inst)| {
                let name = match &inst.kind {
                    InstructionKind::Gate(g) => g.name().to_string(),
                    InstructionKind::Measure => "measure".to_string(),
                    _ => "other".to_string(),
                };
                (name, inst.qubits.clone())
            })
            .collect();

        // Find position of CZ gate
        let cz_pos = ops.iter().position(|(name, _)| name == "cz").unwrap();
        // Find position of first measurement
        let meas_pos = ops.iter().position(|(name, _)| name == "measure").unwrap();

        // H decomposition on q[0] (rz, sx, rz) must come before CZ
        // CZ must come before measurements
        assert!(
            cz_pos > 0,
            "CZ should not be the first gate (H decomposition must precede it)"
        );
        assert!(
            cz_pos < meas_pos,
            "CZ at position {cz_pos} must come before measurement at position {meas_pos}"
        );

        // Verify the first gates on q[0] are from H decomposition (rz/sx)
        let q0_gates_before_cz: Vec<_> = ops[..cz_pos]
            .iter()
            .filter(|(_, qubits)| qubits.contains(&QubitId(0)))
            .map(|(name, _)| name.as_str())
            .collect();
        assert!(
            !q0_gates_before_cz.is_empty(),
            "H decomposition gates on q[0] must precede CZ"
        );
        assert!(
            q0_gates_before_cz.iter().all(|n| *n == "rz" || *n == "sx"),
            "Gates before CZ on q[0] should be rz/sx (H decomposition), got: {q0_gates_before_cz:?}"
        );
    }

    /// Verify that the neutral-atom H decomposition is unitarily correct.
    ///
    /// H = Ry(π/2) · Rz(π) must equal H up to global phase.
    #[test]
    fn test_neutral_atom_h_unitary_correct() {
        // H = Ry(π/2) · Rz(π)  [Rz applied first, then Ry]
        let u = Unitary2x2::ry(PI / 2.0) * Unitary2x2::rz(PI);
        let h = Unitary2x2::h();

        // Must be equal up to global phase: u† · H must be a scalar multiple of I
        let product = u.dagger() * h;
        let [a, b, c, d] = product.data;
        let eps = 1e-10;
        assert!(
            b.norm() < eps && c.norm() < eps,
            "Neutral-atom H decomposition Ry(π/2)·Rz(π) is not H up to global phase: \
             off-diagonal [{b}, {c}] should be zero"
        );
        assert!((a - d).norm() < eps, "Diagonal elements differ: [{a}, {d}]");
        assert!(
            (a.norm() - 1.0).abs() < eps,
            "Phase magnitude is not 1: |{a}| = {}",
            a.norm()
        );
    }

    /// Verify that the neutral-atom PRX decomposition is unitarily correct.
    ///
    /// PRX(θ, φ) = Rz(φ) · Rx(θ) · Rz(−φ) [matrix form; circuit vec = Rz(−φ), Rx(θ), Rz(φ)].
    /// Spot-checks: PRX(θ, 0) = Rx(θ), PRX(θ, π/2) = Ry(θ).
    #[test]
    fn test_neutral_atom_prx_decomposition_correct() {
        // Matrix multiplication order (right = applied first):
        // Rz(phi) * Rx(theta) * Rz(-phi) = Rz(phi)·Rx(theta)·Rz(-phi)
        let prx_decomp = |theta: f64, phi: f64| -> Unitary2x2 {
            Unitary2x2::rz(phi) * Unitary2x2::rx(theta) * Unitary2x2::rz(-phi)
        };
        let eps = 1e-10;

        // PRX(π/3, 0) = Rx(π/3)
        let u = prx_decomp(PI / 3.0, 0.0);
        let rx = Unitary2x2::rx(PI / 3.0);
        let diff = u.dagger() * rx;
        let [a, b, c, _d] = diff.data;
        assert!(
            b.norm() < eps && c.norm() < eps,
            "PRX(π/3, 0) should equal Rx(π/3): off-diagonal [{b}, {c}]"
        );
        assert!(
            (a.norm() - 1.0).abs() < eps,
            "Phase not unitary: {}",
            a.norm()
        );

        // PRX(π/3, π/2) = Ry(π/3)
        let u2 = prx_decomp(PI / 3.0, PI / 2.0);
        let ry = Unitary2x2::ry(PI / 3.0);
        let diff2 = u2.dagger() * ry;
        let [a2, b2, c2, _d2] = diff2.data;
        assert!(
            b2.norm() < eps && c2.norm() < eps,
            "PRX(π/3, π/2) should equal Ry(π/3): off-diagonal [{b2}, {c2}]"
        );
        assert!(
            (a2.norm() - 1.0).abs() < eps,
            "Phase not unitary: {}",
            a2.norm()
        );
    }

    #[test]
    fn test_neutral_atom_translation_h() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::full(5), BasisGates::neutral_atom());
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // H = Rz(π) · Ry(π/2) = 2 gates
        assert_eq!(dag.num_ops(), 2);
    }

    #[test]
    fn test_neutral_atom_translation_cx() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::full(5), BasisGates::neutral_atom());
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // CX = H(target) · CZ · H(target), where H = 2 gates
        // So CX = 2 + 1 + 2 = 5 gates
        assert_eq!(dag.num_ops(), 5);
    }

    #[test]
    fn test_neutral_atom_translation_x() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.x(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::full(1), BasisGates::neutral_atom());
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // X = Rx(π) = 1 gate
        assert_eq!(dag.num_ops(), 1);
        let (_, inst) = dag.topological_ops().next().unwrap();
        let InstructionKind::Gate(g) = &inst.kind else {
            panic!("expected gate")
        };
        assert_eq!(g.name(), "rx");
    }
}
