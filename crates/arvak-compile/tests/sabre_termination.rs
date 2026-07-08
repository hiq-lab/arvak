//! Regression tests for SABRE routing non-termination.
//!
//! Found 2026-07-08 by `benchmarks/qrisp_stress.py`: Qrisp's ripple-carry
//! adders sent SABRE into an infinite SWAP oscillation (pick best SWAP →
//! next iteration picks the inverse SWAP → repeat forever) on linear
//! coupling maps. The fixtures are the exact circuits that hung:
//!
//! * `qfloat_add(5)` (10 qubits) — hangs with TrivialLayout (level 1)
//! * `qfloat_add(6)` (12 qubits) — hangs with DenseLayout (level >= 2)
//!
//! The fix adds a decay penalty on recently swapped qubits plus a
//! stagnation escape that greedily routes the first front-layer gate
//! along the shortest path, which guarantees progress.

use arvak_compile::PassManagerBuilder;
use arvak_compile::property::{BasisGates, CouplingMap};
use arvak_ir::InstructionKind;

fn compile_fixture(fixture: &str, opt_level: u8) {
    let source = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture readable");
    let circuit = arvak_qasm3::parse(&source).expect("fixture parses");
    let n = circuit.num_qubits() as u32;

    let mut dag = circuit.into_dag();
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(opt_level)
        .with_target(CouplingMap::linear(n), BasisGates::ibm())
        .build();
    pm.run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{fixture}/o{opt_level}: pipeline failed: {e}"));

    // Routing must leave every 2-qubit gate on adjacent physical qubits.
    let cmap = CouplingMap::linear(n);
    for (_, inst) in dag.topological_ops() {
        if let InstructionKind::Gate(_) = &inst.kind {
            if inst.qubits.len() == 2 {
                assert!(
                    cmap.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "{fixture}/o{opt_level}: non-adjacent 2q gate on ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0,
                );
            }
        }
    }
}

#[test]
fn test_qfloat_add5_trivial_layout_terminates() {
    compile_fixture("sabre_hang_qfloat_add5.qasm", 1);
}

#[test]
fn test_qfloat_add6_dense_layout_terminates() {
    compile_fixture("sabre_hang_qfloat_add6.qasm", 2);
}

#[test]
fn test_qfloat_add6_level3_terminates() {
    compile_fixture("sabre_hang_qfloat_add6.qasm", 3);
}
