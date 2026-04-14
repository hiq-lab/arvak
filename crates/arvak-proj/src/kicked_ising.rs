//! 1D Floquet Kicked Ising Model — circuit builder and reference simulator.
//!
//! Implements the Trotterised kicked Ising chain
//!
//! ```text
//! H = J · Σ_i Z_i Z_{i+1} + h_x · Σ_i X_i + h_z · Σ_i Z_i
//! ```
//!
//! evolved as a sequence of Floquet steps
//!
//! ```text
//! U_step = exp(-i dt h_x Σ X_i) · exp(-i dt h_z Σ Z_i) · exp(-i dt J Σ Z_i Z_{i+1})
//! ```
//!
//! At the **self-dual point** `J = h_x = π/4` and `h_z = 0` (Bertini-Kos-Prosen,
//! PRX 9, 021033, 2019; Kos et al. PRL 121, 264101, 2018), this model has
//! analytic spectral form factor agreeing with random matrix theory and is
//! the canonical 1D Floquet integrability benchmark.
//!
//! This module provides:
//! - `KimParams` — model parameters
//! - `apply_kim_step` — one Floquet step on an MPS via the existing
//!   `apply_two_qubit_layer_parallel` machinery
//! - `reference_kim_run` — independent dense statevector simulator for
//!   ground-truth comparison at small N (≤ ~24)

use num_complex::Complex64;

use crate::error::Result;
use crate::mps::{self, Mps};

type C = Complex64;

/// Parameters for the 1D Floquet kicked Ising model.
#[derive(Debug, Clone, Copy)]
pub struct KimParams {
    /// Nearest-neighbour ZZ coupling strength.
    pub j: f64,
    /// Transverse field strength (X term).
    pub h_x: f64,
    /// Longitudinal field strength (Z term). Set to 0 for the self-dual point.
    pub h_z: f64,
    /// Trotter step size.
    pub dt: f64,
}

impl KimParams {
    /// Self-dual point of the 1D Floquet kicked Ising chain
    /// (Bertini-Kos-Prosen): `J = h_x = π/4`, `h_z = 0`, `dt = 1`.
    #[must_use]
    pub fn self_dual() -> Self {
        let pi_over_4 = std::f64::consts::FRAC_PI_4;
        Self {
            j: pi_over_4,
            h_x: pi_over_4,
            h_z: 0.0,
            dt: 1.0,
        }
    }
}

/// Apply one Floquet step of a **disordered** kicked Ising chain where
/// each site has its own transverse field strength `h_x_per_site[i]`.
/// All other parameters (J, h_z, dt) come from `params`.
///
/// This is the natural test bed for the Jacobian-allocator: the per-site
/// disorder breaks translation invariance, and inhomogeneous χ allocation
/// can exploit the local entanglement structure.
pub fn apply_kim_step_disordered(
    mps: &mut Mps,
    params: KimParams,
    h_x_per_site: &[f64],
    chi_per_bond: &[usize],
) -> Result<()> {
    let n = mps.n_qubits;
    assert_eq!(h_x_per_site.len(), n, "h_x_per_site must have length n");

    // ZZ entangling layer (homogeneous J)
    let zz_angle = params.j * params.dt;
    let zz_angles: Vec<f64> = vec![zz_angle; n.saturating_sub(1)];
    mps.apply_two_qubit_layer_parallel(mps::zz(0.0), chi_per_bond, &zz_angles)?;

    // Optional global RZ (homogeneous h_z)
    if params.h_z != 0.0 {
        let rz_angle = 2.0 * params.h_z * params.dt;
        let rz_layer: Vec<_> = (0..n).map(|_| mps::rz(rz_angle)).collect();
        mps.apply_single_layer(&rz_layer);
    }

    // Site-dependent RX kick
    let rx_layer: Vec<_> = (0..n)
        .map(|i| mps::rx(2.0 * h_x_per_site[i] * params.dt))
        .collect();
    mps.apply_single_layer(&rx_layer);

    Ok(())
}

/// Apply one Floquet step to an MPS using the supplied per-bond χ profile.
///
/// The step implements `U = U_x · U_z · U_zz` where each factor is the exact
/// exponential of the corresponding Hamiltonian term times `dt`:
///
/// - `U_zz = exp(-i J dt Σ Z_i Z_{i+1})` — ZZ entangling layer
/// - `U_z  = exp(-i h_z dt Σ Z_i)`        — optional longitudinal kick
/// - `U_x  = exp(-i h_x dt Σ X_i)`        — transverse kick
///
/// Note on conventions: `mps::zz(θ) = exp(-i θ Z⊗Z)` (no factor of two), so
/// the angle passed in is `J·dt` directly. `mps::rx(θ) = exp(-i θ/2 X)`
/// follows the standard convention, so the angle passed in is `2·h_x·dt`.
pub fn apply_kim_step(mps: &mut Mps, params: KimParams, chi_per_bond: &[usize]) -> Result<()> {
    let n = mps.n_qubits;

    // ── 1. ZZ entangling layer ──────────────────────────────────────────
    let zz_angle = params.j * params.dt;
    let zz_angles: Vec<f64> = vec![zz_angle; n.saturating_sub(1)];
    mps.apply_two_qubit_layer_parallel(mps::zz(0.0), chi_per_bond, &zz_angles)?;

    // ── 2. Global RZ kick (only if h_z != 0) ────────────────────────────
    if params.h_z != 0.0 {
        let rz_angle = 2.0 * params.h_z * params.dt;
        let rz_layer: Vec<_> = (0..n).map(|_| mps::rz(rz_angle)).collect();
        mps.apply_single_layer(&rz_layer);
    }

    // ── 3. Global RX kick ───────────────────────────────────────────────
    let rx_angle = 2.0 * params.h_x * params.dt;
    let rx_layer: Vec<_> = (0..n).map(|_| mps::rx(rx_angle)).collect();
    mps.apply_single_layer(&rx_layer);

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Independent reference statevector simulator
// ─────────────────────────────────────────────────────────────────────────────

/// Apply a single-qubit gate (`2×2` matrix) to qubit `target` of a dense
/// statevector. Convention: qubit 0 is the **most significant** bit, matching
/// arvak-proj's `to_statevector` ordering.
fn apply_single_dense(psi: &mut [C], n: usize, target: usize, gate: &[[C; 2]; 2]) {
    let stride = 1_usize << (n - 1 - target);
    let block = stride * 2;
    let dim = 1_usize << n;
    let mut i = 0;
    while i < dim {
        for k in 0..stride {
            let i0 = i + k;
            let i1 = i0 + stride;
            let a0 = psi[i0];
            let a1 = psi[i1];
            psi[i0] = gate[0][0] * a0 + gate[0][1] * a1;
            psi[i1] = gate[1][0] * a0 + gate[1][1] * a1;
        }
        i += block;
    }
}

/// Apply a diagonal ZZ rotation `exp(-i θ Z⊗Z)` to qubits `q` and `q+1`
/// of a dense statevector. Convention: qubit 0 is MSB. Matches the
/// `mps::zz(θ)` convention (no factor of two).
fn apply_zz_dense(psi: &mut [C], n: usize, q: usize, theta: f64) {
    let phase_pos = C::new(theta.cos(), -theta.sin()); // exp(-i θ)  for ZZ eigenvalue +1
    let phase_neg = C::new(theta.cos(), theta.sin()); //  exp(+i θ)  for ZZ eigenvalue −1
    let dim = 1_usize << n;
    let bit_q = n - 1 - q;
    let bit_qp1 = n - 1 - (q + 1);
    for (idx, amp) in psi.iter_mut().enumerate().take(dim) {
        let b_q = (idx >> bit_q) & 1;
        let b_qp1 = (idx >> bit_qp1) & 1;
        // ZZ eigenvalue: +1 if both spins equal, -1 otherwise
        let same = b_q == b_qp1;
        *amp *= if same { phase_pos } else { phase_neg };
    }
}

/// Run the kicked Ising circuit on a dense statevector starting from `|0…0⟩`
/// and return `⟨Z_q(t)⟩` for every qubit `q ∈ 0..n` at every Trotter step
/// `t ∈ 0..=n_steps` (so the returned matrix is `(n_steps+1) × n`).
///
/// Index `t=0` is the initial state. Independent of the MPS code path —
/// used as ground truth in validation tests.
///
/// **Only feasible for `n ≤ ~24`** (`2^24` = 16M complex amplitudes ≈ 256 MB).
#[must_use]
pub fn reference_kim_run(n: usize, params: KimParams, n_steps: usize) -> Vec<Vec<f64>> {
    let dim = 1_usize << n;
    let mut psi = vec![C::new(0.0, 0.0); dim];
    psi[0] = C::new(1.0, 0.0);

    let zz_angle = params.j * params.dt;
    let rx_angle = 2.0 * params.h_x * params.dt;
    let rz_angle = 2.0 * params.h_z * params.dt;

    let rx_gate = single_qubit_rx(rx_angle);
    let rz_gate = single_qubit_rz(rz_angle);

    let mut history = Vec::with_capacity(n_steps + 1);
    history.push(measure_all_z(&psi, n));

    for _step in 0..n_steps {
        // ZZ entangling layer
        for q in 0..n - 1 {
            apply_zz_dense(&mut psi, n, q, zz_angle);
        }
        // Optional RZ kick
        if params.h_z != 0.0 {
            for q in 0..n {
                apply_single_dense(&mut psi, n, q, &rz_gate);
            }
        }
        // RX kick
        for q in 0..n {
            apply_single_dense(&mut psi, n, q, &rx_gate);
        }
        history.push(measure_all_z(&psi, n));
    }

    history
}

/// Disordered variant of `reference_kim_run`: site-dependent transverse
/// field `h_x_per_site[i]`, otherwise identical conventions.
#[must_use]
pub fn reference_kim_run_disordered(
    n: usize,
    params: KimParams,
    h_x_per_site: &[f64],
    n_steps: usize,
) -> Vec<Vec<f64>> {
    assert_eq!(h_x_per_site.len(), n);
    let dim = 1_usize << n;
    let mut psi = vec![C::new(0.0, 0.0); dim];
    psi[0] = C::new(1.0, 0.0);

    let zz_angle = params.j * params.dt;
    let rz_angle = 2.0 * params.h_z * params.dt;
    let rz_gate = single_qubit_rz(rz_angle);
    let rx_gates: Vec<[[C; 2]; 2]> = h_x_per_site
        .iter()
        .map(|&hx| single_qubit_rx(2.0 * hx * params.dt))
        .collect();

    let mut history = Vec::with_capacity(n_steps + 1);
    history.push(measure_all_z(&psi, n));

    for _step in 0..n_steps {
        for q in 0..n - 1 {
            apply_zz_dense(&mut psi, n, q, zz_angle);
        }
        if params.h_z != 0.0 {
            for q in 0..n {
                apply_single_dense(&mut psi, n, q, &rz_gate);
            }
        }
        for (q, rx_gate) in rx_gates.iter().enumerate().take(n) {
            apply_single_dense(&mut psi, n, q, rx_gate);
        }
        history.push(measure_all_z(&psi, n));
    }

    history
}

/// Compute `⟨Z_q⟩` for every qubit from a dense (normalised) statevector.
/// Convention: qubit 0 is MSB.
fn measure_all_z(psi: &[C], n: usize) -> Vec<f64> {
    let dim = 1_usize << n;
    let mut z = vec![0.0_f64; n];
    for (q, z_q) in z.iter_mut().enumerate().take(n) {
        let bit = n - 1 - q;
        let mut acc = 0.0_f64;
        for (idx, amp) in psi.iter().enumerate().take(dim) {
            let p = amp.norm_sqr();
            if (idx >> bit) & 1 == 0 {
                acc += p;
            } else {
                acc -= p;
            }
        }
        *z_q = acc;
    }
    z
}

fn single_qubit_rx(theta: f64) -> [[C; 2]; 2] {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    [
        [C::new(c, 0.0), C::new(0.0, -s)],
        [C::new(0.0, -s), C::new(c, 0.0)],
    ]
}

fn single_qubit_rz(theta: f64) -> [[C; 2]; 2] {
    let half = theta / 2.0;
    let phase_neg = C::new(half.cos(), -half.sin());
    let phase_pos = C::new(half.cos(), half.sin());
    [[phase_neg, C::new(0.0, 0.0)], [C::new(0.0, 0.0), phase_pos]]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference simulator must keep the state normalised at every step.
    #[test]
    fn reference_preserves_norm() {
        let params = KimParams::self_dual();
        let n = 6;
        let n_steps = 10;
        let dim = 1_usize << n;

        let mut psi = vec![C::new(0.0, 0.0); dim];
        psi[0] = C::new(1.0, 0.0);

        let zz_angle = params.j * params.dt;
        let rx_angle = 2.0 * params.h_x * params.dt;
        let rx_gate = single_qubit_rx(rx_angle);

        for _ in 0..n_steps {
            for q in 0..n - 1 {
                apply_zz_dense(&mut psi, n, q, zz_angle);
            }
            for q in 0..n {
                apply_single_dense(&mut psi, n, q, &rx_gate);
            }
            let nrm: f64 = psi.iter().map(num_complex::Complex64::norm_sqr).sum();
            assert!((nrm - 1.0).abs() < 1e-12, "norm drifted: {nrm}");
        }
    }

    /// Initial product state |0…0⟩ has ⟨Z_q⟩ = +1 for every q.
    #[test]
    fn initial_state_all_up() {
        let history = reference_kim_run(5, KimParams::self_dual(), 0);
        assert_eq!(history.len(), 1);
        for &z in &history[0] {
            assert!((z - 1.0).abs() < 1e-12);
        }
    }

    /// Self-dual KIM run on N=6 should produce non-trivial dynamics:
    /// the bulk Z-expectation must drop below the initial value within
    /// a few Trotter steps.
    #[test]
    fn self_dual_dynamics_nontrivial() {
        let history = reference_kim_run(6, KimParams::self_dual(), 4);
        assert_eq!(history.len(), 5);
        // Initial all-up
        for &z in &history[0] {
            assert!((z - 1.0).abs() < 1e-12);
        }
        // After a few steps, central qubit should have moved
        let z_central_final = history[4][3];
        assert!(
            z_central_final.abs() < 0.99,
            "central Z did not evolve: {z_central_final}"
        );
    }

    /// MPS evolution at large χ must reproduce the dense reference.
    #[test]
    fn mps_at_max_chi_matches_reference() {
        let n = 8;
        let n_steps = 6;
        let params = KimParams::self_dual();

        // Reference
        let ref_history = reference_kim_run(n, params, n_steps);

        // MPS at χ_max = 2^(n/2) = 16, which is exact for an 8-qubit state
        let chi_max = 1_usize << (n / 2);
        let chi_per_bond = vec![chi_max; n - 1];

        let mut mps = Mps::new(n);
        for (step, ref_step) in ref_history.iter().enumerate().take(n_steps + 1).skip(1) {
            apply_kim_step(&mut mps, params, &chi_per_bond).unwrap();
            for (q, &ref_z) in ref_step.iter().enumerate().take(n) {
                let mps_z = mps.expectation_z(q);
                let diff = (mps_z - ref_z).abs();
                assert!(
                    diff < 1e-10,
                    "step {step} q {q}: mps={mps_z} ref={ref_z} diff={diff}"
                );
            }
        }
    }
}
