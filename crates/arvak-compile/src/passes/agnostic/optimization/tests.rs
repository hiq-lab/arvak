//! Tests for optimization passes.

use std::f64::consts::PI;

use arvak_ir::qubit::QubitId;
use arvak_ir::Circuit;

use crate::pass::Pass;
use crate::property::PropertySet;

use super::{CancelCX, CommutativeCancellation, Optimize1qGates};

#[test]
fn test_optimize_1q_hh_cancels() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.h(QubitId(0)).unwrap(); // H·H = I
    let mut dag = circuit.into_dag();

    let mut props = PropertySet::new();
    Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

    // H·H should cancel to identity (0 gates or very small rotation)
    // Due to numerical precision, we might get 0 gates
    assert!(
        dag.num_ops() <= 1,
        "Expected 0 or 1 ops, got {}",
        dag.num_ops()
    );
}

#[test]
fn test_optimize_1q_reduces_count() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.h(QubitId(0)).unwrap();
    let mut dag = circuit.into_dag();

    let initial_ops = dag.num_ops();
    assert_eq!(initial_ops, 4);

    let mut props = PropertySet::new();
    Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

    // Should reduce 4 gates to at most 3 (RZ, RY, RZ)
    assert!(
        dag.num_ops() <= 3,
        "Expected at most 3 ops, got {}",
        dag.num_ops()
    );
}

#[test]
fn test_cancel_cx_adjacent() {
    let mut circuit = Circuit::with_size("test", 2, 0);
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap(); // CX·CX = I
    let mut dag = circuit.into_dag();

    assert_eq!(dag.num_ops(), 2);

    let mut props = PropertySet::new();
    CancelCX::new().run(&mut dag, &mut props).unwrap();

    // Should cancel both CX gates
    assert_eq!(dag.num_ops(), 0);
}

#[test]
fn test_cancel_cx_not_adjacent() {
    let mut circuit = Circuit::with_size("test", 2, 0);
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(0)).unwrap(); // Intervening gate
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    let mut dag = circuit.into_dag();

    assert_eq!(dag.num_ops(), 3);

    let mut props = PropertySet::new();
    CancelCX::new().run(&mut dag, &mut props).unwrap();

    // Should NOT cancel (H gate between them)
    assert_eq!(dag.num_ops(), 3);
}

#[test]
fn test_commutative_rz_merge() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.rz(PI / 4.0, QubitId(0)).unwrap();
    circuit.rz(PI / 4.0, QubitId(0)).unwrap();
    let mut dag = circuit.into_dag();

    assert_eq!(dag.num_ops(), 2);

    let mut props = PropertySet::new();
    CommutativeCancellation::new()
        .run(&mut dag, &mut props)
        .unwrap();

    // Should merge to single RZ(π/2)
    assert_eq!(dag.num_ops(), 1);
}

#[test]
fn test_commutative_rz_cancellation() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.rz(PI, QubitId(0)).unwrap();
    circuit.rz(-PI, QubitId(0)).unwrap();
    let mut dag = circuit.into_dag();

    assert_eq!(dag.num_ops(), 2);

    let mut props = PropertySet::new();
    CommutativeCancellation::new()
        .run(&mut dag, &mut props)
        .unwrap();

    // Should merge and normalize to near-zero, which might remove the gate
    assert!(dag.num_ops() <= 1);
}

#[test]
fn test_resource_noise_blocks_optimization() {
    use arvak_ir::noise::NoiseModel;

    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit
        .channel_resource(NoiseModel::Depolarizing { p: 0.03 }, QubitId(0))
        .unwrap();
    circuit.h(QubitId(0)).unwrap();
    let mut dag = circuit.into_dag();

    let initial_ops = dag.num_ops();
    assert_eq!(initial_ops, 3);

    let mut props = PropertySet::new();
    Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

    // H·H would normally cancel, but Resource noise channel prevents it
    assert!(
        dag.num_ops() >= 2,
        "Resource noise should block H·H cancellation"
    );
}

#[test]
fn test_zyz_decomposition_roundtrip() {
    use crate::unitary::Unitary2x2;

    let h = Unitary2x2::h();
    let (alpha, beta, gamma, phase) = h.zyz_decomposition();

    // Reconstruct
    let reconstructed = Unitary2x2::rz(alpha) * Unitary2x2::ry(beta) * Unitary2x2::rz(gamma);
    let global = num_complex::Complex64::from_polar(1.0, phase);

    for i in 0..4 {
        let expected = h.data[i];
        let got = reconstructed.data[i] * global;
        assert!(
            (expected - got).norm() < 1e-6,
            "Mismatch: expected {expected:?}, got {got:?}"
        );
    }
}
