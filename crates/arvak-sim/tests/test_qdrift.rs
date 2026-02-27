//! Tests for QDrift stochastic synthesis.

use rand::SeedableRng;

use arvak_sim::SimError;
use arvak_sim::hamiltonian::{Hamiltonian, HamiltonianTerm};
use arvak_sim::qdrift::QDriftEvolution;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn empty_hamiltonian_returns_error() {
    let h = Hamiltonian::from_terms(vec![]);
    let evol = QDriftEvolution::new(h, 1.0, 10);
    assert!(matches!(evol.circuit(), Err(SimError::EmptyHamiltonian)));
}

#[test]
fn zero_samples_returns_error() {
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 1.0)]);
    let evol = QDriftEvolution::new(h, 1.0, 0);
    assert!(matches!(evol.circuit(), Err(SimError::InvalidSamples(0))));
}

// ---------------------------------------------------------------------------
// Circuit structure
// ---------------------------------------------------------------------------

#[test]
fn qubit_count_inferred() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 2, 1.0),
        HamiltonianTerm::z(1, -0.5),
    ]);
    let evol = QDriftEvolution::new(h, 1.0, 20);
    let circuit = evol.circuit().unwrap();
    assert_eq!(circuit.num_qubits(), 3);
}

#[test]
fn with_n_qubits_overrides_width() {
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 1.0)]);
    let evol = QDriftEvolution::new(h, 1.0, 5).with_n_qubits(4);
    let circuit = evol.circuit().unwrap();
    assert_eq!(circuit.num_qubits(), 4);
}

#[test]
fn seeded_rng_is_reproducible() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, -1.0),
        HamiltonianTerm::x(0, -0.5),
        HamiltonianTerm::x(1, -0.5),
    ]);
    let rng1 = rand::rngs::StdRng::seed_from_u64(42);
    let rng2 = rand::rngs::StdRng::seed_from_u64(42);

    let evol1 = QDriftEvolution::new(h.clone(), 1.0, 20);
    let evol2 = QDriftEvolution::new(h, 1.0, 20);

    let c1 = evol1.circuit_with_rng(rng1).unwrap();
    let c2 = evol2.circuit_with_rng(rng2).unwrap();

    // Same seed → same depth.
    assert_eq!(c1.depth(), c2.depth());
    assert_eq!(c1.num_qubits(), c2.num_qubits());
}

#[test]
fn more_samples_produce_deeper_circuit() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, -1.0),
        HamiltonianTerm::x(0, -0.5),
    ]);
    let rng_few = rand::rngs::StdRng::seed_from_u64(7);
    let rng_many = rand::rngs::StdRng::seed_from_u64(7);

    let c_few = QDriftEvolution::new(h.clone(), 1.0, 3)
        .circuit_with_rng(rng_few)
        .unwrap();
    let c_many = QDriftEvolution::new(h, 1.0, 20)
        .circuit_with_rng(rng_many)
        .unwrap();

    assert!(c_many.depth() > c_few.depth());
}

#[test]
fn zero_lambda_produces_trivial_circuit() {
    // All coefficients zero → lambda = 0 → identity circuit.
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 0.0)]);
    let evol = QDriftEvolution::new(h, 1.0, 10);
    let circuit = evol.circuit().unwrap();
    // Should be a 1-qubit trivial circuit (no gates).
    assert_eq!(circuit.num_qubits(), 1);
}

#[test]
fn all_three_pauli_types_compile() {
    use arvak_sim::HamiltonianTerm;
    use arvak_sim::hamiltonian::{PauliOp, PauliString};

    // XYZ mixed term.
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::new(
        0.5,
        PauliString::from_ops([(0, PauliOp::X), (1, PauliOp::Y), (2, PauliOp::Z)]),
    )]);
    let rng = rand::rngs::StdRng::seed_from_u64(0);
    let evol = QDriftEvolution::new(h, 1.0, 5);
    let circuit = evol.circuit_with_rng(rng).unwrap();
    assert_eq!(circuit.num_qubits(), 3);
}
