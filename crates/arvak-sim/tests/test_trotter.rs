//! Tests for Trotter product-formula synthesis.

use arvak_sim::SimError;
use arvak_sim::hamiltonian::{Hamiltonian, HamiltonianTerm};
use arvak_sim::trotter::TrotterEvolution;

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn empty_hamiltonian_returns_error() {
    let h = Hamiltonian::from_terms(vec![]);
    let evol = TrotterEvolution::new(h, 1.0, 1);
    assert!(matches!(
        evol.first_order(),
        Err(SimError::EmptyHamiltonian)
    ));
    let h2 = Hamiltonian::from_terms(vec![]);
    let evol2 = TrotterEvolution::new(h2, 1.0, 1);
    assert!(matches!(
        evol2.second_order(),
        Err(SimError::EmptyHamiltonian)
    ));
}

#[test]
fn zero_steps_returns_error() {
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 1.0)]);
    let evol = TrotterEvolution::new(h, 1.0, 0);
    assert!(matches!(evol.first_order(), Err(SimError::InvalidSteps(0))));
}

// ---------------------------------------------------------------------------
// Circuit structure
// ---------------------------------------------------------------------------

#[test]
fn first_order_qubit_count_inferred() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, 1.0),
        HamiltonianTerm::z(2, -0.5),
    ]);
    let evol = TrotterEvolution::new(h, 1.0, 1);
    let circuit = evol.first_order().unwrap();
    assert_eq!(circuit.num_qubits(), 3);
}

#[test]
fn second_order_qubit_count_inferred() {
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::zz(0, 3, 1.0)]);
    let evol = TrotterEvolution::new(h, 1.0, 1);
    let circuit = evol.second_order().unwrap();
    assert_eq!(circuit.num_qubits(), 4);
}

#[test]
fn with_n_qubits_overrides_inferred_width() {
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 1.0)]);
    let evol = TrotterEvolution::new(h, 1.0, 1).with_n_qubits(5);
    let circuit = evol.first_order().unwrap();
    assert_eq!(circuit.num_qubits(), 5);
}

#[test]
fn more_steps_produce_deeper_circuit() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, 1.0),
        HamiltonianTerm::z(0, -0.5),
    ]);
    let circuit_1 = TrotterEvolution::new(h.clone(), 1.0, 1)
        .first_order()
        .unwrap();
    let circuit_4 = TrotterEvolution::new(h, 1.0, 4).first_order().unwrap();
    // More steps → more gates → deeper circuit.
    assert!(circuit_4.depth() > circuit_1.depth());
}

#[test]
fn single_z_term_circuit_has_rz() {
    // H = Z₀ → exp(-i Z t) requires an Rz gate.
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(0, 1.0)]);
    let evol = TrotterEvolution::new(h, 1.0, 1);
    let circuit = evol.first_order().unwrap();
    // Circuit should have at least 1 qubit and some depth (Rz only, plus the
    // ensure_all_qubits_touched Rz(0)).
    assert_eq!(circuit.num_qubits(), 1);
    assert!(circuit.depth() >= 1);
}

#[test]
fn transverse_ising_circuit_structure() {
    // H = -ZZ - 0.5 X₀ - 0.5 X₁  (transverse-field Ising, 2 qubits)
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, -1.0),
        HamiltonianTerm::x(0, -0.5),
        HamiltonianTerm::x(1, -0.5),
    ]);
    let evol = TrotterEvolution::new(h, 1.0, 2);
    let circuit = evol.first_order().unwrap();
    assert_eq!(circuit.num_qubits(), 2);
    assert!(circuit.depth() >= 3); // at minimum: ZZ (CX+Rz+CX), X₀ (H+Rz+H), X₁
}

#[test]
fn second_order_is_deeper_than_first_order() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::zz(0, 1, 1.0),
        HamiltonianTerm::z(0, -0.5),
    ]);
    let c1 = TrotterEvolution::new(h.clone(), 1.0, 1)
        .first_order()
        .unwrap();
    let c2 = TrotterEvolution::new(h, 1.0, 1).second_order().unwrap();
    // Second order applies terms forward + backward per step → deeper.
    assert!(c2.depth() > c1.depth());
}

#[test]
fn y_operator_basis_change_compiles() {
    use arvak_sim::hamiltonian::{PauliOp, PauliString};
    // H = Y₀⊗Y₁ — exercises Sdg·H basis change.
    let h = Hamiltonian::from_terms(vec![arvak_sim::HamiltonianTerm::new(
        0.5,
        PauliString::from_ops([(0, PauliOp::Y), (1, PauliOp::Y)]),
    )]);
    let evol = TrotterEvolution::new(h, 1.0, 1);
    let circuit = evol.first_order().unwrap();
    assert_eq!(circuit.num_qubits(), 2);
}

#[test]
fn qubit_out_of_range_returns_error() {
    // Term references qubit 5 but circuit width is 2 (from with_n_qubits).
    let h = Hamiltonian::from_terms(vec![HamiltonianTerm::z(5, 1.0)]);
    let evol = TrotterEvolution::new(h, 1.0, 1).with_n_qubits(2);
    assert!(matches!(
        evol.first_order(),
        Err(SimError::QubitOutOfRange { .. })
    ));
}
