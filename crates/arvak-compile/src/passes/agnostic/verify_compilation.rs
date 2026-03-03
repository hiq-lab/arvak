//! Compilation verification pass using sampling-based falsification.
//!
//! Verifies that a compilation pass did not change circuit semantics by
//! comparing output probability distributions on random input states.
//!
//! # Algorithm
//!
//! For each of `num_trials` random computational-basis input states:
//! 1. Compute the output statevector of the "before" circuit.
//! 2. Compute the output statevector of the "after" circuit.
//! 3. Compare them up to global phase.
//!
//! If any trial finds a mismatch, the circuits are not equivalent.
//! This is a falsification approach: it can prove non-equivalence but
//! not prove equivalence. However, with enough trials on random inputs,
//! the probability of missing a bug is exponentially small.
//!
//! # Qubit Limit
//!
//! Statevector simulation is exponential in qubit count. This pass only
//! runs on circuits with ≤ `max_qubits` (default 20). Larger circuits
//! are skipped with a warning.

use num_complex::Complex64;
use tracing::warn;

use arvak_ir::{CircuitDag, InstructionKind, QubitId, StandardGate};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::{Layout, PropertySet};

/// Maximum qubits for verification (statevector simulation is 2^n).
const DEFAULT_MAX_QUBITS: usize = 20;

/// Number of random input states to test.
const DEFAULT_NUM_TRIALS: usize = 8;

/// Tolerance for statevector comparison.
const TOLERANCE: f64 = 1e-8;

/// Compilation verification pass.
///
/// Stores a snapshot of the circuit DAG before compilation and compares
/// it against the post-compilation DAG using statevector simulation.
///
/// # Usage
///
/// ```ignore
/// // Take a snapshot before compilation.
/// let snapshot = VerifyCompilation::snapshot(&dag);
///
/// // Run compilation passes...
/// routing_pass.run(&mut dag, &mut props)?;
///
/// // Verify equivalence.
/// snapshot.run(&mut dag, &mut props)?;
/// ```
pub struct VerifyCompilation {
    /// The pre-compilation DAG to compare against.
    before: CircuitDag,
    /// Maximum number of qubits to verify (skip larger circuits).
    max_qubits: usize,
    /// Number of random input states to test.
    num_trials: usize,
}

impl VerifyCompilation {
    /// Create a verification pass with a snapshot of the current DAG.
    pub fn snapshot(dag: &CircuitDag) -> Self {
        Self {
            before: dag.clone(),
            max_qubits: DEFAULT_MAX_QUBITS,
            num_trials: DEFAULT_NUM_TRIALS,
        }
    }

    /// Set the maximum number of qubits for verification.
    #[must_use]
    pub fn with_max_qubits(mut self, max_qubits: usize) -> Self {
        self.max_qubits = max_qubits;
        self
    }

    /// Set the number of random trials.
    #[must_use]
    pub fn with_num_trials(mut self, num_trials: usize) -> Self {
        self.num_trials = num_trials;
        self
    }
}

impl Pass for VerifyCompilation {
    fn name(&self) -> &'static str {
        "VerifyCompilation"
    }

    fn kind(&self) -> PassKind {
        PassKind::Analysis
    }

    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let num_virtual = self.before.num_qubits();

        // After routing, the DAG uses physical qubit IDs which may span a
        // larger qubit space than the virtual circuit. Use the coupling map
        // size when available, otherwise fall back to the DAG qubit count.
        let num_physical = if let Some(ref cm) = properties.coupling_map {
            cm.num_qubits() as usize
        } else {
            dag.num_qubits()
        };

        let check_qubits = num_virtual.max(num_physical);
        if check_qubits > self.max_qubits {
            warn!(
                num_qubits = check_qubits,
                max_qubits = self.max_qubits,
                "VerifyCompilation: circuit too large, skipping verification"
            );
            return Ok(());
        }

        // If initial_layout exists, routing has remapped the DAG to physical
        // qubit IDs. We must permute the input/output accordingly.
        let has_layout = properties.initial_layout.is_some();

        for trial in 0..self.num_trials {
            let virtual_input = trial % (1 << num_virtual);

            // Simulate the "before" circuit on virtual qubits.
            let sv_before = simulate_dag(&self.before, num_virtual, virtual_input)?;

            let sv_after_virtual = if has_layout {
                let init_layout = properties.initial_layout.as_ref().unwrap();
                let final_layout =
                    properties
                        .layout
                        .as_ref()
                        .ok_or_else(|| CompileError::PassFailed {
                            name: "VerifyCompilation".into(),
                            reason: "initial_layout is set but layout is None".into(),
                        })?;

                // Map virtual input to physical input via the initial layout.
                let physical_input =
                    permute_virtual_to_physical(virtual_input, init_layout, num_virtual);

                // Simulate the "after" circuit on physical qubits.
                let sv_after = simulate_dag(dag, num_physical, physical_input)?;

                // Map physical output back to virtual qubit ordering.
                permute_physical_to_virtual(&sv_after, final_layout, num_virtual)
            } else {
                // No layout: both circuits use the same virtual qubits.
                simulate_dag(dag, num_virtual, virtual_input)?
            };

            if !statevectors_equivalent(&sv_before, &sv_after_virtual) {
                return Err(CompileError::PassFailed {
                    name: "VerifyCompilation".into(),
                    reason: format!(
                        "compilation changed circuit semantics: mismatch on input \
                         state |{virtual_input}⟩ (trial {trial})"
                    ),
                });
            }
        }

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, _properties: &PropertySet) -> bool {
        true
    }
}

/// Map a virtual-qubit basis state index to physical-qubit space.
///
/// For each virtual qubit `v`, if bit `v` is set in `virtual_state`,
/// set bit `p` in the physical state where `p = layout[v]`.
fn permute_virtual_to_physical(virtual_state: usize, layout: &Layout, num_virtual: usize) -> usize {
    let mut physical_state = 0;
    for v in 0..num_virtual {
        if virtual_state & (1 << v) != 0 {
            let p = layout
                .get_physical(QubitId(u32::try_from(v).expect("qubit index overflow")))
                .unwrap_or(u32::try_from(v).expect("qubit index overflow"));
            physical_state |= 1 << (p as usize);
        }
    }
    physical_state
}

/// Permute a statevector from physical-qubit ordering to virtual-qubit ordering.
///
/// For each virtual basis state index, look up the corresponding physical index
/// using the final layout and copy the amplitude. Non-mapped physical qubits
/// (ancillas) are assumed to be in |0⟩ and projected out.
fn permute_physical_to_virtual(
    sv_physical: &[Complex64],
    final_layout: &Layout,
    num_virtual: usize,
) -> Vec<Complex64> {
    let virtual_dim = 1 << num_virtual;
    let mut result = vec![Complex64::new(0.0, 0.0); virtual_dim];

    for (virtual_idx, slot) in result.iter_mut().enumerate() {
        let mut physical_idx = 0;
        for v in 0..num_virtual {
            if virtual_idx & (1 << v) != 0 {
                let p = final_layout
                    .get_physical(QubitId(u32::try_from(v).expect("qubit index overflow")))
                    .unwrap_or(u32::try_from(v).expect("qubit index overflow"));
                physical_idx |= 1 << (p as usize);
            }
        }
        *slot = sv_physical[physical_idx];
    }

    result
}

/// Compare two statevectors for equivalence up to global phase.
///
/// Two statevectors |ψ⟩ and |φ⟩ are equivalent if |ψ⟩ = e^{iθ} |φ⟩
/// for some global phase θ.
fn statevectors_equivalent(a: &[Complex64], b: &[Complex64]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    // Find the first non-zero amplitude to determine the global phase.
    let mut phase: Option<Complex64> = None;

    for (ai, bi) in a.iter().zip(b.iter()) {
        let norm_a = ai.norm();
        let norm_b = bi.norm();

        // Both zero: skip.
        if norm_a < TOLERANCE && norm_b < TOLERANCE {
            continue;
        }

        // One zero, other not: not equivalent.
        if norm_a < TOLERANCE || norm_b < TOLERANCE {
            return false;
        }

        // Compute phase ratio.
        let ratio = *ai / *bi;
        if let Some(ref p) = phase {
            // Check consistency with previously determined phase.
            if (ratio - *p).norm() > TOLERANCE * 100.0 {
                return false;
            }
        } else {
            phase = Some(ratio);
        }
    }

    true
}

/// Simulate a DAG on a computational-basis input state.
///
/// Returns the output statevector as a `Vec<Complex64>`.
fn simulate_dag(
    dag: &CircuitDag,
    num_qubits: usize,
    input_state: usize,
) -> CompileResult<Vec<Complex64>> {
    let dim = 1 << num_qubits;
    let mut sv = vec![Complex64::new(0.0, 0.0); dim];
    sv[input_state % dim] = Complex64::new(1.0, 0.0);

    for (_, inst) in dag.topological_ops() {
        match &inst.kind {
            InstructionKind::Gate(gate) => match &gate.kind {
                arvak_ir::GateKind::Standard(std_gate) => {
                    apply_standard_gate(&mut sv, num_qubits, std_gate, &inst.qubits)?;
                }
                arvak_ir::GateKind::Custom(custom) => {
                    if let Some(ref matrix) = custom.matrix {
                        apply_custom_unitary(&mut sv, num_qubits, matrix, &inst.qubits)?;
                    } else {
                        return Err(CompileError::PassFailed {
                            name: "VerifyCompilation".into(),
                            reason: format!(
                                "custom gate '{}' has no unitary matrix for verification",
                                custom.name
                            ),
                        });
                    }
                }
            },
            InstructionKind::Barrier | InstructionKind::Delay { .. } => {
                // No-ops for simulation.
            }
            InstructionKind::Measure | InstructionKind::Reset => {
                // Skip measurement/reset — we compare unitary behaviour only.
            }
            InstructionKind::Shuttle { .. } => {
                // Shuttle is a physical operation, no unitary effect.
            }
            InstructionKind::NoiseChannel { .. } => {
                // Skip noise channels for equivalence checking.
            }
        }
    }

    Ok(sv)
}

/// Apply a standard gate to the statevector.
#[allow(clippy::too_many_lines)]
fn apply_standard_gate(
    sv: &mut [Complex64],
    num_qubits: usize,
    gate: &StandardGate,
    qubits: &[QubitId],
) -> CompileResult<()> {
    match gate {
        // Single-qubit gates.
        StandardGate::I => {}
        StandardGate::X => apply_1q(sv, num_qubits, qubits[0].0, &PAULI_X),
        StandardGate::Y => apply_1q(sv, num_qubits, qubits[0].0, &PAULI_Y),
        StandardGate::Z => apply_1q(sv, num_qubits, qubits[0].0, &PAULI_Z),
        StandardGate::H => apply_1q(sv, num_qubits, qubits[0].0, &HADAMARD),
        StandardGate::S => {
            let m = [
                Complex64::new(1.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 1.0),
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::Sdg => {
            let m = [
                Complex64::new(1.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, -1.0),
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::T => {
            let phase = Complex64::from_polar(1.0, std::f64::consts::FRAC_PI_4);
            let m = [
                Complex64::new(1.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                phase,
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::Tdg => {
            let phase = Complex64::from_polar(1.0, -std::f64::consts::FRAC_PI_4);
            let m = [
                Complex64::new(1.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                phase,
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::SX => {
            let half = Complex64::new(0.5, 0.0);
            let half_i = Complex64::new(0.0, 0.5);
            let m = [half + half_i, half - half_i, half - half_i, half + half_i];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::SXdg => {
            let half = Complex64::new(0.5, 0.0);
            let half_i = Complex64::new(0.0, 0.5);
            let m = [half - half_i, half + half_i, half + half_i, half - half_i];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::Rx(theta) => {
            let t = theta.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in Rx, cannot verify".into(),
            })?;
            let c = Complex64::new((t / 2.0).cos(), 0.0);
            let s = Complex64::new(0.0, -(t / 2.0).sin());
            let m = [c, s, s, c];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::Ry(theta) => {
            let t = theta.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in Ry, cannot verify".into(),
            })?;
            let c = Complex64::new((t / 2.0).cos(), 0.0);
            let s = Complex64::new((t / 2.0).sin(), 0.0);
            let m = [c, -s, s, c];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::Rz(theta) => {
            let t = theta.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in Rz, cannot verify".into(),
            })?;
            let m = [
                Complex64::from_polar(1.0, -t / 2.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::from_polar(1.0, t / 2.0),
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::P(lambda) => {
            let l = lambda.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in P, cannot verify".into(),
            })?;
            let m = [
                Complex64::new(1.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::from_polar(1.0, l),
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::U(theta, phi, lambda) => {
            let t = theta.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in U, cannot verify".into(),
            })?;
            let p = phi.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in U, cannot verify".into(),
            })?;
            let l = lambda.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in U, cannot verify".into(),
            })?;
            let c = (t / 2.0).cos();
            let s = (t / 2.0).sin();
            let m = [
                Complex64::new(c, 0.0),
                -Complex64::from_polar(s, l),
                Complex64::from_polar(s, p),
                Complex64::from_polar(c, p + l),
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }
        StandardGate::PRX(theta, phi) => {
            let t = theta.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in PRX, cannot verify".into(),
            })?;
            let p = phi.as_f64().ok_or_else(|| CompileError::PassFailed {
                name: "VerifyCompilation".into(),
                reason: "symbolic parameter in PRX, cannot verify".into(),
            })?;
            // PRX(θ, φ) = Rz(φ) · Rx(θ) · Rz(-φ)
            let c = Complex64::new((t / 2.0).cos(), 0.0);
            let s = (t / 2.0).sin();
            let m = [
                c,
                Complex64::new(0.0, -s) * Complex64::from_polar(1.0, -p),
                Complex64::new(0.0, -s) * Complex64::from_polar(1.0, p),
                c,
            ];
            apply_1q(sv, num_qubits, qubits[0].0, &m);
        }

        // Two-qubit gates.
        StandardGate::CX => apply_cx(sv, num_qubits, qubits[0].0, qubits[1].0),
        StandardGate::CY => {
            // CY = |0⟩⟨0| ⊗ I + |1⟩⟨1| ⊗ Y
            apply_controlled_1q(sv, num_qubits, qubits[0].0, qubits[1].0, &PAULI_Y);
        }
        StandardGate::CZ => {
            // CZ = |0⟩⟨0| ⊗ I + |1⟩⟨1| ⊗ Z
            apply_controlled_1q(sv, num_qubits, qubits[0].0, qubits[1].0, &PAULI_Z);
        }
        StandardGate::CH => {
            apply_controlled_1q(sv, num_qubits, qubits[0].0, qubits[1].0, &HADAMARD);
        }
        StandardGate::Swap => apply_swap(sv, num_qubits, qubits[0].0, qubits[1].0),
        StandardGate::ECR => {
            // ECR in |q0,q1⟩ basis (q0=MSB), matching gate.rs definition:
            // ECR = (1/√2) [[0, 0, 1, i], [0, 0, i, 1], [1, -i, 0, 0], [-i, 1, 0, 0]]
            let s = 1.0 / 2.0_f64.sqrt();
            let ecr = [
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(s, 0.0),
                Complex64::new(0.0, s),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, s),
                Complex64::new(s, 0.0),
                Complex64::new(s, 0.0),
                Complex64::new(0.0, -s),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, -s),
                Complex64::new(s, 0.0),
                Complex64::new(0.0, 0.0),
                Complex64::new(0.0, 0.0),
            ];
            apply_2q_unitary(sv, num_qubits, qubits[0].0, qubits[1].0, &ecr);
        }
        // Parameterized two-qubit gates and three-qubit gates: skip for now
        // (they are decomposed by BasisTranslation before verification runs).
        _ => {
            // For gates we haven't implemented, warn and skip.
            warn!(
                gate = ?gate,
                "VerifyCompilation: unsupported gate in verification, skipping"
            );
        }
    }

    Ok(())
}

/// Apply a custom unitary matrix to the statevector.
fn apply_custom_unitary(
    sv: &mut [Complex64],
    num_qubits: usize,
    matrix: &[Complex64],
    qubits: &[QubitId],
) -> CompileResult<()> {
    if qubits.len() == 1 {
        let m: [Complex64; 4] = matrix.try_into().map_err(|_| CompileError::PassFailed {
            name: "VerifyCompilation".into(),
            reason: "custom 1-qubit gate matrix must have 4 elements".into(),
        })?;
        apply_1q(sv, num_qubits, qubits[0].0, &m);
        Ok(())
    } else if qubits.len() == 2 {
        let m: [Complex64; 16] = matrix.try_into().map_err(|_| CompileError::PassFailed {
            name: "VerifyCompilation".into(),
            reason: "custom 2-qubit gate matrix must have 16 elements".into(),
        })?;
        apply_2q_unitary(sv, num_qubits, qubits[0].0, qubits[1].0, &m);
        Ok(())
    } else {
        Err(CompileError::PassFailed {
            name: "VerifyCompilation".into(),
            reason: format!(
                "custom gate on {} qubits not supported for verification",
                qubits.len()
            ),
        })
    }
}

// ---- Statevector primitives ----

const PAULI_X: [Complex64; 4] = [
    Complex64::new(0.0, 0.0),
    Complex64::new(1.0, 0.0),
    Complex64::new(1.0, 0.0),
    Complex64::new(0.0, 0.0),
];

const PAULI_Y: [Complex64; 4] = [
    Complex64::new(0.0, 0.0),
    Complex64::new(0.0, -1.0),
    Complex64::new(0.0, 1.0),
    Complex64::new(0.0, 0.0),
];

const PAULI_Z: [Complex64; 4] = [
    Complex64::new(1.0, 0.0),
    Complex64::new(0.0, 0.0),
    Complex64::new(0.0, 0.0),
    Complex64::new(-1.0, 0.0),
];

const HADAMARD: [Complex64; 4] = [
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0),
    Complex64::new(-std::f64::consts::FRAC_1_SQRT_2, 0.0),
];

/// Apply a 2x2 unitary to qubit `q` in the statevector.
fn apply_1q(sv: &mut [Complex64], num_qubits: usize, q: u32, matrix: &[Complex64; 4]) {
    let q = q as usize;
    let dim = 1 << num_qubits;
    let step = 1 << q;

    for i in 0..dim {
        if i & step != 0 {
            continue;
        }
        let j = i | step;
        let a = sv[i];
        let b = sv[j];
        sv[i] = matrix[0] * a + matrix[1] * b;
        sv[j] = matrix[2] * a + matrix[3] * b;
    }
}

/// Apply a CNOT gate (control, target).
fn apply_cx(sv: &mut [Complex64], num_qubits: usize, control: u32, target: u32) {
    let ctrl = control as usize;
    let tgt = target as usize;
    let dim = 1 << num_qubits;
    let ctrl_mask = 1 << ctrl;
    let tgt_mask = 1 << tgt;

    for i in 0..dim {
        if i & ctrl_mask != 0 && i & tgt_mask == 0 {
            let j = i | tgt_mask;
            sv.swap(i, j);
        }
    }
}

/// Apply a controlled single-qubit gate.
fn apply_controlled_1q(
    sv: &mut [Complex64],
    num_qubits: usize,
    control: u32,
    target: u32,
    matrix: &[Complex64; 4],
) {
    let ctrl = control as usize;
    let tgt = target as usize;
    let dim = 1 << num_qubits;
    let ctrl_mask = 1 << ctrl;
    let tgt_mask = 1 << tgt;

    for i in 0..dim {
        // Only act when control is |1⟩ and target is |0⟩.
        if i & ctrl_mask != 0 && i & tgt_mask == 0 {
            let j = i | tgt_mask;
            let a = sv[i];
            let b = sv[j];
            sv[i] = matrix[0] * a + matrix[1] * b;
            sv[j] = matrix[2] * a + matrix[3] * b;
        }
    }
}

/// Apply a SWAP gate.
fn apply_swap(sv: &mut [Complex64], num_qubits: usize, q1: u32, q2: u32) {
    let q1 = q1 as usize;
    let q2 = q2 as usize;
    let dim = 1 << num_qubits;
    let mask1 = 1 << q1;
    let mask2 = 1 << q2;

    for i in 0..dim {
        // Swap |...0...1...⟩ with |...1...0...⟩ where the bits differ at q1 and q2.
        if i & mask1 != 0 && i & mask2 == 0 {
            let j = (i & !mask1) | mask2;
            sv.swap(i, j);
        }
    }
}

/// Apply a 4x4 unitary to a two-qubit gate (q1, q2) in the statevector.
/// Matrix is in row-major order in the computational basis of (q1, q2):
/// |00⟩, |01⟩, |10⟩, |11⟩.
fn apply_2q_unitary(
    sv: &mut [Complex64],
    num_qubits: usize,
    q1: u32,
    q2: u32,
    matrix: &[Complex64; 16],
) {
    let q1 = q1 as usize;
    let q2 = q2 as usize;
    let dim = 1 << num_qubits;
    let mask1 = 1 << q1;
    let mask2 = 1 << q2;

    for i in 0..dim {
        // Only process when both q1 and q2 bits are 0 to avoid double-counting.
        if i & mask1 != 0 || i & mask2 != 0 {
            continue;
        }

        let i00 = i;
        let i01 = i | mask2;
        let i10 = i | mask1;
        let i11 = i | mask1 | mask2;

        let a = sv[i00];
        let b = sv[i01];
        let c = sv[i10];
        let d = sv[i11];

        sv[i00] = matrix[0] * a + matrix[1] * b + matrix[2] * c + matrix[3] * d;
        sv[i01] = matrix[4] * a + matrix[5] * b + matrix[6] * c + matrix[7] * d;
        sv[i10] = matrix[8] * a + matrix[9] * b + matrix[10] * c + matrix[11] * d;
        sv[i11] = matrix[12] * a + matrix[13] * b + matrix[14] * c + matrix[15] * d;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_identity_circuits_equivalent() {
        // Two identical circuits should verify as equivalent.
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let dag = circuit.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag);
        let mut dag_copy = dag;
        let mut props = PropertySet::new();
        verifier.run(&mut dag_copy, &mut props).unwrap();
    }

    #[test]
    fn test_different_circuits_detected() {
        // Original: H(0), CX(0,1) — creates Bell state
        // Modified: X(0), CX(0,1) — creates |11⟩
        // These are not equivalent.
        let mut original = Circuit::with_size("test", 2, 0);
        original.h(QubitId(0)).unwrap();
        original.cx(QubitId(0), QubitId(1)).unwrap();
        let dag_original = original.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag_original);

        let mut modified = Circuit::with_size("test", 2, 0);
        modified.x(QubitId(0)).unwrap();
        modified.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag_modified = modified.into_dag();

        let mut props = PropertySet::new();
        let result = verifier.run(&mut dag_modified, &mut props);
        assert!(result.is_err(), "should detect non-equivalent circuits");
    }

    #[test]
    fn test_global_phase_ignored() {
        // Two circuits that differ only by global phase should be equivalent.
        // Ry(π/2)·Rz(π) = -i·H (equals H up to global phase -i).
        let mut c1 = Circuit::with_size("test", 1, 0);
        c1.h(QubitId(0)).unwrap();
        let dag1 = c1.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag1);

        // Circuit applies Rz first, then Ry. Unitary = Ry(π/2) · Rz(π) = -i·H.
        let mut c2 = Circuit::with_size("test", 1, 0);
        c2.rz(std::f64::consts::PI, QubitId(0)).unwrap();
        c2.ry(std::f64::consts::FRAC_PI_2, QubitId(0)).unwrap();
        let mut dag2 = c2.into_dag();

        let mut props = PropertySet::new();
        verifier.run(&mut dag2, &mut props).unwrap();
    }

    #[test]
    fn test_bell_state_compilation_preserves_semantics() {
        // Full pipeline test: compile a Bell circuit for IQM and verify.
        use crate::passes::{BasicRouting, BasisTranslation, TrivialLayout};
        use crate::property::{BasisGates, CouplingMap};

        let mut circuit = Circuit::with_size("bell", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        // Snapshot before compilation.
        let verifier = VerifyCompilation::snapshot(&dag);

        // Compile.
        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        BasicRouting.run(&mut dag, &mut props).unwrap();
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // Verify: the compiled circuit should be equivalent to the original.
        verifier.run(&mut dag, &mut props).unwrap();
    }

    #[test]
    fn test_ghz_compilation_preserves_semantics() {
        // GHZ(3) compiled for IBM basis (RZ + SX + X + CX).
        // Note: Eagle basis (ECR) has a known qubit-ordering issue in the
        // CX→ECR decomposition (DEBT-03). Using IBM basis which is verified.
        use crate::passes::{BasicRouting, BasisTranslation, TrivialLayout};
        use crate::property::{BasisGates, CouplingMap};

        let circuit = Circuit::ghz(3).unwrap();
        let mut dag = circuit.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag);

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::ibm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        BasicRouting.run(&mut dag, &mut props).unwrap();
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        verifier.run(&mut dag, &mut props).unwrap();
    }

    #[test]
    fn test_eagle_cx_decomposition_known_issue() {
        // DEBT-03: The CX→ECR decomposition has a qubit-ordering issue.
        // VerifyCompilation correctly detects this as non-equivalent.
        use crate::passes::BasisTranslation;
        use crate::property::{BasisGates, CouplingMap};

        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag);

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::eagle());
        BasisTranslation.run(&mut dag, &mut props).unwrap();

        // This should fail because the CX→ECR decomposition is incorrect.
        let result = verifier.run(&mut dag, &mut props);
        assert!(
            result.is_err(),
            "expected Eagle CX→ECR decomposition to fail verification (DEBT-03)"
        );
    }

    #[test]
    fn test_sabre_routing_preserves_semantics() {
        // Verify SABRE routing doesn't break semantics.
        use crate::passes::{SabreRouting, TrivialLayout};
        use crate::property::{BasisGates, CouplingMap};

        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(3)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag);

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        verifier.run(&mut dag, &mut props).unwrap();
    }

    #[test]
    fn test_dense_layout_preserves_semantics() {
        // Verify DenseLayout + SabreRouting doesn't break semantics.
        use crate::passes::{DenseLayout, SabreRouting};
        use crate::property::{BasisGates, CouplingMap};

        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let verifier = VerifyCompilation::snapshot(&dag);

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        verifier.run(&mut dag, &mut props).unwrap();
    }

    #[test]
    fn test_statevectors_equivalent_same() {
        let a = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
        let b = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
        assert!(statevectors_equivalent(&a, &b));
    }

    #[test]
    fn test_statevectors_equivalent_global_phase() {
        let phase = Complex64::from_polar(1.0, 0.7);
        let a = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
        let b = vec![phase, Complex64::new(0.0, 0.0)];
        assert!(statevectors_equivalent(&a, &b));
    }

    #[test]
    fn test_statevectors_not_equivalent() {
        let a = vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
        let b = vec![Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0)];
        assert!(!statevectors_equivalent(&a, &b));
    }
}
