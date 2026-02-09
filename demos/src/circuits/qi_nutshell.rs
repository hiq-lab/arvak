//! QI-Nutshell: Quantum Internet Protocol Emulation Circuits
//!
//! Implements the QKD protocol mapping from "Quantum Internet in a Nutshell"
//! (Hilder et al., arXiv:2507.14383v2), demonstrating Arvak's ability to
//! express quantum communication protocols as compilable circuits.
//!
//! # Protocols
//!
//! - **BB84**: Prepare-and-measure QKD with optional eavesdropper
//! - **BBM92**: Entanglement-based QKD with optional eavesdropper
//! - **PCCM Attack**: Phase Covariant Cloning Machine (parameterized)
//! - **QEC-QKD**: BB84 with integrated \[\[4,2,2\]\] error detection
//!
//! # Register Mapping
//!
//! Following the QI-Nutshell approach, quantum communication parties are mapped
//! to named qubit registers on a single quantum processor:
//!
//! | Party   | Register | Role                              |
//! |---------|----------|-----------------------------------|
//! | Alice   | `a`      | Sender / key generator            |
//! | Bob     | `b`      | Receiver / key verifier           |
//! | Eve     | `e`      | Eavesdropper (cloning register)   |
//! | Syndrome| `syn`    | QEC stabilizer ancillae           |

use std::f64::consts::PI;

use arvak_ir::Circuit;
use arvak_ir::parameter::ParameterExpression;
use arvak_ir::qubit::QubitId;

/// Encoding basis for QKD protocols.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Basis {
    /// Computational basis (Z): |0⟩, |1⟩
    Z,
    /// Hadamard basis (X): |+⟩, |−⟩
    X,
}

/// Eavesdropping strategy.
#[derive(Debug, Clone)]
pub enum EveStrategy {
    /// No eavesdropper present.
    None,
    /// Intercept-resend: Eve measures and re-prepares.
    InterceptResend,
    /// Phase Covariant Cloning Machine with attack angle θ.
    /// θ controls the fidelity trade-off: higher θ = better clone, worse original.
    Pccm(f64),
    /// Parameterized PCCM for variational optimization.
    PccmVariational,
}

// ============================================================================
// BB84 Protocol
// ============================================================================

/// Build a BB84 QKD circuit for a single key bit.
///
/// Maps the BB84 prepare-and-measure protocol onto qubit registers:
/// - Alice prepares a qubit encoding `bit` in `alice_basis`
/// - (Optional) Eve intercepts using the chosen strategy
/// - Bob measures in `bob_basis`
///
/// When `alice_basis == bob_basis`, the key bit is deterministic.
/// Basis mismatch or eavesdropping introduces detectable errors.
///
/// # Register layout
///
/// | Register | Qubits | Purpose                     |
/// |----------|--------|-----------------------------|
/// | `a`      | 1      | Alice's prepared qubit      |
/// | `e`      | 0–2    | Eve's ancilla (if attacking)|
/// | `b_meas` | 1      | Bob's measurement outcome   |
pub fn bb84_circuit(bit: bool, alice_basis: Basis, bob_basis: Basis, eve: &EveStrategy) -> Circuit {
    let eve_qubits: u32 = match eve {
        EveStrategy::None => 0,
        EveStrategy::InterceptResend => 0, // Eve uses Alice's qubit directly
        EveStrategy::Pccm(_) | EveStrategy::PccmVariational => 2,
    };

    let mut circuit = Circuit::new("bb84");

    // Named registers following QI-Nutshell convention
    let alice = circuit.add_qreg("a", 1);
    let eve_reg = if eve_qubits > 0 {
        circuit.add_qreg("e", eve_qubits)
    } else {
        vec![]
    };
    let bob_creg = circuit.add_creg("b_meas", 1);
    let eve_creg = if matches!(eve, EveStrategy::InterceptResend) {
        circuit.add_creg("e_meas", 1)
    } else {
        vec![]
    };

    let a0 = alice[0];

    // ── Phase 1: Alice's state preparation ──────────────────────────
    circuit
        .barrier(
            [a0].into_iter()
                .chain(eve_reg.iter().copied())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    // Encode bit value
    if bit {
        circuit.x(a0).unwrap();
    }

    // Encode in chosen basis
    if alice_basis == Basis::X {
        circuit.h(a0).unwrap();
    }

    // ── Phase 2: Channel (Eve's attack) ─────────────────────────────
    circuit
        .barrier(
            [a0].into_iter()
                .chain(eve_reg.iter().copied())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    match eve {
        EveStrategy::None => {
            // Clean channel — no intervention
        }
        EveStrategy::InterceptResend => {
            // Eve measures Alice's qubit, then re-prepares
            // (Always measures in Z basis — a naive strategy)
            circuit.measure(a0, eve_creg[0]).unwrap();
            circuit.reset(a0).unwrap();
            // Eve re-prepares based on her measurement (simplified: same state)
            // In a real conditional circuit this would use c_if
        }
        EveStrategy::Pccm(theta) => {
            // Phase Covariant Cloning Machine
            // Maps |ψ⟩|0⟩|0⟩ → approximate clones on a0 and e0
            apply_pccm(&mut circuit, a0, eve_reg[0], eve_reg[1], *theta);
        }
        EveStrategy::PccmVariational => {
            let theta = ParameterExpression::symbol("theta");
            apply_pccm_parameterized(&mut circuit, a0, eve_reg[0], eve_reg[1], theta);
        }
    }

    // ── Phase 3: Bob's measurement ──────────────────────────────────
    circuit
        .barrier(
            [a0].into_iter()
                .chain(eve_reg.iter().copied())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    // Bob chooses his measurement basis
    if bob_basis == Basis::X {
        circuit.h(a0).unwrap();
    }

    circuit.measure(a0, bob_creg[0]).unwrap();

    circuit
}

/// Build a full BB84 key exchange for `n` rounds.
///
/// Each round uses independently random bases for Alice and Bob.
/// Returns the circuit plus the list of (alice_basis, bob_basis) pairs
/// for sifting in classical post-processing.
pub fn bb84_multi_round(
    key_bits: &[bool],
    alice_bases: &[Basis],
    bob_bases: &[Basis],
    eve: &EveStrategy,
) -> Circuit {
    assert_eq!(key_bits.len(), alice_bases.len());
    assert_eq!(key_bits.len(), bob_bases.len());
    let n = key_bits.len();

    let eve_qubits_per_round: u32 = match eve {
        EveStrategy::Pccm(_) | EveStrategy::PccmVariational => 2,
        _ => 0,
    };

    let mut circuit = Circuit::new("bb84_multi");

    // One Alice qubit per round
    let alice_reg = circuit.add_qreg("a", n as u32);
    let eve_reg = if eve_qubits_per_round > 0 {
        circuit.add_qreg("e", n as u32 * eve_qubits_per_round)
    } else {
        vec![]
    };
    let bob_creg = circuit.add_creg("b_meas", n as u32);

    for round in 0..n {
        let a = alice_reg[round];

        // Alice prepares
        if key_bits[round] {
            circuit.x(a).unwrap();
        }
        if alice_bases[round] == Basis::X {
            circuit.h(a).unwrap();
        }

        // Eve's attack (if PCCM)
        if let EveStrategy::Pccm(theta) = eve {
            let e_base = round as u32 * eve_qubits_per_round;
            let e0 = eve_reg[e_base as usize];
            let e1 = eve_reg[(e_base + 1) as usize];
            apply_pccm(&mut circuit, a, e0, e1, *theta);
        }

        // Bob measures
        if bob_bases[round] == Basis::X {
            circuit.h(a).unwrap();
        }
        circuit.measure(a, bob_creg[round]).unwrap();
    }

    circuit
}

// ============================================================================
// BBM92 Protocol
// ============================================================================

/// Build a BBM92 entanglement-based QKD circuit.
///
/// BBM92 uses shared Bell pairs instead of state preparation:
/// 1. A source creates a Bell pair |Φ+⟩ = (|00⟩ + |11⟩)/√2
/// 2. One half goes to Alice, one to Bob
/// 3. Both measure in independently chosen bases
/// 4. Matching bases → correlated key bits
///
/// # Register layout
///
/// | Register | Qubits | Purpose                     |
/// |----------|--------|-----------------------------|
/// | `a`      | 1      | Alice's half of Bell pair   |
/// | `b`      | 1      | Bob's half of Bell pair     |
/// | `e`      | 0–2    | Eve's cloning register      |
pub fn bbm92_circuit(alice_basis: Basis, bob_basis: Basis, eve: &EveStrategy) -> Circuit {
    let eve_qubits: u32 = match eve {
        EveStrategy::Pccm(_) | EveStrategy::PccmVariational => 2,
        _ => 0,
    };

    let mut circuit = Circuit::new("bbm92");

    let alice = circuit.add_qreg("a", 1);
    let bob = circuit.add_qreg("b", 1);
    let eve_reg = if eve_qubits > 0 {
        circuit.add_qreg("e", eve_qubits)
    } else {
        vec![]
    };
    let alice_creg = circuit.add_creg("a_meas", 1);
    let bob_creg = circuit.add_creg("b_meas", 1);

    let a0 = alice[0];
    let b0 = bob[0];

    // ── Phase 1: Entanglement source ────────────────────────────────
    // Prepare Bell state |Φ+⟩ = (|00⟩ + |11⟩)/√2
    circuit.h(a0).unwrap();
    circuit.cx(a0, b0).unwrap();

    circuit
        .barrier(
            [a0, b0]
                .into_iter()
                .chain(eve_reg.iter().copied())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    // ── Phase 2: Channel (Eve intercepts Bob's half) ────────────────
    if let EveStrategy::Pccm(theta) = eve {
        apply_pccm(&mut circuit, b0, eve_reg[0], eve_reg[1], *theta);
        circuit
            .barrier(
                [a0, b0]
                    .into_iter()
                    .chain(eve_reg.iter().copied())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
    }

    // ── Phase 3: Measurements ───────────────────────────────────────
    // Alice chooses basis
    if alice_basis == Basis::X {
        circuit.h(a0).unwrap();
    }
    circuit.measure(a0, alice_creg[0]).unwrap();

    // Bob chooses basis
    if bob_basis == Basis::X {
        circuit.h(b0).unwrap();
    }
    circuit.measure(b0, bob_creg[0]).unwrap();

    circuit
}

// ============================================================================
// Phase Covariant Cloning Machine (PCCM)
// ============================================================================

/// Apply a Phase Covariant Cloning Machine.
///
/// The PCCM creates an approximate clone of a qubit from the equatorial plane
/// of the Bloch sphere. The attack angle θ controls the fidelity trade-off:
///
/// - θ = 0: No disturbance (Eve gets nothing)
/// - θ = π/4: Optimal symmetric cloning (equal fidelity for Alice/Bob and Eve)
/// - θ = π/2: Eve gets a perfect copy (Bob's state is destroyed)
///
/// Circuit decomposition (from QI-Nutshell paper, Fig. 4):
///
/// ```text
///  input ─────●────Ry(θ)────●───── clone_out
///             │              │
///  e0    ─────X─────────────X───── (ancilla)
///  e1    ──────────────────────── (Eve's copy extracted via CNOT)
/// ```
fn apply_pccm(circuit: &mut Circuit, input: QubitId, e0: QubitId, e1: QubitId, theta: f64) {
    // PCCM decomposition:
    // 1. CNOT input → e0 (entangle input with Eve's ancilla)
    circuit.cx(input, e0).unwrap();

    // 2. Ry(θ) on input (rotate to control clone fidelity)
    circuit.ry(theta, input).unwrap();

    // 3. CNOT input → e0 (disentangle conditionally)
    circuit.cx(input, e0).unwrap();

    // 4. CNOT e0 → e1 (Eve extracts her copy)
    circuit.cx(e0, e1).unwrap();
}

/// Apply a parameterized PCCM for variational optimization.
///
/// Uses a symbolic parameter `theta` that can be bound at runtime,
/// enabling hybrid quantum-classical optimization of the attack angle
/// (as demonstrated in the QI-Nutshell paper using COBYLA).
fn apply_pccm_parameterized(
    circuit: &mut Circuit,
    input: QubitId,
    e0: QubitId,
    e1: QubitId,
    theta: ParameterExpression,
) {
    circuit.cx(input, e0).unwrap();
    circuit.ry(theta, input).unwrap();
    circuit.cx(input, e0).unwrap();
    circuit.cx(e0, e1).unwrap();
}

// ============================================================================
// QEC-Integrated QKD ([[4,2,2]] detection code)
// ============================================================================

/// Build a BB84 circuit with integrated \[\[4,2,2\]\] error detection.
///
/// The \[\[4,2,2\]\] code encodes 2 logical qubits into 4 physical qubits and
/// can detect any single-qubit error. When integrated into QKD, the stabilizer
/// measurements serve a dual purpose:
///
/// 1. **Error suppression**: Detect and discard corrupted transmissions
/// 2. **Channel fingerprinting**: Deviations in syndrome statistics reveal
///    eavesdropping activity (the QI-Nutshell "privacy authentication" insight)
///
/// # Register layout
///
/// | Register | Qubits | Purpose                          |
/// |----------|--------|----------------------------------|
/// | `data`   | 4      | Encoded data block               |
/// | `syn`    | 2      | Stabilizer measurement ancillae  |
/// | `b_meas` | 2 bits | Bob's decoded key bits           |
/// | `s_meas` | 2 bits | Syndrome outcomes                |
pub fn bb84_qec_circuit(
    bits: [bool; 2],
    alice_basis: Basis,
    bob_basis: Basis,
    inject_noise: bool,
) -> Circuit {
    let mut circuit = Circuit::new("bb84_qec_422");

    let data = circuit.add_qreg("data", 4);
    let syn = circuit.add_qreg("syn", 2);
    let bob_creg = circuit.add_creg("b_meas", 2);
    let syn_creg = circuit.add_creg("s_meas", 2);

    // ── Phase 1: Encode logical qubits into [[4,2,2]] ───────────────
    // Prepare logical input
    if bits[0] {
        circuit.x(data[0]).unwrap();
    }
    if bits[1] {
        circuit.x(data[1]).unwrap();
    }

    // Basis encoding
    if alice_basis == Basis::X {
        circuit.h(data[0]).unwrap();
        circuit.h(data[1]).unwrap();
    }

    circuit.barrier(data.clone()).unwrap();

    // [[4,2,2]] encoding circuit
    // Stabilizer generators: X⊗X⊗X⊗X and Z⊗Z⊗Z⊗Z
    // Encoding: |ψ₁ψ₂⟩ → encoded state across 4 qubits
    circuit.cx(data[0], data[2]).unwrap();
    circuit.cx(data[1], data[3]).unwrap();
    circuit.h(data[2]).unwrap();
    circuit.h(data[3]).unwrap();
    circuit.cx(data[2], data[0]).unwrap();
    circuit.cx(data[3], data[1]).unwrap();

    circuit.barrier(data.clone()).unwrap();

    // ── Phase 2: Quantum channel (with optional noise injection) ────
    if inject_noise {
        // Simulate a single-qubit bit-flip error on qubit 2
        // In a real scenario this would be a noise model;
        // here we inject a deterministic X error for demonstration
        circuit.x(data[2]).unwrap();
    }

    circuit
        .barrier(
            data.iter()
                .copied()
                .chain(syn.iter().copied())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    // ── Phase 3: Stabilizer measurements ────────────────────────────
    // Measure X⊗X⊗X⊗X stabilizer
    for &d in &data {
        circuit.cx(d, syn[0]).unwrap();
    }

    // Measure Z⊗Z⊗Z⊗Z stabilizer
    for &d in &data {
        circuit.h(d).unwrap();
    }
    for &d in &data {
        circuit.cx(d, syn[1]).unwrap();
    }
    for &d in &data {
        circuit.h(d).unwrap();
    }

    circuit.measure(syn[0], syn_creg[0]).unwrap();
    circuit.measure(syn[1], syn_creg[1]).unwrap();

    circuit.barrier(data.clone()).unwrap();

    // ── Phase 4: Decode and measure ─────────────────────────────────
    // Reverse encoding
    circuit.cx(data[3], data[1]).unwrap();
    circuit.cx(data[2], data[0]).unwrap();
    circuit.h(data[3]).unwrap();
    circuit.h(data[2]).unwrap();
    circuit.cx(data[1], data[3]).unwrap();
    circuit.cx(data[0], data[2]).unwrap();

    // Bob's basis choice
    if bob_basis == Basis::X {
        circuit.h(data[0]).unwrap();
        circuit.h(data[1]).unwrap();
    }

    circuit.measure(data[0], bob_creg[0]).unwrap();
    circuit.measure(data[1], bob_creg[1]).unwrap();

    circuit
}

// ============================================================================
// Protocol analysis helpers
// ============================================================================

/// Compute theoretical PCCM fidelities for a given attack angle.
///
/// Returns (F_AB, F_AE) where:
/// - F_AB = fidelity of Bob's state with Alice's original
/// - F_AE = fidelity of Eve's clone with Alice's original
pub fn pccm_fidelities(theta: f64) -> (f64, f64) {
    let cos_t = theta.cos();
    let sin_t = theta.sin();

    // From QI-Nutshell paper: equatorial qubit cloning fidelities
    let f_ab = (1.0 + cos_t) / 2.0;
    let f_ae = (1.0 + sin_t) / 2.0;

    (f_ab, f_ae)
}

/// Find the optimal symmetric PCCM attack angle.
///
/// At the symmetric point, F_AB = F_AE, which occurs at θ = π/4.
pub fn optimal_symmetric_angle() -> f64 {
    PI / 4.0
}

/// QBER (Quantum Bit Error Rate) estimate for a given PCCM angle.
///
/// When Alice and Bob use matching bases, QBER = 1 - F_AB.
pub fn pccm_qber(theta: f64) -> f64 {
    let (f_ab, _) = pccm_fidelities(theta);
    1.0 - f_ab
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bb84_no_eve() {
        let circuit = bb84_circuit(false, Basis::Z, Basis::Z, &EveStrategy::None);
        assert_eq!(circuit.num_qubits(), 1); // Just Alice's qubit
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_bb84_x_basis() {
        let circuit = bb84_circuit(true, Basis::X, Basis::X, &EveStrategy::None);
        assert_eq!(circuit.num_qubits(), 1);
    }

    #[test]
    fn test_bb84_with_pccm() {
        let circuit = bb84_circuit(false, Basis::Z, Basis::Z, &EveStrategy::Pccm(PI / 4.0));
        assert_eq!(circuit.num_qubits(), 3); // Alice + 2 Eve
    }

    #[test]
    fn test_bb84_variational_pccm() {
        let circuit = bb84_circuit(false, Basis::Z, Basis::Z, &EveStrategy::PccmVariational);
        assert_eq!(circuit.num_qubits(), 3);
    }

    #[test]
    fn test_bbm92_no_eve() {
        let circuit = bbm92_circuit(Basis::Z, Basis::Z, &EveStrategy::None);
        assert_eq!(circuit.num_qubits(), 2); // Alice + Bob
    }

    #[test]
    fn test_bbm92_with_pccm() {
        let circuit = bbm92_circuit(Basis::Z, Basis::Z, &EveStrategy::Pccm(PI / 4.0));
        assert_eq!(circuit.num_qubits(), 4); // Alice + Bob + 2 Eve
    }

    #[test]
    fn test_bb84_qec() {
        let circuit = bb84_qec_circuit([false, true], Basis::Z, Basis::Z, false);
        assert_eq!(circuit.num_qubits(), 6); // 4 data + 2 syndrome
    }

    #[test]
    fn test_bb84_qec_with_noise() {
        let circuit = bb84_qec_circuit([true, true], Basis::Z, Basis::Z, true);
        assert_eq!(circuit.num_qubits(), 6);
    }

    #[test]
    fn test_pccm_fidelities_no_attack() {
        let (f_ab, f_ae) = pccm_fidelities(0.0);
        assert!((f_ab - 1.0).abs() < 1e-10); // No disturbance
        assert!((f_ae - 0.5).abs() < 1e-10); // Eve gets nothing
    }

    #[test]
    fn test_pccm_fidelities_symmetric() {
        let (f_ab, f_ae) = pccm_fidelities(PI / 4.0);
        assert!((f_ab - f_ae).abs() < 1e-10); // Symmetric cloning
    }

    #[test]
    fn test_pccm_fidelities_full_attack() {
        let (f_ab, f_ae) = pccm_fidelities(PI / 2.0);
        assert!((f_ab - 0.5).abs() < 1e-10); // Bob gets nothing
        assert!((f_ae - 1.0).abs() < 1e-10); // Eve gets perfect copy
    }

    #[test]
    fn test_multi_round_bb84() {
        let circuit = bb84_multi_round(
            &[false, true, true, false],
            &[Basis::Z, Basis::X, Basis::Z, Basis::X],
            &[Basis::Z, Basis::Z, Basis::Z, Basis::X],
            &EveStrategy::None,
        );
        assert_eq!(circuit.num_qubits(), 4);
    }
}
