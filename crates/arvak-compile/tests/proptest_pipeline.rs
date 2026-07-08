//! Property-based fuzzing of the full compilation pipeline.
//!
//! Random circuits (full standard-gate set, 2-10 qubits) are compiled on
//! random connected coupling maps (spanning tree + extra edges, built via
//! both `from_edge_list` and the `new()`+`add_edge()` path) for random
//! bases and optimization levels. Every case must satisfy the same
//! invariants as the corpus sweep in `pipeline_invariants.rs`:
//!
//! 1. the pipeline terminates and succeeds (a hang fails via job timeout)
//! 2. the emitted QASM3 parses back
//! 3. every gate is in the target basis
//! 4. every 2-qubit gate acts on coupled physical qubits
//! 5. register sizes are consistent
//! 6. layout-aware statevector equivalence (circuits are <= 10 qubits)
//!
//! This is the only test genre that finds *unknown* failure classes like
//! the 2026-07 SABRE oscillation — corpus tests only re-check known
//! shapes. Case count: 32 by default (fast PR CI), overridden via
//! `PROPTEST_CASES` in the nightly job (512). Failures are persisted by
//! proptest under `proptest-regressions/` — commit those files.

use arvak_compile::passes::VerifyCompilation;
use arvak_compile::property::{BasisGates, CouplingMap};
use arvak_compile::{Pass, PassManagerBuilder};
use arvak_ir::{Circuit, InstructionKind, QubitId};
use proptest::prelude::*;

const SEMANTICS_TRIALS: usize = 4;

/// One random gate: kind index, qubit picks, angles.
#[derive(Debug, Clone)]
struct GateSpec {
    kind: usize,
    q: [usize; 3],
    theta: f64,
    phi: f64,
    lam: f64,
}

const NUM_GATE_KINDS: usize = 30;

fn gate_strategy() -> impl Strategy<Value = GateSpec> {
    (
        0..NUM_GATE_KINDS,
        prop::array::uniform3(0usize..64),
        -3.1f64..3.1,
        -3.1f64..3.1,
        -3.1f64..3.1,
    )
        .prop_map(|(kind, q, theta, phi, lam)| GateSpec {
            kind,
            q,
            theta,
            phi,
            lam,
        })
}

/// Apply a gate spec to the circuit, mapping the raw qubit picks onto
/// distinct in-range indices.
#[allow(clippy::too_many_lines)]
fn apply_gate(c: &mut Circuit, spec: &GateSpec, n: usize) {
    let q0 = QubitId((spec.q[0] % n) as u32);
    let q1 = QubitId(((spec.q[0] + 1 + spec.q[1] % (n - 1)) % n) as u32);
    let q2_raw = (spec.q[0] + spec.q[2]) % n;
    let (t, p, l) = (spec.theta, spec.phi, spec.lam);

    // Ignore Result: builder errors here would be generator bugs, and the
    // unwrap keeps failure output attached to the offending spec.
    match spec.kind {
        0 => c.x(q0).map(|_| ()).unwrap(),
        1 => c.y(q0).map(|_| ()).unwrap(),
        2 => c.z(q0).map(|_| ()).unwrap(),
        3 => c.h(q0).map(|_| ()).unwrap(),
        4 => c.s(q0).map(|_| ()).unwrap(),
        5 => c.sdg(q0).map(|_| ()).unwrap(),
        6 => c.t(q0).map(|_| ()).unwrap(),
        7 => c.tdg(q0).map(|_| ()).unwrap(),
        8 => c.sx(q0).map(|_| ()).unwrap(),
        9 => c.sxdg(q0).map(|_| ()).unwrap(),
        10 => c.rx(t, q0).map(|_| ()).unwrap(),
        11 => c.ry(t, q0).map(|_| ()).unwrap(),
        12 => c.rz(t, q0).map(|_| ()).unwrap(),
        13 => c.p(t, q0).map(|_| ()).unwrap(),
        14 => c.u(t, p, l, q0).map(|_| ()).unwrap(),
        15 => c.prx(t, p, q0).map(|_| ()).unwrap(),
        16 => c.cx(q0, q1).map(|_| ()).unwrap(),
        17 => c.cy(q0, q1).map(|_| ()).unwrap(),
        18 => c.cz(q0, q1).map(|_| ()).unwrap(),
        19 => c.ch(q0, q1).map(|_| ()).unwrap(),
        20 => c.cp(t, q0, q1).map(|_| ()).unwrap(),
        21 => c.crx(t, q0, q1).map(|_| ()).unwrap(),
        22 => c.cry(t, q0, q1).map(|_| ()).unwrap(),
        23 => c.crz(t, q0, q1).map(|_| ()).unwrap(),
        24 => c.swap(q0, q1).map(|_| ()).unwrap(),
        25 => c.iswap(q0, q1).map(|_| ()).unwrap(),
        26 => c.rxx(t, q0, q1).map(|_| ()).unwrap(),
        27 => c.ryy(t, q0, q1).map(|_| ()).unwrap(),
        28 => c.rzz(t, q0, q1).map(|_| ()).unwrap(),
        29 => {
            // Three-qubit gates need three distinct qubits.
            if n < 3 {
                c.ecr(q0, q1).map(|_| ()).unwrap();
                return;
            }
            let mut q2 = QubitId(q2_raw as u32);
            if q2 == q0 || q2 == q1 {
                q2 = QubitId(((q1.0 as usize + 1 + usize::from(q2_raw != 0)) % n) as u32);
            }
            if q2 == q0 || q2 == q1 {
                q2 = QubitId(((q0.0 as usize + n - 1) % n) as u32);
            }
            if q2 == q0 || q2 == q1 {
                c.ecr(q0, q1).map(|_| ()).unwrap();
            } else {
                c.ccx(q0, q1, q2).map(|_| ()).unwrap();
            }
        }
        _ => unreachable!(),
    }
}

/// A random connected coupling map over `device` qubits: random spanning
/// tree plus a few extra edges, optionally built via the manual
/// `new()`+`add_edge()` path (no precomputed distance matrices).
#[derive(Debug, Clone)]
struct MapSpec {
    tree_parents: Vec<usize>,
    extra_edges: Vec<(usize, usize)>,
    manual: bool,
}

fn map_strategy() -> impl Strategy<Value = MapSpec> {
    (
        prop::collection::vec(0usize..1024, 1..12),
        prop::collection::vec((0usize..64, 0usize..64), 0..4),
        any::<bool>(),
    )
        .prop_map(|(tree_parents, extra_edges, manual)| MapSpec {
            tree_parents,
            extra_edges,
            manual,
        })
}

fn build_map(spec: &MapSpec, device: u32) -> CouplingMap {
    let d = device as usize;
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for i in 1..d {
        let parent = spec.tree_parents[(i - 1) % spec.tree_parents.len()] % i;
        edges.push((parent as u32, i as u32));
    }
    for &(a, b) in &spec.extra_edges {
        let (a, b) = (a % d, b % d);
        if a != b {
            edges.push((a as u32, b as u32));
        }
    }
    if spec.manual {
        let mut map = CouplingMap::new(device);
        for &(a, b) in &edges {
            map.add_edge(a, b);
        }
        map
    } else {
        CouplingMap::from_edge_list(device, &edges)
    }
}

const BASES: [fn() -> BasisGates; 5] = [
    BasisGates::ibm,
    BasisGates::eagle,
    BasisGates::heron,
    BasisGates::iqm,
    BasisGates::neutral_atom,
];

fn fuzz_case(
    n: usize,
    gates: &[GateSpec],
    map_spec: &MapSpec,
    extra_qubits: u32,
    basis_idx: usize,
    level: u8,
) {
    let mut circuit = Circuit::with_size("fuzz", n as u32, 0);
    for g in gates {
        apply_gate(&mut circuit, g, n);
    }

    let device = n as u32 + extra_qubits;
    let cmap = build_map(map_spec, device);
    let basis = BASES[basis_idx % BASES.len()]();
    let label = format!(
        "n{n}+{extra_qubits} basis{basis_idx} o{level} manual:{}",
        map_spec.manual
    );

    let mut dag = circuit.clone().into_dag();
    let snapshot = VerifyCompilation::snapshot(&dag).with_num_trials(SEMANTICS_TRIALS);

    // Invariant 1: pipeline succeeds.
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(level)
        .with_target(cmap.clone(), basis.clone())
        .build();
    pm.run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: pipeline failed: {e}"));

    // Invariant 5: register consistency.
    let num_qubits = dag.num_qubits();
    assert_eq!(
        num_qubits, device as usize,
        "{label}: routed circuit must span the device"
    );
    for (_, inst) in dag.topological_ops() {
        for q in &inst.qubits {
            assert!(
                (q.0 as usize) < num_qubits,
                "{label}: qubit index {} out of range",
                q.0
            );
        }
    }

    // Invariants 3 + 4: basis conformance, coupling adjacency.
    for (_, inst) in dag.topological_ops() {
        if let InstructionKind::Gate(g) = &inst.kind {
            assert!(
                basis.contains(g.name()),
                "{label}: non-basis gate '{}'",
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

    // Invariant 2: emitted QASM3 re-parses.
    let compiled = Circuit::from_dag(dag.clone());
    let qasm = arvak_qasm3::emit(&compiled).unwrap_or_else(|e| panic!("{label}: emit failed: {e}"));
    arvak_qasm3::parse(&qasm)
        .unwrap_or_else(|e| panic!("{label}: emitted QASM does not re-parse: {e}\n{qasm}"));

    // Invariant 6: statevector equivalence (layout-aware).
    snapshot
        .run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: NOT semantically equivalent: {e}"));
}

fn case_count() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(32)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: case_count(),
        // Shrinking re-runs the full pipeline; keep it bounded.
        max_shrink_iters: 200,
        .. ProptestConfig::default()
    })]

    #[test]
    fn fuzz_pipeline_invariants(
        n in 2usize..=8,
        gates in prop::collection::vec(gate_strategy(), 1..40),
        map_spec in map_strategy(),
        extra_qubits in 0u32..4,
        basis_idx in 0usize..5,
        level in 0u8..=3,
    ) {
        fuzz_case(n, &gates, &map_spec, extra_qubits, basis_idx, level);
    }
}
