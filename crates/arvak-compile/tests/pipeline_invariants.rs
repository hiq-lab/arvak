//! Pipeline invariant sweep over a frozen corpus of realistic circuits.
//!
//! The corpus (`tests/fixtures/corpus_*.qasm`) is Qrisp-generated arithmetic,
//! QFT, uncomputation chains, and CX ladders — the circuit shapes that
//! exposed eight compiler bugs in the 2026-07 IQM review and Qrisp stress
//! testing. Every fixture is compiled across all optimization levels, all
//! target bases, and several coupling-map shapes (including the
//! `new()`+`add_edge()` construction path the Python bindings use), and the
//! result must satisfy five invariants:
//!
//! 1. the pipeline terminates and succeeds (a hang fails via test timeout)
//! 2. the emitted QASM3 parses back (self-roundtrip)
//! 3. every gate is in the target basis
//! 4. every 2-qubit gate acts on coupled physical qubits
//! 5. register sizes are consistent (all indices < num_qubits, device-sized)
//!
//! For circuits <= 10 qubits on circuit-sized devices, statevector
//! equivalence with the input is verified as well (layout-aware).
//!
//! Bugs this sweep would have caught: SABRE non-termination, sxdg/ry/rz
//! basis leaks, `qubit[2] q;` with `q[2]` references, `shortest_path`
//! returning None on add_edge-built maps, invalid emitted QASM.

use arvak_compile::passes::VerifyCompilation;
use arvak_compile::property::{BasisGates, CouplingMap};
use arvak_compile::{Pass, PassManagerBuilder};
use arvak_ir::{Circuit, InstructionKind};

const ALL_BASES: [(fn() -> BasisGates, &str); 5] = [
    (BasisGates::ibm, "ibm"),
    (BasisGates::eagle, "eagle"),
    (BasisGates::heron, "heron"),
    (BasisGates::iqm, "iqm"),
    (BasisGates::neutral_atom, "neutral_atom"),
];

/// Semantic verification is exponential in qubit count; cap it.
const SEMANTICS_MAX_QUBITS: usize = 10;
const SEMANTICS_TRIALS: usize = 16;

fn load_fixture(name: &str) -> Circuit {
    let source = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture readable");
    arvak_qasm3::parse(&source).expect("fixture parses")
}

/// Coupling maps to sweep. `device_extra` adds spare physical qubits so the
/// register-consistency invariant is exercised with layouts that can place
/// logical qubits on high physical indices.
fn coupling_maps(n: u32, device_extra: u32) -> Vec<(String, CouplingMap)> {
    let d = n + device_extra;
    let mut maps = vec![
        (format!("linear{d}"), CouplingMap::linear(d)),
        (format!("star{d}"), CouplingMap::star(d)),
    ];

    // Coarse grid via from_edge_list.
    let side = (f64::from(d)).sqrt().ceil() as u32;
    let mut grid_edges = Vec::new();
    for r in 0..side {
        for c in 0..side {
            let q = r * side + c;
            if q >= d {
                continue;
            }
            if c + 1 < side && q + 1 < d {
                grid_edges.push((q, q + 1));
            }
            if r + 1 < side && q + side < d {
                grid_edges.push((q, q + side));
            }
        }
    }
    maps.push((
        format!("grid{d}"),
        CouplingMap::from_edge_list(d, &grid_edges),
    ));

    // The manual construction path (no precomputed distance matrices) —
    // exactly what the Python bindings produce. Regression for the
    // shortest_path-returns-None bug.
    let mut manual = CouplingMap::new(d);
    for i in 0..d.saturating_sub(1) {
        manual.add_edge(i, i + 1);
    }
    maps.push((format!("manual_linear{d}"), manual));

    maps
}

#[allow(clippy::cast_possible_truncation)]
fn check_case(
    fixture: &str,
    circuit: &Circuit,
    level: u8,
    basis_name: &str,
    basis: &BasisGates,
    map_name: &str,
    cmap: &CouplingMap,
    check_semantics: bool,
) {
    let label = format!("{fixture}/{basis_name}/{map_name}/o{level}");
    let device_size = cmap.num_qubits();

    let mut dag = circuit.clone().into_dag();
    let snapshot = VerifyCompilation::snapshot(&dag).with_num_trials(SEMANTICS_TRIALS);

    // Invariant 1: pipeline succeeds (termination enforced by test timeout).
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(level)
        .with_target(cmap.clone(), basis.clone())
        .build();
    pm.run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: pipeline failed: {e}"));

    // Invariant 5: registers consistent and device-sized.
    let num_qubits = dag.num_qubits();
    assert_eq!(
        num_qubits, device_size as usize,
        "{label}: routed circuit must span the device"
    );
    for (_, inst) in dag.topological_ops() {
        for q in &inst.qubits {
            assert!(
                (q.0 as usize) < num_qubits,
                "{label}: qubit index {} out of range (num_qubits {num_qubits})",
                q.0
            );
        }
    }

    // Invariants 3 + 4: basis conformance and coupling adjacency.
    for (_, inst) in dag.topological_ops() {
        if let InstructionKind::Gate(g) = &inst.kind {
            assert!(
                basis.contains(g.name()),
                "{label}: non-basis gate '{}' in output",
                g.name()
            );
            if inst.qubits.len() == 2 {
                assert!(
                    cmap.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "{label}: 2q gate on uncoupled qubits ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0
                );
            }
        }
    }

    // Invariant 2: emitted QASM3 parses back.
    let compiled = Circuit::from_dag(dag.clone());
    let qasm = arvak_qasm3::emit(&compiled).unwrap_or_else(|e| panic!("{label}: emit failed: {e}"));
    arvak_qasm3::parse(&qasm)
        .unwrap_or_else(|e| panic!("{label}: emitted QASM does not re-parse: {e}\n{qasm}"));

    // Layout-aware statevector equivalence for small circuit-sized devices.
    if check_semantics {
        snapshot
            .run(&mut dag, &mut props)
            .unwrap_or_else(|e| panic!("{label}: NOT semantically equivalent: {e}"));
    }
}

fn sweep(fixture: &str) {
    let circuit = load_fixture(fixture);
    let n = circuit.num_qubits() as u32;

    for (basis_fn, basis_name) in ALL_BASES {
        for level in [0u8, 1, 2, 3] {
            // Circuit-sized device: all invariants incl. semantics (small n).
            let semantics = (n as usize) <= SEMANTICS_MAX_QUBITS;
            for (map_name, cmap) in coupling_maps(n, 0) {
                check_case(
                    fixture,
                    &circuit,
                    level,
                    basis_name,
                    &basis_fn(),
                    &map_name,
                    &cmap,
                    semantics,
                );
            }
            // Oversized device: exercises layouts on high physical indices.
            for (map_name, cmap) in coupling_maps(n, 3) {
                check_case(
                    fixture,
                    &circuit,
                    level,
                    basis_name,
                    &basis_fn(),
                    &map_name,
                    &cmap,
                    false,
                );
            }
        }
    }
}

#[test]
fn test_pipeline_invariants_qfloat_add_4() {
    sweep("corpus_qfloat_add_4.qasm");
}

#[test]
fn test_pipeline_invariants_qfloat_add_6() {
    sweep("corpus_qfloat_add_6.qasm");
}

#[test]
// 13 qubits x 40 configs is ~70s in debug builds; the nightly release
// test job runs it, the fast PR job skips it.
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug; covered by nightly release tests"
)]
fn test_pipeline_invariants_qfloat_mul_3() {
    sweep("corpus_qfloat_mul_3.qasm");
}

#[test]
fn test_pipeline_invariants_qft_5() {
    sweep("corpus_qft_5.qasm");
}

#[test]
fn test_pipeline_invariants_qft_8() {
    sweep("corpus_qft_8.qasm");
}

#[test]
fn test_pipeline_invariants_qbool_and_4() {
    sweep("corpus_qbool_and_4.qasm");
}

#[test]
fn test_pipeline_invariants_cx_ladder_5() {
    sweep("corpus_cx_ladder_5.qasm");
}
