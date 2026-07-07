//! Unitary-equivalence regression tests for basis translation and 1-qubit
//! optimization.
//!
//! Every (gate, target-basis) pair is compiled and then simulated against the
//! original circuit with `VerifyCompilation` on all computational-basis
//! inputs. Each gate under test is sandwiched between fixed rotations
//! (`Ry`/`Rx`) so that *relative-phase* errors — invisible on bare basis
//! states (e.g. Rz(θ) vs Rz(−θ)) — become amplitude differences.
//!
//! These tests would have caught the historical bugs fixed alongside them:
//! - `Optimize1qGates` merging runs in reversed order (and never converging)
//! - IBM/Eagle/Heron `Rx` and Eagle/Heron `Ry` decompositions
//! - IQM `Rz` emitting the inverse rotation
//! - the Eagle `CX → ECR` decomposition

use arvak_compile::passes::{BasisTranslation, OneQubitBasis, Optimize1qGates, VerifyCompilation};
use arvak_compile::property::{BasisGates, CouplingMap};
use arvak_compile::{Pass, PropertySet};
use arvak_ir::{Circuit, QubitId};

/// A named gate-insertion case for the per-basis test matrices.
type GateCase<'a> = (&'a str, Box<dyn FnOnce(&mut Circuit)>);

/// A (basis-constructor, name) pair for iterating all target bases.
type BasisCase = (fn() -> BasisGates, &'static str);

const THETA: f64 = 0.7345;

/// Compile `circuit` with `BasisTranslation` for the given basis and verify
/// statevector equivalence with the original on all basis inputs.
fn assert_translation_preserves_semantics(circuit: &Circuit, basis: BasisGates, label: &str) {
    let mut dag = circuit.clone().into_dag();
    let num_trials = 1usize << circuit.num_qubits();
    let snapshot = VerifyCompilation::snapshot(&dag).with_num_trials(num_trials);

    let mut props =
        PropertySet::new().with_target(CouplingMap::full(circuit.num_qubits() as u32), basis);
    BasisTranslation
        .run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: translation failed: {e}"));

    // The coupling map / layout were not used (no routing ran), so drop them
    // before verification to compare in the same qubit space.
    let mut verify_props = PropertySet::new();
    snapshot
        .run(&mut dag, &mut verify_props)
        .unwrap_or_else(|e| panic!("{label}: translated circuit is NOT equivalent: {e}"));
}

/// Build a 1-qubit circuit `Ry(0.42) · G · Rx(0.91)` (application order) where
/// `G` is appended by `add_gate`. The sandwich turns phase errors in `G` into
/// amplitude errors on basis-state inputs.
fn sandwich_1q(add_gate: impl FnOnce(&mut Circuit)) -> Circuit {
    let mut c = Circuit::with_size("t", 1, 0);
    c.ry(0.42, QubitId(0)).unwrap();
    add_gate(&mut c);
    c.rx(0.91, QubitId(0)).unwrap();
    c
}

/// Build a 2-qubit circuit with rotations before and after the 2-qubit gate.
fn sandwich_2q(add_gate: impl FnOnce(&mut Circuit)) -> Circuit {
    let mut c = Circuit::with_size("t", 2, 0);
    c.ry(0.3, QubitId(0)).unwrap();
    c.rx(0.5, QubitId(1)).unwrap();
    add_gate(&mut c);
    c.ry(0.7, QubitId(0)).unwrap();
    c.rx(1.1, QubitId(1)).unwrap();
    c
}

/// Run the full per-basis gate matrix.
fn check_basis_1q(basis: fn() -> BasisGates, name: &str) {
    let q = QubitId(0);
    let cases: Vec<GateCase> = vec![
        ("x", Box::new(move |c| c.x(q).map(|_| ()).unwrap())),
        ("y", Box::new(move |c| c.y(q).map(|_| ()).unwrap())),
        ("z", Box::new(move |c| c.z(q).map(|_| ()).unwrap())),
        ("h", Box::new(move |c| c.h(q).map(|_| ()).unwrap())),
        ("s", Box::new(move |c| c.s(q).map(|_| ()).unwrap())),
        ("sdg", Box::new(move |c| c.sdg(q).map(|_| ()).unwrap())),
        ("t", Box::new(move |c| c.t(q).map(|_| ()).unwrap())),
        ("tdg", Box::new(move |c| c.tdg(q).map(|_| ()).unwrap())),
        ("rx", Box::new(move |c| c.rx(THETA, q).map(|_| ()).unwrap())),
        ("ry", Box::new(move |c| c.ry(THETA, q).map(|_| ()).unwrap())),
        ("rz", Box::new(move |c| c.rz(THETA, q).map(|_| ()).unwrap())),
    ];
    for (gate, add) in cases {
        let circuit = sandwich_1q(add);
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/{gate}"));
    }
}

#[test]
fn test_ibm_1q_gates_unitary_equivalent() {
    check_basis_1q(BasisGates::ibm, "ibm");
}

#[test]
fn test_eagle_1q_gates_unitary_equivalent() {
    check_basis_1q(BasisGates::eagle, "eagle");
}

#[test]
fn test_heron_1q_gates_unitary_equivalent() {
    check_basis_1q(BasisGates::heron, "heron");
}

#[test]
fn test_iqm_1q_gates_unitary_equivalent() {
    check_basis_1q(BasisGates::iqm, "iqm");
}

#[test]
fn test_neutral_atom_1q_gates_unitary_equivalent() {
    check_basis_1q(BasisGates::neutral_atom, "neutral_atom");
}

#[test]
fn test_iqm_sx_unitary_equivalent() {
    let circuit = sandwich_1q(|c| c.sx(QubitId(0)).map(|_| ()).unwrap());
    assert_translation_preserves_semantics(&circuit, BasisGates::iqm(), "iqm/sx");
    let circuit = sandwich_1q(|c| c.sxdg(QubitId(0)).map(|_| ()).unwrap());
    assert_translation_preserves_semantics(&circuit, BasisGates::iqm(), "iqm/sxdg");
}

#[test]
fn test_cx_unitary_equivalent_all_bases() {
    for (basis, name) in [
        (BasisGates::ibm as fn() -> BasisGates, "ibm"),
        (BasisGates::eagle, "eagle"),
        (BasisGates::heron, "heron"),
        (BasisGates::iqm, "iqm"),
        (BasisGates::neutral_atom, "neutral_atom"),
    ] {
        let circuit = sandwich_2q(|c| c.cx(QubitId(0), QubitId(1)).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/cx"));
    }
}

#[test]
fn test_cz_swap_unitary_equivalent_all_bases() {
    for (basis, name) in [
        (BasisGates::ibm as fn() -> BasisGates, "ibm"),
        (BasisGates::eagle, "eagle"),
        (BasisGates::heron, "heron"),
        (BasisGates::iqm, "iqm"),
        (BasisGates::neutral_atom, "neutral_atom"),
    ] {
        let circuit = sandwich_2q(|c| c.cz(QubitId(0), QubitId(1)).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/cz"));

        let circuit = sandwich_2q(|c| c.swap(QubitId(0), QubitId(1)).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/swap"));
    }
}

#[test]
fn test_heron_rzz_unitary_equivalent() {
    let circuit = sandwich_2q(|c| c.rzz(THETA, QubitId(0), QubitId(1)).map(|_| ()).unwrap());
    assert_translation_preserves_semantics(&circuit, BasisGates::heron(), "heron/rzz");
}

/// Build a 3-qubit circuit with rotations before and after the 3-qubit gate.
fn sandwich_3q(add_gate: impl FnOnce(&mut Circuit)) -> Circuit {
    let mut c = Circuit::with_size("t", 3, 0);
    c.ry(0.3, QubitId(0)).unwrap();
    c.rx(0.5, QubitId(1)).unwrap();
    c.ry(0.9, QubitId(2)).unwrap();
    add_gate(&mut c);
    c.ry(0.7, QubitId(0)).unwrap();
    c.rx(1.1, QubitId(1)).unwrap();
    c.ry(0.2, QubitId(2)).unwrap();
    c
}

const ALL_BASES: [BasisCase; 5] = [
    (BasisGates::ibm, "ibm"),
    (BasisGates::eagle, "eagle"),
    (BasisGates::heron, "heron"),
    (BasisGates::iqm, "iqm"),
    (BasisGates::neutral_atom, "neutral_atom"),
];

/// Gates without a target-specific rule must translate through the generic
/// `decompose_to_simpler` fallback on every basis (Raphael/IQM bug #1:
/// "compile() cannot decompose arbitrary gates into the target gate set").
#[test]
fn test_decomposed_1q_gates_unitary_equivalent_all_bases() {
    let q = QubitId(0);
    for (basis, name) in ALL_BASES {
        let circuit = sandwich_1q(|c| c.p(THETA, q).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/p"));

        let circuit = sandwich_1q(|c| c.u(0.3, 0.2, 0.1, q).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/u"));

        let circuit = sandwich_1q(|c| c.sxdg(q).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/sxdg"));
    }
}

#[test]
fn test_decomposed_2q_gates_unitary_equivalent_all_bases() {
    let (a, b) = (QubitId(0), QubitId(1));
    for (basis, name) in ALL_BASES {
        let cases: Vec<GateCase> = vec![
            ("cy", Box::new(move |c| c.cy(a, b).map(|_| ()).unwrap())),
            ("ch", Box::new(move |c| c.ch(a, b).map(|_| ()).unwrap())),
            (
                "crx",
                Box::new(move |c| c.crx(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "cry",
                Box::new(move |c| c.cry(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "crz",
                Box::new(move |c| c.crz(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "cp",
                Box::new(move |c| c.cp(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "rxx",
                Box::new(move |c| c.rxx(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "ryy",
                Box::new(move |c| c.ryy(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "rzz",
                Box::new(move |c| c.rzz(THETA, a, b).map(|_| ()).unwrap()),
            ),
            (
                "iswap",
                Box::new(move |c| c.iswap(a, b).map(|_| ()).unwrap()),
            ),
            ("ecr", Box::new(move |c| c.ecr(a, b).map(|_| ()).unwrap())),
        ];
        for (gate, add) in cases {
            let circuit = sandwich_2q(add);
            assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/{gate}"));
        }
    }
}

#[test]
fn test_decomposed_3q_gates_unitary_equivalent_all_bases() {
    let (a, b, t) = (QubitId(0), QubitId(1), QubitId(2));
    for (basis, name) in ALL_BASES {
        let circuit = sandwich_3q(|c| c.ccx(a, b, t).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/ccx"));

        let circuit = sandwich_3q(|c| c.cswap(a, b, t).map(|_| ()).unwrap());
        assert_translation_preserves_semantics(&circuit, basis(), &format!("{name}/cswap"));
    }
}

// ---------------------------------------------------------------------------
// Optimize1qGates
// ---------------------------------------------------------------------------

/// Run `Optimize1qGates` on the circuit and verify statevector equivalence.
fn assert_optimizer_preserves_semantics(circuit: &Circuit, basis: OneQubitBasis, label: &str) {
    let mut dag = circuit.clone().into_dag();
    let num_trials = 1usize << circuit.num_qubits();
    let snapshot = VerifyCompilation::snapshot(&dag).with_num_trials(num_trials);

    let mut props = PropertySet::new();
    Optimize1qGates::with_basis(basis)
        .run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: optimizer failed: {e}"));

    snapshot
        .run(&mut dag, &mut props)
        .unwrap_or_else(|e| panic!("{label}: optimized circuit is NOT equivalent: {e}"));
}

/// Non-commuting gate runs — the historical reversed-product bug produced
/// wrong circuits for every one of these.
fn non_commuting_circuits() -> Vec<(&'static str, Circuit)> {
    let q = QubitId(0);
    let mut out = Vec::new();

    let mut c = Circuit::with_size("hs", 1, 0);
    c.h(q).unwrap();
    c.s(q).unwrap();
    out.push(("h_then_s", c));

    let mut c = Circuit::with_size("sh", 1, 0);
    c.s(q).unwrap();
    c.h(q).unwrap();
    out.push(("s_then_h", c));

    let mut c = Circuit::with_size("hts", 1, 0);
    c.h(q).unwrap();
    c.t(q).unwrap();
    c.s(q).unwrap();
    c.h(q).unwrap();
    out.push(("h_t_s_h", c));

    let mut c = Circuit::with_size("rots", 1, 0);
    c.rx(0.3, q).unwrap();
    c.ry(0.4, q).unwrap();
    c.rz(0.5, q).unwrap();
    c.rx(1.2, q).unwrap();
    out.push(("rx_ry_rz_rx", c));

    let mut c = Circuit::with_size("xh", 1, 0);
    c.x(q).unwrap();
    c.h(q).unwrap();
    c.t(q).unwrap();
    out.push(("x_h_t", c));

    out
}

#[test]
fn test_optimize_1q_zyz_preserves_semantics() {
    for (label, circuit) in non_commuting_circuits() {
        assert_optimizer_preserves_semantics(&circuit, OneQubitBasis::ZYZ, &format!("zyz/{label}"));
    }
}

#[test]
fn test_optimize_1q_zsx_preserves_semantics() {
    for (label, circuit) in non_commuting_circuits() {
        assert_optimizer_preserves_semantics(&circuit, OneQubitBasis::ZSX, &format!("zsx/{label}"));
    }
}

#[test]
fn test_optimize_1q_u3_preserves_semantics() {
    for (label, circuit) in non_commuting_circuits() {
        assert_optimizer_preserves_semantics(&circuit, OneQubitBasis::U3, &format!("u3/{label}"));
    }
}

/// A 4-gate non-commuting run must strictly shrink under ZYZ (≤ 3 gates) and
/// the optimizer must terminate at a fixed point (regression: the previous
/// implementation ping-ponged between two emissions until an iteration cap).
#[test]
fn test_optimize_1q_converges_and_reduces() {
    let q = QubitId(0);
    let mut c = Circuit::with_size("t", 1, 0);
    c.rx(0.3, q).unwrap();
    c.ry(0.4, q).unwrap();
    c.rz(0.5, q).unwrap();
    c.rx(1.2, q).unwrap();

    let mut dag = c.into_dag();
    let mut props = PropertySet::new();
    Optimize1qGates::new().run(&mut dag, &mut props).unwrap();
    assert!(
        dag.num_ops() <= 3,
        "4-gate run should merge to ≤ 3 ZYZ gates, got {}",
        dag.num_ops()
    );

    // Running again must be a no-op (fixed point).
    let before = dag.num_ops();
    Optimize1qGates::new().run(&mut dag, &mut props).unwrap();
    assert_eq!(dag.num_ops(), before, "optimizer is not at a fixed point");
}

/// Full default pipeline (translation + optimization) on a circuit with
/// non-commuting 1q runs and rotations, for each IBM-family basis.
#[test]
fn test_full_pipeline_rotation_circuit_preserves_semantics() {
    use arvak_compile::PassManagerBuilder;

    for (basis, name) in [
        (BasisGates::ibm as fn() -> BasisGates, "ibm"),
        (BasisGates::eagle, "eagle"),
        (BasisGates::heron, "heron"),
        (BasisGates::iqm, "iqm"),
        (BasisGates::neutral_atom, "neutral_atom"),
    ] {
        let mut c = Circuit::with_size("t", 2, 0);
        c.h(QubitId(0)).unwrap();
        c.t(QubitId(0)).unwrap();
        c.rx(0.3, QubitId(1)).unwrap();
        c.cx(QubitId(0), QubitId(1)).unwrap();
        c.ry(0.8, QubitId(0)).unwrap();
        c.rz(1.4, QubitId(1)).unwrap();
        c.s(QubitId(0)).unwrap();

        let mut dag = c.clone().into_dag();
        let snapshot = VerifyCompilation::snapshot(&dag).with_num_trials(4);

        let (pm, mut props) = PassManagerBuilder::new()
            .with_optimization_level(1)
            .with_target(CouplingMap::full(2), basis())
            .build();
        pm.run(&mut dag, &mut props)
            .unwrap_or_else(|e| panic!("{name}: pipeline failed: {e}"));

        snapshot
            .run(&mut dag, &mut props)
            .unwrap_or_else(|e| panic!("{name}: full pipeline is NOT equivalent: {e}"));
    }
}
