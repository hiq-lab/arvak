//! Integration tests for measurement safety across optimization passes.
//!
//! These tests verify that no optimization pass incorrectly transforms
//! gates across measurement boundaries. This is the key correctness
//! property for quantum compilation with mid-circuit measurements.

use std::f64::consts::PI;

use arvak_compile::passes::{
    CancelCX, CommutativeCancellation, MeasurementBarrierVerification, Optimize1qGates,
    VerificationResult,
};
use arvak_compile::{Pass, PassManagerBuilder, PropertySet};
use arvak_ir::{Circuit, CircuitDag, ClbitId, QubitId};

/// Helper: count operations of a given kind in a DAG.
fn count_ops(dag: &CircuitDag, kind: &str) -> usize {
    dag.topological_ops()
        .filter(|(_, inst)| inst.name() == kind)
        .count()
}

/// Helper: count total gate operations in a DAG.
fn count_gates(dag: &CircuitDag) -> usize {
    dag.topological_ops()
        .filter(|(_, inst)| inst.is_gate())
        .count()
}

/// Helper: count measurements in a DAG.
fn count_measurements(dag: &CircuitDag) -> usize {
    dag.topological_ops()
        .filter(|(_, inst)| inst.is_measure())
        .count()
}

/// Helper: collect operation names in topological order for a specific qubit.
fn ops_on_qubit(dag: &CircuitDag, qubit: QubitId) -> Vec<String> {
    dag.topological_ops()
        .filter(|(_, inst)| inst.qubits.contains(&qubit))
        .map(|(_, inst)| inst.name().to_string())
        .collect()
}

// ============================================================================
// Test 1: H-Measure-H must NOT be optimized to identity
// ============================================================================

#[test]
fn test_h_measure_h_not_optimized() {
    let mut circuit = Circuit::with_size("test", 1, 1);
    circuit.h(QubitId(0)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.h(QubitId(0)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    // Run the 1q optimizer
    let pass = Optimize1qGates::new();
    pass.run(&mut dag, &mut props).unwrap();

    // Both H gates and the measurement must survive
    let ops = ops_on_qubit(&dag, QubitId(0));
    assert!(
        ops.contains(&"measure".to_string()),
        "Measurement must survive optimization"
    );
    // There should be gate(s) both before AND after the measurement
    let meas_idx = ops.iter().position(|op| op == "measure").unwrap();
    assert!(meas_idx > 0, "There should be gates before the measurement");
    assert!(
        meas_idx < ops.len() - 1,
        "There should be gates after the measurement"
    );
}

// ============================================================================
// Test 2: CX-Measure-CX must NOT be cancelled
// ============================================================================

#[test]
fn test_cx_measure_cx_not_cancelled() {
    let mut circuit = Circuit::with_size("test", 2, 1);
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    // Run CX cancellation
    let pass = CancelCX;
    pass.run(&mut dag, &mut props).unwrap();

    // Both CX gates must survive — measurement separates them
    assert_eq!(
        count_ops(&dag, "cx"),
        2,
        "Both CX gates must survive when separated by measurement"
    );
    assert_eq!(count_measurements(&dag), 1);
}

// ============================================================================
// Test 3: Rz(pi)-Measure-Rz(-pi) must NOT be merged
// ============================================================================

#[test]
fn test_rz_measure_rz_not_merged() {
    let mut circuit = Circuit::with_size("test", 1, 1);
    circuit.rz(PI, QubitId(0)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.rz(-PI, QubitId(0)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    // Run commutative cancellation
    let pass = CommutativeCancellation;
    pass.run(&mut dag, &mut props).unwrap();

    // Both Rz gates must survive — measurement separates them
    let ops = ops_on_qubit(&dag, QubitId(0));
    let gate_count = ops.iter().filter(|op| *op != "measure").count();
    assert!(
        gate_count >= 2,
        "Both Rz gates should survive when separated by measurement, got ops: {ops:?}"
    );
    assert_eq!(count_measurements(&dag), 1);
}

// ============================================================================
// Test 4: Full pipeline with mid-circuit measurement
// ============================================================================

#[test]
fn test_full_pipeline_mid_circuit_measurement() {
    let mut circuit = Circuit::with_size("test", 2, 1);
    // Pre-measurement block
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    // Mid-circuit measurement
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    // Post-measurement block
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();

    let mut dag = circuit.into_dag();

    // Build a full pipeline at opt level 2
    let (pm, mut props) = PassManagerBuilder::new().with_optimization_level(2).build();

    pm.run(&mut dag, &mut props).unwrap();

    // Measurement must still be present
    assert_eq!(
        count_measurements(&dag),
        1,
        "Mid-circuit measurement must survive full pipeline"
    );

    // There should be gates on both sides of the measurement
    let ops = ops_on_qubit(&dag, QubitId(0));
    let meas_idx = ops.iter().position(|op| op == "measure");
    assert!(meas_idx.is_some(), "Measurement must be in qubit 0 ops");
    let meas_idx = meas_idx.unwrap();
    assert!(
        meas_idx > 0,
        "Gates must exist before measurement on qubit 0"
    );
    assert!(
        meas_idx < ops.len() - 1,
        "Gates must exist after measurement on qubit 0"
    );

    // Verification pass should have passed
    let result = props.get::<VerificationResult>();
    assert!(result.is_some(), "Verification result should be stored");
    assert!(result.unwrap().passed, "Verification should pass");
}

// ============================================================================
// Test 5: H-Measure-Reset-H pattern
// ============================================================================

#[test]
fn test_measure_reset_h_not_merged_with_pre_measurement() {
    let mut circuit = Circuit::with_size("test", 1, 1);
    circuit.h(QubitId(0)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.reset(QubitId(0)).unwrap();
    circuit.h(QubitId(0)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    // Run the 1q optimizer
    let pass = Optimize1qGates::new();
    pass.run(&mut dag, &mut props).unwrap();

    // Measurement and reset must survive
    assert_eq!(count_measurements(&dag), 1);
    let ops = ops_on_qubit(&dag, QubitId(0));
    assert!(
        ops.contains(&"reset".to_string()),
        "Reset must survive, got: {ops:?}"
    );
    assert!(
        ops.contains(&"measure".to_string()),
        "Measurement must survive, got: {ops:?}"
    );
}

// ============================================================================
// Test 6: Multi-qubit measurement with subsequent gates
// ============================================================================

#[test]
fn test_multi_qubit_measurement_gates_survive() {
    let mut circuit = Circuit::with_size("test", 3, 3);
    // Create entangled state
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();

    // Measure all
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.measure(QubitId(1), ClbitId(1)).unwrap();
    circuit.measure(QubitId(2), ClbitId(2)).unwrap();

    let measurements_before = {
        let dag_ref = circuit.dag();
        count_measurements(dag_ref)
    };

    let mut dag = circuit.into_dag();
    let (pm, mut props) = PassManagerBuilder::new().with_optimization_level(2).build();

    pm.run(&mut dag, &mut props).unwrap();

    // All measurements must survive
    assert_eq!(
        count_measurements(&dag),
        measurements_before,
        "All measurements must survive optimization"
    );
}

// ============================================================================
// Test 7: Adjacent H-H without measurement CAN be cancelled (positive test)
// ============================================================================

#[test]
fn test_adjacent_hh_is_cancelled() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.h(QubitId(0)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    let pass = Optimize1qGates::new();
    pass.run(&mut dag, &mut props).unwrap();

    // Two adjacent H gates should be cancelled (H*H = I)
    let gate_count = count_gates(&dag);
    assert!(
        gate_count < 2,
        "Adjacent H-H should be optimized, got {gate_count} gates"
    );
}

// ============================================================================
// Test 8: Adjacent CX-CX without measurement CAN be cancelled (positive test)
// ============================================================================

#[test]
fn test_adjacent_cx_cx_is_cancelled() {
    let mut circuit = Circuit::with_size("test", 2, 0);
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    let pass = CancelCX;
    pass.run(&mut dag, &mut props).unwrap();

    assert_eq!(
        count_ops(&dag, "cx"),
        0,
        "Adjacent CX-CX should cancel to identity"
    );
}

// ============================================================================
// Test 9: Verification pass detects correct measurement count
// ============================================================================

#[test]
fn test_verification_pass_counts_measurements() {
    let mut circuit = Circuit::with_size("test", 3, 3);
    circuit.h(QubitId(0)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.measure(QubitId(1), ClbitId(1)).unwrap();
    circuit.h(QubitId(2)).unwrap();
    circuit.measure(QubitId(2), ClbitId(2)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    let pass = MeasurementBarrierVerification;
    pass.run(&mut dag, &mut props).unwrap();

    let result = props.get::<VerificationResult>().unwrap();
    assert!(result.passed);
    assert_eq!(result.measurements_found, 3);
    assert_eq!(result.qubits_checked, 3);
}

// ============================================================================
// Test 10: Barrier blocks optimization (positive test)
// ============================================================================

#[test]
fn test_barrier_blocks_optimization() {
    let mut circuit = Circuit::with_size("test", 1, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.barrier([QubitId(0)]).unwrap();
    circuit.h(QubitId(0)).unwrap();

    let mut dag = circuit.into_dag();
    let mut props = PropertySet::new();

    let pass = Optimize1qGates::new();
    pass.run(&mut dag, &mut props).unwrap();

    // Barrier should prevent H-H cancellation
    let ops = ops_on_qubit(&dag, QubitId(0));
    assert!(
        ops.contains(&"barrier".to_string()),
        "Barrier must survive, got: {ops:?}"
    );
}
