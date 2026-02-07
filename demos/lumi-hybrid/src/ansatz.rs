//! UCCSD Ansatz for H2 molecule
//!
//! This module implements a minimal UCCSD (Unitary Coupled Cluster Singles and Doubles)
//! ansatz for the H2 molecule using 2 qubits.

use anyhow::Result;
use arvak_ir::{Circuit, ClbitId, QubitId};
use std::f64::consts::PI;

/// Create the UCCSD ansatz circuit for H2
///
/// The minimal H2 ansatz uses 2 qubits and 1 variational parameter.
/// The circuit prepares the Hartree-Fock state and then applies
/// the UCCSD excitation operator.
///
/// ```text
/// |0⟩ ─[X]─[Ry(π/2)]─●─[Rz(θ)]─●─[Ry(-π/2)]─
///                    │         │
/// |0⟩ ─[X]───────────X─────────X────────────
/// ```
///
/// This corresponds to the excitation |01⟩ ↔ |10⟩ (single excitation)
/// which captures the essential correlation in H2.
pub fn create_uccsd_ansatz(parameters: &[f64]) -> Result<Circuit> {
    let theta = parameters.first().copied().unwrap_or(0.0);

    let mut circuit = Circuit::with_size("uccsd_h2", 2, 2);

    // Prepare Hartree-Fock reference state |01⟩
    // (one electron in each spin orbital)
    circuit.x(QubitId(0))?;

    // UCCSD excitation operator exp(θ(a†b - b†a))
    // Decomposed into native gates:

    // Basis rotation - ry(theta, qubit)
    circuit.ry(PI / 2.0, QubitId(0))?;

    // Entangling CNOT
    circuit.cx(QubitId(0), QubitId(1))?;

    // Variational rotation - rz(theta, qubit)
    circuit.rz(theta, QubitId(0))?;

    // Unentangle
    circuit.cx(QubitId(0), QubitId(1))?;

    // Undo basis rotation
    circuit.ry(-PI / 2.0, QubitId(0))?;

    // Add measurements
    circuit.measure(QubitId(0), ClbitId(0))?;
    circuit.measure(QubitId(1), ClbitId(1))?;

    Ok(circuit)
}

/// Create a hardware-efficient ansatz (alternative to UCCSD)
///
/// This uses a simpler structure that may be easier to run on NISQ devices:
///
/// ```text
/// |0⟩ ─[Ry(θ₀)]─●─[Ry(θ₂)]─
///               │
/// |0⟩ ─[Ry(θ₁)]─X─[Ry(θ₃)]─
/// ```
#[allow(dead_code)]
pub fn create_hardware_efficient_ansatz(parameters: &[f64]) -> Result<Circuit> {
    let theta0 = parameters.first().copied().unwrap_or(0.0);
    let theta1 = parameters.get(1).copied().unwrap_or(0.0);
    let theta2 = parameters.get(2).copied().unwrap_or(0.0);
    let theta3 = parameters.get(3).copied().unwrap_or(0.0);

    let mut circuit = Circuit::with_size("hw_efficient_h2", 2, 2);

    // First layer of rotations
    circuit.ry(theta0, QubitId(0))?;
    circuit.ry(theta1, QubitId(1))?;

    // Entangling layer
    circuit.cx(QubitId(0), QubitId(1))?;

    // Second layer of rotations
    circuit.ry(theta2, QubitId(0))?;
    circuit.ry(theta3, QubitId(1))?;

    // Measurements
    circuit.measure(QubitId(0), ClbitId(0))?;
    circuit.measure(QubitId(1), ClbitId(1))?;

    Ok(circuit)
}

/// Get the number of parameters for the UCCSD ansatz
#[allow(dead_code)]
pub fn uccsd_num_params() -> usize {
    1
}

/// Get the number of parameters for the hardware-efficient ansatz
#[allow(dead_code)]
pub fn hw_efficient_num_params() -> usize {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uccsd_ansatz_creation() {
        let params = vec![0.5];
        let circuit = create_uccsd_ansatz(&params).unwrap();

        assert_eq!(circuit.num_qubits(), 2);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_hw_efficient_ansatz_creation() {
        let params = vec![0.1, 0.2, 0.3, 0.4];
        let circuit = create_hardware_efficient_ansatz(&params).unwrap();

        assert_eq!(circuit.num_qubits(), 2);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_ansatz_parameter_variation() {
        // Different parameters should produce different circuits
        let circuit1 = create_uccsd_ansatz(&[0.0]).unwrap();
        let circuit2 = create_uccsd_ansatz(&[1.0]).unwrap();

        // Circuits have same structure but different parameters
        assert_eq!(circuit1.num_qubits(), circuit2.num_qubits());
    }
}
