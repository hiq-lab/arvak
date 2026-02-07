//! VQE (Variational Quantum Eigensolver) ansatz circuits.
//!
//! An ansatz is a parameterized quantum circuit used in variational algorithms.
//! The parameters are optimized classically to minimize the expected energy.

use arvak_ir::Circuit;
use arvak_ir::qubit::QubitId;

/// Generate a TwoLocal ansatz circuit.
///
/// The TwoLocal ansatz alternates between:
/// - Rotation layers (Ry rotations on each qubit)
/// - Entanglement layers (CZ gates between adjacent qubits)
///
/// # Arguments
/// * `n_qubits` - Number of qubits
/// * `reps` - Number of repetitions (layers)
/// * `params` - Parameter values [θ₀, θ₁, ..., θₙ]
///
/// # Returns
/// A parameterized circuit ready for execution.
///
/// # Parameters needed
/// Total parameters = n_qubits * (reps + 1)
pub fn two_local_ansatz(n_qubits: usize, reps: usize, params: &[f64]) -> Circuit {
    let expected_params = n_qubits * (reps + 1);
    assert!(
        params.len() >= expected_params,
        "Expected {} parameters, got {}",
        expected_params,
        params.len()
    );

    let mut circuit = Circuit::with_size("two_local", n_qubits as u32, 0);
    let mut param_idx = 0;

    // Initial rotation layer
    for q in 0..n_qubits {
        circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
        param_idx += 1;
    }

    // Alternating entanglement and rotation layers
    for _ in 0..reps {
        // Entanglement layer (linear connectivity with CZ)
        for q in 0..n_qubits - 1 {
            circuit
                .cz(QubitId(q as u32), QubitId((q + 1) as u32))
                .unwrap();
        }

        // Rotation layer
        for q in 0..n_qubits {
            circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
            param_idx += 1;
        }
    }

    circuit
}

/// Generate a hardware-efficient ansatz with RY-RZ rotations.
///
/// This ansatz uses RY and RZ rotations followed by CZ entanglement,
/// which is hardware-efficient for superconducting qubit systems.
///
/// # Parameters needed
/// Total parameters = 2 * n_qubits * (reps + 1)
pub fn hardware_efficient_ansatz(n_qubits: usize, reps: usize, params: &[f64]) -> Circuit {
    let expected_params = 2 * n_qubits * (reps + 1);
    assert!(
        params.len() >= expected_params,
        "Expected {} parameters, got {}",
        expected_params,
        params.len()
    );

    let mut circuit = Circuit::with_size("hw_efficient", n_qubits as u32, 0);
    let mut param_idx = 0;

    // Initial rotation layer
    for q in 0..n_qubits {
        circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
        param_idx += 1;
        circuit.rz(params[param_idx], QubitId(q as u32)).unwrap();
        param_idx += 1;
    }

    // Alternating entanglement and rotation layers
    for _ in 0..reps {
        // Entanglement layer (linear connectivity with CZ)
        for q in 0..n_qubits - 1 {
            circuit
                .cz(QubitId(q as u32), QubitId((q + 1) as u32))
                .unwrap();
        }

        // Rotation layer
        for q in 0..n_qubits {
            circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
            param_idx += 1;
            circuit.rz(params[param_idx], QubitId(q as u32)).unwrap();
            param_idx += 1;
        }
    }

    circuit
}

/// Generate a simple RY ansatz (rotation-only).
///
/// This is the simplest ansatz, consisting only of RY rotations.
/// Useful for testing and as a baseline.
///
/// # Parameters needed
/// Total parameters = n_qubits
pub fn ry_ansatz(n_qubits: usize, params: &[f64]) -> Circuit {
    assert!(
        params.len() >= n_qubits,
        "Expected {} parameters, got {}",
        n_qubits,
        params.len()
    );

    let mut circuit = Circuit::with_size("ry_ansatz", n_qubits as u32, 0);

    for (q, &param) in params.iter().enumerate().take(n_qubits) {
        circuit.ry(param, QubitId(q as u32)).unwrap();
    }

    circuit
}

/// Generate a UCCSD-inspired ansatz (simplified).
///
/// The Unitary Coupled Cluster Singles and Doubles (UCCSD) ansatz
/// is commonly used for molecular simulations in VQE.
/// This is a simplified version for demo purposes.
///
/// # Parameters needed
/// Depends on number of excitations (approximately n_qubits^2)
pub fn uccsd_like_ansatz(n_qubits: usize, params: &[f64]) -> Circuit {
    // Simplified UCCSD-like structure
    // Real UCCSD would require Trotterization of cluster operators

    let mut circuit = Circuit::with_size("uccsd_like", n_qubits as u32, 0);
    let mut param_idx = 0;

    // Hartree-Fock-like initial state (alternating |1⟩ and |0⟩)
    for q in 0..n_qubits / 2 {
        circuit.x(QubitId(q as u32)).unwrap();
    }

    // Single excitation-like rotations
    for q in 0..n_qubits {
        if param_idx < params.len() {
            circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
            param_idx += 1;
        }
    }

    // Double excitation-like entanglement
    for q in 0..n_qubits - 1 {
        circuit
            .cx(QubitId(q as u32), QubitId((q + 1) as u32))
            .unwrap();
        if param_idx < params.len() {
            circuit
                .rz(params[param_idx], QubitId((q + 1) as u32))
                .unwrap();
            param_idx += 1;
        }
        circuit
            .cx(QubitId(q as u32), QubitId((q + 1) as u32))
            .unwrap();
    }

    // Final rotation layer
    for q in 0..n_qubits {
        if param_idx < params.len() {
            circuit.ry(params[param_idx], QubitId(q as u32)).unwrap();
            param_idx += 1;
        }
    }

    circuit
}

/// Calculate the number of parameters needed for a given ansatz.
pub fn num_parameters(ansatz: &str, n_qubits: usize, reps: usize) -> usize {
    match ansatz {
        "two_local" => n_qubits * (reps + 1),
        "hardware_efficient" => 2 * n_qubits * (reps + 1),
        "ry" => n_qubits,
        "uccsd_like" => 3 * n_qubits - 1,
        _ => panic!("Unknown ansatz: {}", ansatz),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_two_local_ansatz() {
        let params = vec![PI / 4.0; 6]; // 2 qubits, 2 reps = 2 * 3 = 6 params
        let circuit = two_local_ansatz(2, 2, &params);

        assert_eq!(circuit.num_qubits(), 2);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_hardware_efficient_ansatz() {
        let params = vec![0.1; 12]; // 2 qubits, 2 reps = 2 * 2 * 3 = 12 params
        let circuit = hardware_efficient_ansatz(2, 2, &params);

        assert_eq!(circuit.num_qubits(), 2);
    }

    #[test]
    fn test_ry_ansatz() {
        let params = vec![PI / 2.0; 4];
        let circuit = ry_ansatz(4, &params);

        assert_eq!(circuit.num_qubits(), 4);
        assert_eq!(circuit.depth(), 1);
    }

    #[test]
    fn test_num_parameters() {
        assert_eq!(num_parameters("two_local", 4, 2), 12);
        assert_eq!(num_parameters("hardware_efficient", 4, 2), 24);
        assert_eq!(num_parameters("ry", 4, 0), 4);
    }
}
