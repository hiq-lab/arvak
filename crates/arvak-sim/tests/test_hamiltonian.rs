//! Tests for Hamiltonian data structures.

use arvak_sim::hamiltonian::{Hamiltonian, HamiltonianTerm, PauliOp, PauliString};

// ---------------------------------------------------------------------------
// PauliString
// ---------------------------------------------------------------------------

#[test]
fn pauli_string_drops_identity() {
    let ps = PauliString::from_ops([(0, PauliOp::I), (1, PauliOp::Z)]);
    assert_eq!(ps.ops().len(), 1);
    assert_eq!(ps.ops()[0], (1, PauliOp::Z));
}

#[test]
fn pauli_string_sorted_by_qubit() {
    let ps = PauliString::from_ops([(3, PauliOp::X), (1, PauliOp::Z), (0, PauliOp::Y)]);
    let qubits: Vec<u32> = ps.ops().iter().map(|(q, _)| *q).collect();
    assert_eq!(qubits, vec![0, 1, 3]);
}

#[test]
fn pauli_string_identity_is_empty() {
    let ps = PauliString::from_ops([] as [(u32, PauliOp); 0]);
    assert!(ps.is_identity());
    assert_eq!(ps.max_qubit(), None);
}

#[test]
fn pauli_string_max_qubit() {
    let ps = PauliString::from_ops([(0, PauliOp::X), (5, PauliOp::Z)]);
    assert_eq!(ps.max_qubit(), Some(5));
}

#[test]
fn pauli_string_zz() {
    let ps = PauliString::zz([2u32, 0, 4]);
    let qubits: Vec<u32> = ps.ops().iter().map(|(q, _)| *q).collect();
    assert_eq!(qubits, vec![0, 2, 4]);
    assert!(ps.ops().iter().all(|(_, op)| *op == PauliOp::Z));
}

// ---------------------------------------------------------------------------
// HamiltonianTerm shorthands
// ---------------------------------------------------------------------------

#[test]
fn term_z_shorthand() {
    let t = HamiltonianTerm::z(3, -0.5);
    assert!((t.coeff - (-0.5)).abs() < 1e-15);
    assert_eq!(t.pauli.ops(), &[(3, PauliOp::Z)]);
}

#[test]
fn term_zz_shorthand() {
    let t = HamiltonianTerm::zz(0, 1, 1.0);
    assert_eq!(t.pauli.ops().len(), 2);
    assert_eq!(t.pauli.ops()[0], (0, PauliOp::Z));
    assert_eq!(t.pauli.ops()[1], (1, PauliOp::Z));
}

#[test]
fn term_x_shorthand() {
    let t = HamiltonianTerm::x(2, 0.3);
    assert_eq!(t.pauli.ops(), &[(2, PauliOp::X)]);
}

// ---------------------------------------------------------------------------
// Hamiltonian
// ---------------------------------------------------------------------------

#[test]
fn hamiltonian_lambda() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::z(0, -1.0),
        HamiltonianTerm::z(1, 0.5),
        HamiltonianTerm::zz(0, 1, -0.25),
    ]);
    // |−1.0| + |0.5| + |−0.25| = 1.75
    let lambda = h.lambda();
    assert!((lambda - 1.75).abs() < 1e-12);
}

#[test]
fn hamiltonian_min_qubits() {
    let h = Hamiltonian::from_terms(vec![
        HamiltonianTerm::z(0, 1.0),
        HamiltonianTerm::zz(2, 4, 0.5),
    ]);
    assert_eq!(h.min_qubits(), 5); // highest index is 4, so need 5 qubits
}

#[test]
fn hamiltonian_empty_min_qubits() {
    let h = Hamiltonian::from_terms(vec![]);
    assert_eq!(h.min_qubits(), 0);
}

#[test]
fn hamiltonian_from_iter() {
    let h: Hamiltonian = vec![HamiltonianTerm::z(0, 1.0), HamiltonianTerm::x(1, -0.5)]
        .into_iter()
        .collect();
    assert_eq!(h.n_terms(), 2);
}
