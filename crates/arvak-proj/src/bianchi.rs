//! Bianchi-violation diagnostic for MPS in balanced canonical form.
//!
//! After SVD truncation, the canonical-consistency condition (the
//! tensor-network analogue of the Bianchi identity in differential geometry —
//! a structural constraint from `GL(χ)` gauge invariance at each bond,
//! Noether's second theorem) is violated. This module measures that violation
//! per bond.
//!
//! # What this module is, and what it isn't
//!
//! This module provides only the **diagnostic**: `bianchi_violation`,
//! `bianchi_profile`, `total_bianchi_violation`, and `transfer_matrix`.
//!
//! It does **not** provide a Bianchi-projection truncation correction.
//! Earlier work (Phases 3/3b — Λ-only and rotation-based single-bond
//! projections gated behind sin(C/2)) was implemented and validated against
//! the kicked-Ising and Heisenberg-XXZ benchmarks. Both failed to reduce
//! discarded weight, and the empirical correlation between sin(C/2) and the
//! actual truncation error on QKR / Bethe-ansatz circuits was negative
//! (Spearman ≈ −0.2). The journey, the math fix that made the diagnostic
//! correct, and the negative results are documented in
//! `crates/arvak-proj/BIANCHI_JOURNEY.md`.
//!
//! The truncation-prediction problem is now solved by
//! `finite_difference_jacobian` (Spearman ≈ 0.65–0.71 against measured
//! discarded weight), which is what arvak-proj uses in production.
//!
//! References:
//! - Evenbly, PRB 98, 085155 (2018)
//! - Tindall et al., PRX Quantum 5, 010308 (2024)
//! - Zauner-Stauber et al., SciPost Phys. Core 4, 004 (2021)

use num_complex::Complex64;

use crate::mps::{Mps, SiteTensor};

type C = Complex64;

// ─────────────────────────────────────────────────────────────────────────────
// Transfer matrix
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the right-environment transfer matrix
///
/// ```text
/// T_i[α, α'] = Σ_{σ, β} A_i[α, σ, β]^* · A_i[α', σ, β]
/// ```
///
/// for site i.
///
/// Returns a `left_dim × left_dim` row-major `Vec<f64>` (real entries — the
/// transfer matrix is Hermitian and we only need its real part for the
/// diagnostic).
///
/// Cost: O(χ_left² · d · χ_right) ≈ O(χ³) for square bonds.
#[must_use]
pub fn transfer_matrix(site: &SiteTensor) -> Vec<f64> {
    let ld = site.left_dim;
    let rd = site.right_dim;
    let mut t = vec![0.0_f64; ld * ld];

    for a in 0..ld {
        for ap in 0..ld {
            let mut acc = C::new(0.0, 0.0);
            for b in 0..rd {
                let m0a = site.m0[a * rd + b];
                let m0ap = site.m0[ap * rd + b];
                acc += m0a.conj() * m0ap;

                let m1a = site.m1[a * rd + b];
                let m1ap = site.m1[ap * rd + b];
                acc += m1a.conj() * m1ap;
            }
            t[a * ld + ap] = acc.re;
        }
    }
    t
}

// ─────────────────────────────────────────────────────────────────────────────
// Bianchi violation
// ─────────────────────────────────────────────────────────────────────────────

/// Compute the Bianchi violation in the **balanced canonical form** used by
/// arvak-proj (where the SVD distributes √S onto BOTH adjacent sites).
///
/// In this gauge each site stores
///
/// ```text
/// A^σ[α,β] = U[α·d+σ, β] · √λ_right[β]
/// ```
///
/// and the consistency condition (derived from the SVD identity
/// `M = U·diag(λ)·V†` via Parseval on V) is:
///
/// ```text
/// Σ_{σ,β} |A^σ[α,β]|² · λ_right[β] = λ_left[α] · δ_{α,α}
/// ```
///
/// Note the **single power** of λ on both sides — NOT λ². Half of each √λ is
/// absorbed into the site tensor itself, so the implicit "weight" of one
/// factor of λ is already inside `|A|²`. (An earlier version of this module
/// used λ² and produced false-positive violations on Bell/GHZ states; see
/// `BIANCHI_JOURNEY.md` for the derivation.)
///
/// The full Bianchi violation includes the off-diagonal Schmidt-frame
/// alignment check:
///
/// ```text
/// R[α,α'] = Σ_{σ,β} A^σ[α,β]^* · A^σ[α',β] · λ_right[β]
/// B_i² = Σ_{α,α'} |R[α,α'] − λ_left[α] · δ_{α,α'}|²
/// ```
///
/// `lambda_left` is the diagonal `Λ_{i-1}` (length must equal `site.left_dim`).
/// `lambda_right` is the diagonal `Λ_i` (length must equal `site.right_dim`).
///
/// For the leftmost site, pass `&[1.0]` (trivial environment).
/// For the rightmost site, pass `&[1.0]`.
///
/// Returns `B_i` as a non-negative scalar. **Zero on a properly canonical
/// MPS** (verified analytically on Bell state and GHZ).
#[must_use]
pub fn bianchi_violation(site: &SiteTensor, lambda_left: &[f64], lambda_right: &[f64]) -> f64 {
    let ld = site.left_dim;
    let rd = site.right_dim;

    if lambda_left.len() != ld || lambda_right.len() != rd {
        return 0.0;
    }

    let mut violation_sq = 0.0_f64;

    for (a, &ll_a) in lambda_left.iter().enumerate().take(ld) {
        for ap in 0..ld {
            let mut acc = C::new(0.0, 0.0);
            for (b, &lr) in lambda_right.iter().enumerate().take(rd) {
                let m0a = site.m0[a * rd + b];
                let m0ap = site.m0[ap * rd + b];
                acc += m0a.conj() * m0ap * lr;

                let m1a = site.m1[a * rd + b];
                let m1ap = site.m1[ap * rd + b];
                acc += m1a.conj() * m1ap * lr;
            }

            let target = if a == ap { ll_a } else { 0.0 };

            let dr = acc.re - target;
            let di = acc.im;
            violation_sq += dr * dr + di * di;
        }
    }

    violation_sq.sqrt()
}

/// Compute `B_i` for every bond in the MPS.
///
/// Returns a `Vec<f64>` of length `n_qubits`. Sites without populated `lambda`
/// at neighbouring bonds (e.g. fresh product states, or boundary sites) get a
/// `0.0` entry — no violation can be diagnosed there.
#[must_use]
pub fn bianchi_profile(mps: &Mps) -> Vec<f64> {
    let n = mps.n_qubits;
    let mut profile = Vec::with_capacity(n);

    let trivial = vec![1.0_f64];

    for i in 0..n {
        let site = &mps.sites[i];

        let lambda_left: &[f64] = if i == 0 {
            &trivial
        } else {
            mps.sites[i - 1].lambda.as_deref().unwrap_or(&trivial)
        };

        let lambda_right: &[f64] = if i == n - 1 {
            &trivial
        } else {
            site.lambda.as_deref().unwrap_or(&trivial)
        };

        let b = bianchi_violation(site, lambda_left, lambda_right);
        profile.push(b);
    }

    profile
}

/// Sum (in quadrature) of Bianchi violations across all bonds. A scalar
/// diagnostic for the entire MPS.
#[must_use]
pub fn total_bianchi_violation(mps: &Mps) -> f64 {
    bianchi_profile(mps)
        .iter()
        .map(|b| b * b)
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mps::{self, Mps};

    #[test]
    fn product_state_has_zero_violation() {
        let mps = Mps::new(5);
        let profile = bianchi_profile(&mps);
        assert_eq!(profile.len(), 5);
        for (i, b) in profile.iter().enumerate() {
            assert!(*b < 1e-10, "site {i} has non-zero violation {b}");
        }
    }

    #[test]
    fn transfer_matrix_of_product_state_is_identity() {
        let mps = Mps::new(3);
        let t = transfer_matrix(&mps.sites[0]);
        assert_eq!(t.len(), 1);
        assert!((t[0] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn bell_state_diagnostic_works() {
        let mut mps = Mps::new(2);
        mps.apply_single(0, mps::h());
        mps.apply_two_qubit(0, mps::cx(), 4).unwrap();

        let profile = bianchi_profile(&mps);
        assert_eq!(profile.len(), 2);

        for (i, b) in profile.iter().enumerate() {
            assert!(b.is_finite(), "site {i} produced non-finite B_i={b}");
            assert!(*b >= 0.0);
        }
    }

    #[test]
    fn ghz_state_finite_violation() {
        let mut mps = Mps::new(4);
        mps.apply_single(0, mps::h());
        for i in 0..3 {
            mps.apply_two_qubit(i, mps::cx(), 8).unwrap();
        }
        let profile = bianchi_profile(&mps);
        for (i, b) in profile.iter().enumerate() {
            assert!(b.is_finite(), "site {i}: B={b}");
            assert!(*b >= 0.0);
        }
    }

    #[test]
    fn total_violation_is_nonneg() {
        let mut mps = Mps::new(3);
        mps.apply_single(0, mps::h());
        mps.apply_two_qubit(0, mps::cx(), 4).unwrap();
        mps.apply_two_qubit(1, mps::cx(), 4).unwrap();
        let total = total_bianchi_violation(&mps);
        assert!(total >= 0.0);
        assert!(total.is_finite());
    }
}
