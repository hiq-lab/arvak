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

// ============================================================================
// MQT-inspired: ConsolidateBlocks at level 3 respects measurements
// (MQT test_collect_blocks: interruptBlock applied to full pipeline)
// ============================================================================

#[test]
fn test_consolidate_blocks_respects_measurement_in_pipeline() {
    // Run at optimization level 3 (which enables ConsolidateBlocks).
    // Measurement between two-qubit gates must survive.
    let mut circuit = Circuit::with_size("test", 2, 1);
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.measure(QubitId(0), ClbitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();

    let mut dag = circuit.into_dag();
    let (pm, mut props) = PassManagerBuilder::new().with_optimization_level(3).build();

    pm.run(&mut dag, &mut props).unwrap();

    // Measurement must survive at opt level 3.
    assert_eq!(
        count_measurements(&dag),
        1,
        "Measurement must survive optimization level 3 (ConsolidateBlocks)"
    );
}

// ============================================================================
// MQT-inspired: routing preserves circuit semantics (unitary equivalence)
// (MQT test_dd_functionality: CircuitEquivalence)
// ============================================================================

#[test]
fn test_routing_preserves_bell_state_semantics() {
    use arvak_compile::passes::{SabreRouting, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::CouplingMap;

    // Build a Bell state circuit.
    let mut circuit = Circuit::with_size("test", 2, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    let mut dag = circuit.into_dag();

    // Take a snapshot before routing.
    let verify = VerifyCompilation::snapshot(&dag);

    // Run layout + routing on linear(5).
    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(5));
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    // VerifyCompilation returns Ok(()) on success, Err on mismatch.
    verify
        .run(&mut dag, &mut props)
        .expect("Bell state semantics must be preserved after routing");
}

#[test]
fn test_routing_preserves_ghz_semantics() {
    use arvak_compile::passes::{SabreRouting, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::CouplingMap;

    // GHZ(4): H(0), CX(0,1), CX(1,2), CX(2,3).
    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.cx(QubitId(2), QubitId(3)).unwrap();
    let mut dag = circuit.into_dag();

    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::star(5));
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("GHZ semantics must be preserved after routing on star topology");
}

// ============================================================================
// MQT-inspired: grid topology routing
// (MQT test_heuristic: tests on IBM QX4/Casablanca/London topologies)
// ============================================================================

#[test]
fn test_sabre_grid_topology() {
    use arvak_compile::passes::{SabreRouting, TrivialLayout};
    use arvak_compile::property::CouplingMap;

    // 2×3 grid topology (6 qubits):
    //  0 - 1 - 2
    //  |   |   |
    //  3 - 4 - 5
    let grid =
        CouplingMap::from_edge_list(6, &[(0, 1), (1, 2), (3, 4), (4, 5), (0, 3), (1, 4), (2, 5)]);

    // Circuit with distant qubits: CX(0,5) requires routing.
    let mut circuit = Circuit::with_size("test", 6, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(5)).unwrap(); // distance 2 on grid
    let mut dag = circuit.into_dag();

    let mut props = PropertySet::new();
    props.coupling_map = Some(grid);
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    // Should have inserted SWAPs (distance > 1).
    assert!(
        dag.num_ops() > 2,
        "Grid routing should insert SWAPs for distant qubits, got {} ops",
        dag.num_ops()
    );
}

#[test]
fn test_sabre_grid_topology_preserves_semantics() {
    use arvak_compile::passes::{SabreRouting, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::CouplingMap;

    // 2×2 grid (4 qubits):
    //  0 - 1
    //  |   |
    //  2 - 3
    let grid = CouplingMap::from_edge_list(4, &[(0, 1), (1, 3), (2, 3), (0, 2)]);

    // CX(0,3) — diagonal on grid, distance 2.
    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(3)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();

    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(grid);
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Grid routing must preserve circuit semantics");
}

// ============================================================================
// MQT-inspired: full pipeline with algorithm circuit (QFT)
// (MQT test/algorithms/test_qft.cpp: QFT through compilation)
// ============================================================================

// ============================================================================
// MQT-inspired: full pipeline with multi-CX algorithm circuit
// (MQT test/algorithms/test_qft.cpp: algorithm circuits through compilation)
// Uses GHZ instead of QFT since QFT's CP gate isn't in the basis translator.
// ============================================================================

#[test]
fn test_step1_dense_sabre_preserves_semantics() {
    use arvak_compile::passes::{DenseLayout, SabreRouting, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    // Step 1: DenseLayout + SabreRouting only (no BasisTranslation, no Optimize).
    let circuit = Circuit::ghz(5).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(5));
    props.basis_gates = Some(BasisGates::ibm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Step 1: DenseLayout + SabreRouting must preserve GHZ(5) semantics");
}

#[test]
fn test_step2_dense_sabre_basis_preserves_semantics() {
    use arvak_compile::passes::{BasisTranslation, DenseLayout, SabreRouting, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    // Step 2: DenseLayout + SabreRouting + BasisTranslation (no Optimize).
    let circuit = Circuit::ghz(5).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(5));
    props.basis_gates = Some(BasisGates::ibm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Step 2: DenseLayout + SabreRouting + BasisTranslation must preserve semantics");
}

#[test]
fn test_step3_dense_sabre_basis_opt_preserves_semantics() {
    use arvak_compile::passes::{
        BasisTranslation, DenseLayout, OneQubitBasis, Optimize1qGates, SabreRouting,
        VerifyCompilation,
    };
    use arvak_compile::property::{BasisGates, CouplingMap};

    // Step 3: DenseLayout + SabreRouting + BasisTranslation + Optimize1qGates(ZSX).
    let circuit = Circuit::ghz(5).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(5));
    props.basis_gates = Some(BasisGates::ibm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();
    Optimize1qGates::with_basis(OneQubitBasis::ZSX)
        .run(&mut dag, &mut props)
        .unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Step 3: Full pipeline must preserve semantics");
}

#[test]
fn test_full_pipeline_ghz5_linear_preserves_semantics() {
    use arvak_compile::passes::VerifyCompilation;
    use arvak_compile::property::{BasisGates, CouplingMap};

    // GHZ(5) on linear(5): H(0), CX(0,1), CX(1,2), CX(2,3), CX(3,4).
    let circuit = Circuit::ghz(5).unwrap();
    let mut dag = circuit.into_dag();

    let verify = VerifyCompilation::snapshot(&dag);

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(2)
        .with_target(CouplingMap::linear(5), BasisGates::ibm())
        .build();

    pm.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("GHZ(5) semantics must be preserved after full compilation on linear topology");
}

#[test]
fn test_step_star_dense_sabre() {
    use arvak_compile::passes::{DenseLayout, SabreRouting, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.cx(QubitId(2), QubitId(3)).unwrap();
    circuit.h(QubitId(3)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::star(5));
    props.basis_gates = Some(BasisGates::iqm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Star: DenseLayout + SabreRouting must preserve semantics");
}

#[test]
fn test_step_star_trivial_sabre_basis() {
    use arvak_compile::passes::{BasisTranslation, SabreRouting, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.cx(QubitId(2), QubitId(3)).unwrap();
    circuit.h(QubitId(3)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::star(5));
    props.basis_gates = Some(BasisGates::iqm());
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Star: TrivialLayout + SabreRouting + BasisTranslation must preserve semantics");
}

#[test]
fn test_step_star_dense_sabre_basis() {
    use arvak_compile::passes::{BasisTranslation, DenseLayout, SabreRouting, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.cx(QubitId(2), QubitId(3)).unwrap();
    circuit.h(QubitId(3)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::star(5));
    props.basis_gates = Some(BasisGates::iqm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Star: DenseLayout + SabreRouting + BasisTranslation must preserve semantics");
}

#[test]
fn test_full_pipeline_entangled_star_preserves_semantics() {
    use arvak_compile::passes::VerifyCompilation;
    use arvak_compile::property::{BasisGates, CouplingMap};

    // Entangled circuit on star topology (requires SWAPs).
    let mut circuit = Circuit::with_size("test", 4, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.cx(QubitId(2), QubitId(3)).unwrap();
    circuit.h(QubitId(3)).unwrap();
    let mut dag = circuit.into_dag();

    let verify = VerifyCompilation::snapshot(&dag);

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(2)
        .with_target(CouplingMap::star(5), BasisGates::iqm())
        .build();

    pm.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Entangled circuit semantics must be preserved on star topology");
}

// ============================================================================
// MQT-inspired: full pipeline level 3 (ConsolidateBlocks + routing + translation)
// (MQT: end-to-end optimization correctness across all passes)
// ============================================================================

#[test]
fn test_step_consolidate_only() {
    // Just ConsolidateBlocks — no routing, no basis translation.
    use arvak_compile::passes::{ConsolidateBlocks, VerifyCompilation};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("ConsolidateBlocks alone must preserve semantics");
}

#[test]
fn test_step_consolidate_then_basis() {
    // ConsolidateBlocks + BasisTranslation (no routing).
    use arvak_compile::passes::{BasisTranslation, ConsolidateBlocks, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("ConsolidateBlocks + BasisTranslation must preserve semantics");
}

#[test]
fn test_step_consolidate_after_trivial_layout_no_route() {
    // Just TrivialLayout (sets layout) + ConsolidateBlocks (no actual routing).
    use arvak_compile::passes::{ConsolidateBlocks, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    // Skip routing — just consolidate directly.
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("TrivialLayout + ConsolidateBlocks (no route) must preserve semantics");
}

#[test]
fn test_step_level3_trivial_sabre_no_consolidate() {
    // TrivialLayout + SabreRouting only (no ConsolidateBlocks).
    use arvak_compile::passes::{SabreRouting, TrivialLayout, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("TrivialLayout + SabreRouting (no consolidate) must preserve semantics");
}

#[test]
fn test_step_level3_trivial_sabre_consolidate() {
    use arvak_compile::passes::{
        ConsolidateBlocks, SabreRouting, TrivialLayout, VerifyCompilation,
    };
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    TrivialLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("TrivialLayout + SabreRouting + ConsolidateBlocks must preserve semantics");
}

#[test]
fn test_step_level3_dense_sabre_consolidate() {
    use arvak_compile::passes::{ConsolidateBlocks, DenseLayout, SabreRouting, VerifyCompilation};
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("DenseLayout + SabreRouting + ConsolidateBlocks must preserve semantics");
}

#[test]
fn test_step_level3_dense_sabre_consolidate_basis() {
    use arvak_compile::passes::{
        BasisTranslation, ConsolidateBlocks, DenseLayout, SabreRouting, VerifyCompilation,
    };
    use arvak_compile::property::{BasisGates, CouplingMap};

    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();
    let verify = VerifyCompilation::snapshot(&dag);

    let mut props = PropertySet::new();
    props.coupling_map = Some(CouplingMap::linear(4));
    props.basis_gates = Some(BasisGates::ibm());
    DenseLayout.run(&mut dag, &mut props).unwrap();
    SabreRouting::new().run(&mut dag, &mut props).unwrap();
    ConsolidateBlocks.run(&mut dag, &mut props).unwrap();
    BasisTranslation.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Dense+Sabre+Consolidate+Basis must preserve semantics");
}

#[test]
fn test_full_pipeline_level3_preserves_semantics() {
    use arvak_compile::passes::VerifyCompilation;
    use arvak_compile::property::{BasisGates, CouplingMap};

    // Build a circuit with enough structure to exercise ConsolidateBlocks.
    let mut circuit = Circuit::with_size("test", 3, 0);
    circuit.h(QubitId(0)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap();
    circuit.cx(QubitId(0), QubitId(1)).unwrap(); // CX·CX = I → consolidation target
    circuit.h(QubitId(1)).unwrap();
    circuit.cx(QubitId(1), QubitId(2)).unwrap();
    circuit.t(QubitId(0)).unwrap();
    circuit.s(QubitId(2)).unwrap();
    let mut dag = circuit.into_dag();

    let verify = VerifyCompilation::snapshot(&dag);

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(3)
        .with_target(CouplingMap::linear(4), BasisGates::ibm())
        .build();

    pm.run(&mut dag, &mut props).unwrap();

    verify
        .run(&mut dag, &mut props)
        .expect("Level 3 compilation must preserve semantics");
}
