//! QML (Quantum Machine Learning) classifier circuits.
//!
//! Parameterized quantum circuits for classification tasks.
//! The circuit alternates data-encoding layers (Rx rotations encoding input
//! features) with variational layers (Ry rotations + CZ entangling).

use arvak_ir::Circuit;
use arvak_ir::qubit::QubitId;

/// Generate a parameterized quantum classifier circuit.
///
/// Architecture per layer:
/// 1. Data encoding: `Rx(data[i])` on each qubit
/// 2. Variational: `Ry(weight[i])` on each qubit + CZ entangling ring
///
/// # Arguments
/// * `n_qubits` - Number of qubits
/// * `depth` - Number of repeated layers
/// * `data` - Input feature values (length = `n_qubits` * depth)
/// * `weights` - Trainable parameters (length = `n_qubits` * depth)
///
/// # Returns
/// A parameterized classifier circuit.
pub fn qml_classifier(n_qubits: usize, depth: usize, data: &[f64], weights: &[f64]) -> Circuit {
    assert!(n_qubits >= 2, "QML classifier requires at least 2 qubits");
    let expected = n_qubits * depth;
    assert!(
        data.len() >= expected,
        "Expected {} data values, got {}",
        expected,
        data.len()
    );
    assert!(
        weights.len() >= expected,
        "Expected {} weights, got {}",
        expected,
        weights.len()
    );

    let mut circuit = Circuit::with_size("qml_classifier", n_qubits as u32, 0);
    let mut data_idx = 0;
    let mut weight_idx = 0;

    for _ in 0..depth {
        // Data encoding layer: Rx rotations
        for q in 0..n_qubits {
            circuit.rx(data[data_idx], QubitId(q as u32)).unwrap();
            data_idx += 1;
        }

        // Variational layer: Ry rotations
        for q in 0..n_qubits {
            circuit.ry(weights[weight_idx], QubitId(q as u32)).unwrap();
            weight_idx += 1;
        }

        // Entangling layer: CZ ring
        for q in 0..n_qubits - 1 {
            circuit
                .cz(QubitId(q as u32), QubitId((q + 1) as u32))
                .unwrap();
        }
        if n_qubits > 2 {
            circuit
                .cz(QubitId((n_qubits - 1) as u32), QubitId(0))
                .unwrap();
        }
    }

    circuit
}

/// Calculate the number of parameters needed for a QML classifier.
///
/// Both data and weight vectors need `n_qubits * depth` values each.
pub fn num_qml_parameters(n_qubits: usize, depth: usize) -> usize {
    n_qubits * depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qml_classifier() {
        let n_qubits = 4;
        let depth = 3;
        let n_params = num_qml_parameters(n_qubits, depth);
        let data = vec![0.5; n_params];
        let weights = vec![0.1; n_params];

        let circuit = qml_classifier(n_qubits, depth, &data, &weights);

        assert_eq!(circuit.num_qubits(), 4);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_num_qml_parameters() {
        assert_eq!(num_qml_parameters(4, 3), 12);
        assert_eq!(num_qml_parameters(2, 1), 2);
        assert_eq!(num_qml_parameters(8, 5), 40);
    }
}
