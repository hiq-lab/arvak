//! Gate inversion utilities.

use arvak_ir::gate::{GateKind, StandardGate};
use arvak_ir::instruction::{Instruction, InstructionKind};
use arvak_ir::parameter::ParameterExpression;

use crate::error::{UncomputeError, UncomputeResult};

/// Strategy for computing gate inverses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InverseStrategy {
    /// Compute exact algebraic inverse.
    #[default]
    Exact,
    /// Use approximate inverse (may introduce small errors).
    Approximate,
    /// Use the gate's adjoint (conjugate transpose).
    Adjoint,
}

/// Compute the inverse of a standard gate.
///
/// For unitary gates U, this returns U† (U-dagger).
///
/// # Examples
///
/// - H† = H (Hadamard is self-inverse)
/// - X† = X (Pauli gates are self-inverse)
/// - S† = Sdg
/// - T† = Tdg
/// - Rx(θ)† = Rx(-θ)
pub fn inverse_gate(gate: &StandardGate) -> UncomputeResult<StandardGate> {
    match gate {
        // Self-inverse gates (Hermitian)
        StandardGate::I => Ok(StandardGate::I),
        StandardGate::X => Ok(StandardGate::X),
        StandardGate::Y => Ok(StandardGate::Y),
        StandardGate::Z => Ok(StandardGate::Z),
        StandardGate::H => Ok(StandardGate::H),
        StandardGate::CX => Ok(StandardGate::CX),
        StandardGate::CY => Ok(StandardGate::CY),
        StandardGate::CZ => Ok(StandardGate::CZ),
        StandardGate::Swap => Ok(StandardGate::Swap),
        StandardGate::CCX => Ok(StandardGate::CCX),
        StandardGate::CSwap => Ok(StandardGate::CSwap),

        // S and T gates
        StandardGate::S => Ok(StandardGate::Sdg),
        StandardGate::Sdg => Ok(StandardGate::S),
        StandardGate::T => Ok(StandardGate::Tdg),
        StandardGate::Tdg => Ok(StandardGate::T),

        // SX gates
        StandardGate::SX => Ok(StandardGate::SXdg),
        StandardGate::SXdg => Ok(StandardGate::SX),

        // Rotation gates: negate the angle
        StandardGate::Rx(theta) => Ok(StandardGate::Rx(negate_param(theta))),
        StandardGate::Ry(theta) => Ok(StandardGate::Ry(negate_param(theta))),
        StandardGate::Rz(theta) => Ok(StandardGate::Rz(negate_param(theta))),
        StandardGate::P(lambda) => Ok(StandardGate::P(negate_param(lambda))),

        // U gate: U(θ, φ, λ)† = U(-θ, -λ, -φ)
        StandardGate::U(theta, phi, lambda) => Ok(StandardGate::U(
            negate_param(theta),
            negate_param(lambda),
            negate_param(phi),
        )),

        // Controlled rotations: negate the angle
        StandardGate::CRx(theta) => Ok(StandardGate::CRx(negate_param(theta))),
        StandardGate::CRy(theta) => Ok(StandardGate::CRy(negate_param(theta))),
        StandardGate::CRz(theta) => Ok(StandardGate::CRz(negate_param(theta))),
        StandardGate::CP(lambda) => Ok(StandardGate::CP(negate_param(lambda))),

        // Two-qubit rotations: negate the angle
        StandardGate::RXX(theta) => Ok(StandardGate::RXX(negate_param(theta))),
        StandardGate::RYY(theta) => Ok(StandardGate::RYY(negate_param(theta))),
        StandardGate::RZZ(theta) => Ok(StandardGate::RZZ(negate_param(theta))),

        // iSWAP is not self-inverse: iSWAP† ≠ iSWAP.
        // iSWAP† has conjugated off-diagonal elements (−i instead of i).
        // A full inverse requires decomposition into basis gates.
        // TODO: implement ISwap† decomposition (e.g., two CX + Rz sequence).
        StandardGate::ISwap => Err(UncomputeError::InversionNotImplemented("iswap".into())),

        // CH (controlled-Hadamard) is self-inverse
        StandardGate::CH => Ok(StandardGate::CH),

        // PRX gate: PRX(θ, φ)† = PRX(-θ, φ)
        StandardGate::PRX(theta, phi) => Ok(StandardGate::PRX(negate_param(theta), phi.clone())),

        // ECR is self-inverse: ECR† = ECR (Hermitian unitary).
        StandardGate::ECR => Ok(StandardGate::ECR),
    }
}

/// Negate a parameter expression.
fn negate_param(param: &ParameterExpression) -> ParameterExpression {
    -param.clone()
}

/// Compute the inverse of an instruction.
///
/// For gate instructions, this inverts the gate.
/// For non-unitary operations (measure, reset), this returns an error.
pub fn inverse_instruction(instruction: &Instruction) -> UncomputeResult<Instruction> {
    match &instruction.kind {
        InstructionKind::Gate(gate) => {
            let inverse_gate_kind = match &gate.kind {
                GateKind::Standard(std_gate) => GateKind::Standard(inverse_gate(std_gate)?),
                GateKind::Custom(custom) => {
                    // Custom gates cannot be automatically inverted without their definition
                    return Err(UncomputeError::NonInvertibleGate(custom.name.clone()));
                }
            };

            Ok(Instruction {
                kind: InstructionKind::Gate(arvak_ir::gate::Gate {
                    kind: inverse_gate_kind,
                    label: gate.label.clone(),
                    condition: gate.condition.clone(),
                }),
                qubits: instruction.qubits.clone(),
                clbits: instruction.clbits.clone(),
            })
        }

        InstructionKind::Measure => Err(UncomputeError::NonUnitaryOperation("measure".into())),

        InstructionKind::Reset => Err(UncomputeError::NonUnitaryOperation("reset".into())),

        InstructionKind::Barrier => {
            // Barriers don't need inversion - they're just markers
            Ok(instruction.clone())
        }

        InstructionKind::Delay { .. } => {
            // Delays don't need inversion
            Ok(instruction.clone())
        }

        InstructionKind::Shuttle { from_zone, to_zone } => {
            // Inverse of a shuttle is shuttling back
            Ok(Instruction {
                kind: InstructionKind::Shuttle {
                    from_zone: *to_zone,
                    to_zone: *from_zone,
                },
                qubits: instruction.qubits.clone(),
                clbits: instruction.clbits.clone(),
            })
        }

        InstructionKind::NoiseChannel { .. } => {
            Err(UncomputeError::NonUnitaryOperation("noise_channel".into()))
        }
    }
}

/// Check if a gate is self-inverse (Hermitian).
pub fn is_self_inverse(gate: &StandardGate) -> bool {
    matches!(
        gate,
        StandardGate::I
            | StandardGate::X
            | StandardGate::Y
            | StandardGate::Z
            | StandardGate::H
            | StandardGate::CX
            | StandardGate::CY
            | StandardGate::CZ
            | StandardGate::Swap
            | StandardGate::CCX
            | StandardGate::CSwap
            | StandardGate::CH
            | StandardGate::ECR
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_self_inverse_gates() {
        assert!(is_self_inverse(&StandardGate::H));
        assert!(is_self_inverse(&StandardGate::X));
        assert!(is_self_inverse(&StandardGate::CX));

        assert!(!is_self_inverse(&StandardGate::S));
        assert!(!is_self_inverse(&StandardGate::T));
    }

    #[test]
    fn test_inverse_h() {
        let h = StandardGate::H;
        let h_inv = inverse_gate(&h).unwrap();

        // H is self-inverse
        assert_eq!(h_inv, StandardGate::H);
    }

    #[test]
    fn test_inverse_s() {
        let s = StandardGate::S;
        let s_inv = inverse_gate(&s).unwrap();

        assert_eq!(s_inv, StandardGate::Sdg);
    }

    #[test]
    fn test_inverse_t() {
        let t = StandardGate::T;
        let t_inv = inverse_gate(&t).unwrap();

        assert_eq!(t_inv, StandardGate::Tdg);
    }

    #[test]
    fn test_inverse_rx() {
        let rx = StandardGate::Rx(ParameterExpression::constant(PI / 4.0));
        let rx_inv = inverse_gate(&rx).unwrap();

        if let StandardGate::Rx(param) = rx_inv {
            // Should be -π/4
            assert!((param.as_f64().unwrap() + PI / 4.0).abs() < 1e-10);
        } else {
            panic!("Expected Rx gate");
        }
    }

    #[test]
    fn test_inverse_instruction() {
        let inst = Instruction::single_qubit_gate(StandardGate::S, arvak_ir::qubit::QubitId(0));
        let inv = inverse_instruction(&inst).unwrap();

        if let InstructionKind::Gate(gate) = &inv.kind {
            if let GateKind::Standard(std_gate) = &gate.kind {
                assert_eq!(*std_gate, StandardGate::Sdg);
            } else {
                panic!("Expected standard gate");
            }
        } else {
            panic!("Expected gate instruction");
        }
    }

    #[test]
    fn test_measure_not_invertible() {
        let inst = Instruction::measure(arvak_ir::qubit::QubitId(0), arvak_ir::qubit::ClbitId(0));
        let result = inverse_instruction(&inst);

        assert!(result.is_err());
    }
}
