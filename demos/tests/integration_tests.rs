//! Integration tests for the demo suite.
//!
//! These tests verify the end-to-end functionality of the demo algorithms
//! using mock backends for reliable, reproducible testing.

use hiq_demos::circuits::grover::{grover_circuit, optimal_iterations};
use hiq_demos::circuits::qaoa::qaoa_circuit;
use hiq_demos::circuits::vqe::{num_parameters, two_local_ansatz};
use hiq_demos::problems::{
    Graph, beh2_hamiltonian, exact_ground_state_energy, h2_hamiltonian, h2o_hamiltonian,
    lih_hamiltonian,
};
use hiq_demos::runners::{QaoaRunner, VqeRunner};

/// Test that all molecular Hamiltonians have correct qubit counts.
#[test]
fn test_all_hamiltonian_qubits() {
    assert_eq!(h2_hamiltonian().num_qubits(), 2);
    assert_eq!(lih_hamiltonian().num_qubits(), 4);
    assert_eq!(beh2_hamiltonian().num_qubits(), 6);
    assert_eq!(h2o_hamiltonian().num_qubits(), 8);
}

/// Test that exact energies are defined for all molecules.
#[test]
fn test_all_exact_energies() {
    let molecules = ["h2", "lih", "beh2", "h2o"];
    for molecule in &molecules {
        let energy = exact_ground_state_energy(molecule);
        assert!(energy.is_some(), "Missing exact energy for {}", molecule);
        assert!(
            energy.unwrap() < 0.0,
            "Energy for {} should be negative",
            molecule
        );
    }
}

/// Test Grover circuit generation for various qubit counts.
#[test]
fn test_grover_circuit_scaling() {
    for n_qubits in 2..=6 {
        let iterations = optimal_iterations(n_qubits);
        let marked = (1 << n_qubits) - 1; // Last state
        let circuit = grover_circuit(n_qubits, marked, iterations);

        assert_eq!(circuit.num_qubits(), n_qubits);
        // Circuit should have gates
        assert!(circuit.dag().num_ops() > 0);
    }
}

/// Test VQE ansatz parameter counts.
#[test]
fn test_vqe_parameter_counts() {
    // TwoLocal ansatz with RY gates: 2 parameters per qubit per layer
    for n_qubits in 2..=6 {
        for reps in 1..=3 {
            let expected = n_qubits * (reps + 1);
            let actual = num_parameters("two_local", n_qubits, reps);
            assert_eq!(
                actual, expected,
                "Wrong param count for {} qubits, {} reps",
                n_qubits, reps
            );
        }
    }
}

/// Test VQE ansatz circuit generation.
#[test]
fn test_vqe_ansatz_generation() {
    let n_qubits = 4;
    let reps = 2;
    let n_params = num_parameters("two_local", n_qubits, reps);
    let params: Vec<f64> = (0..n_params).map(|i| i as f64 * 0.1).collect();

    let circuit = two_local_ansatz(n_qubits, reps, &params);

    assert_eq!(circuit.num_qubits(), n_qubits);
    assert!(circuit.dag().num_ops() > 0);
}

/// Test QAOA circuit generation for different graphs.
#[test]
fn test_qaoa_circuit_generation() {
    let graphs = [Graph::square_4(), Graph::complete_4(), Graph::ring_6()];

    for graph in &graphs {
        let layers = 2;
        let gamma: Vec<f64> = (0..layers).map(|_| 0.5).collect();
        let beta: Vec<f64> = (0..layers).map(|_| 0.5).collect();

        let circuit = qaoa_circuit(graph, &gamma, &beta);

        assert_eq!(circuit.num_qubits(), graph.n_nodes);
        assert!(circuit.dag().num_ops() > 0);
    }
}

/// Test VQE runner finds reasonable energies for H2.
#[test]
fn test_vqe_h2_convergence() {
    let h = h2_hamiltonian();
    let runner = VqeRunner::new(h).with_reps(2).with_maxiter(50);

    let result = runner.run();

    // Should find a negative energy
    assert!(result.optimal_energy < 0.0);

    // Should be within reasonable range of exact value
    let exact = exact_ground_state_energy("h2").unwrap();
    let error = (result.optimal_energy - exact).abs();
    assert!(
        error < 0.1,
        "VQE error {} too large (found {}, exact {})",
        error,
        result.optimal_energy,
        exact
    );
}

/// Test QAOA runner finds good cuts for square graph.
#[test]
fn test_qaoa_square_convergence() {
    let graph = Graph::square_4();
    let runner = QaoaRunner::new(graph).with_layers(2).with_maxiter(50);

    let result = runner.run();

    // Square graph has optimal cut of 4
    assert!(
        result.best_cut >= 3.0,
        "QAOA should find cut >= 3, got {}",
        result.best_cut
    );
    assert!(
        result.approximation_ratio > 0.7,
        "Approximation ratio {} too low",
        result.approximation_ratio
    );
}

/// Test that energy history shows convergence.
#[test]
fn test_vqe_energy_convergence_trend() {
    let h = h2_hamiltonian();
    let runner = VqeRunner::new(h).with_reps(2).with_maxiter(30);

    let result = runner.run();

    // Energy should generally decrease (with some noise)
    let history = &result.energy_history;
    assert!(history.len() > 1, "Should have energy history");

    // Final energy should be lower than initial
    let first = history.first().unwrap();
    let last = history.last().unwrap();
    assert!(
        last <= first,
        "Energy should decrease: first={}, last={}",
        first,
        last
    );
}

/// Test multiple VQE runs give consistent results.
#[test]
fn test_vqe_reproducibility() {
    let h = h2_hamiltonian();

    let runner1 = VqeRunner::new(h.clone()).with_reps(1).with_maxiter(20);
    let runner2 = VqeRunner::new(h).with_reps(1).with_maxiter(20);

    let result1 = runner1.run();
    let result2 = runner2.run();

    // Same configuration should give same results (deterministic optimizer)
    assert!(
        (result1.optimal_energy - result2.optimal_energy).abs() < 0.01,
        "Results should be reproducible"
    );
}

/// Test QAOA with different layer depths.
#[test]
fn test_qaoa_layer_scaling() {
    let graph = Graph::square_4();

    let mut prev_ratio = 0.0;
    for layers in 1..=3 {
        let runner = QaoaRunner::new(graph.clone())
            .with_layers(layers)
            .with_maxiter(30);
        let result = runner.run();

        // More layers should generally give better results
        assert!(
            result.approximation_ratio >= prev_ratio * 0.9,
            "Layer {} ratio {} should not be much worse than {}",
            layers,
            result.approximation_ratio,
            prev_ratio
        );
        prev_ratio = result.approximation_ratio;
    }
}

/// Test error mitigation configuration.
#[test]
fn test_mitigation_config() {
    use hiq_demos::runners::MitigationConfig;

    let config = MitigationConfig::full_mitigation();
    assert!(config.zne_enabled);
    assert!(config.measurement_mitigation);
    assert!(config.pauli_twirling);

    let custom = MitigationConfig::new()
        .with_zne(vec![1.0, 2.0])
        .with_measurement_mitigation(500);
    assert!(custom.zne_enabled);
    assert!(custom.measurement_mitigation);
    assert!(!custom.pauli_twirling);
}

/// Test zero noise extrapolation.
#[test]
fn test_zne_extrapolation() {
    use hiq_demos::runners::zero_noise_extrapolation;

    // Simulate noisy data with linear decay
    let values = vec![0.9, 0.8, 0.7, 0.6];
    let scales = vec![1.0, 2.0, 3.0, 4.0];

    let result = zero_noise_extrapolation(&values, &scales);

    // Should extrapolate to ~1.0 at zero noise
    assert!(
        (result.extrapolated_value - 1.0).abs() < 0.05,
        "ZNE should extrapolate to ~1.0, got {}",
        result.extrapolated_value
    );
    assert!(
        result.fit_quality > 0.99,
        "Fit should be excellent for linear data"
    );
}

/// Test measurement mitigator.
#[test]
fn test_measurement_mitigator() {
    use hiq_demos::runners::MeasurementMitigator;

    // Create mitigator with 5% error rate for 2 qubits
    let mitigator = MeasurementMitigator::from_error_rate(2, 0.05);

    // Fidelity should be high
    let fidelity = mitigator.average_fidelity();
    assert!(fidelity > 0.85, "Average fidelity {} too low", fidelity);

    // Test mitigation of noisy distribution
    // True state is |00>, but noise gives some probability to other states
    let noisy = vec![0.90, 0.04, 0.04, 0.02];
    let mitigated = mitigator.mitigate(&noisy);

    // Mitigated should boost the main peak
    assert!(
        mitigated[0] >= noisy[0],
        "Mitigation should increase main peak"
    );
}
