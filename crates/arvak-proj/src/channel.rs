//! Channel assessment: sin(C/2) commensurability filter and K_eff computation.
//!
//! For each qubit pair (i,j), computes the effective coupling strength
//! K_eff(i,j) = K_ij · |sin(C_ij/2)| where C_ij is the commensurability
//! residual — the minimum distance of ω_i/ω_j from any low-order rational.

use std::f64::consts::PI;

/// Maximum denominator when searching for nearby rationals.
const MAX_RATIONAL_ORDER: u32 = 12;

/// Structural coefficient from pair-counting (Hinderink 2026).
/// Non-perturbative interference effect: independent-channel model
/// overestimates by 5 orders of magnitude at N=6.
pub const DELTA_STRUCT: f64 = 0.06;

/// Pair-counting scaling coefficient β ≈ 0.327 ± 0.020.
/// ln(Γ_c) = a − β · N(N−1)/2.
pub const BETA: f64 = 0.327;

/// Commensurability residual C_ij: minimum distance of ω_i/ω_j
/// from any low-order rational p/q with 1 ≤ q ≤ `MAX_RATIONAL_ORDER`.
#[must_use]
pub fn commensurability_residual(omega_i: f64, omega_j: f64) -> f64 {
    if omega_j.abs() < 1e-15 {
        return PI; // maximally incommensurate if denominator is zero
    }
    let ratio = omega_i / omega_j;
    let mut min_dist = f64::MAX;
    for q in 1..=MAX_RATIONAL_ORDER {
        let qf = f64::from(q);
        let p = (ratio * qf).round();
        if p > 0.0 {
            let dist = (ratio - p / qf).abs();
            if dist < min_dist {
                min_dist = dist;
            }
        }
    }
    min_dist
}

/// The commensurability filter |sin(C_ij/2)|.
///
/// - Near 0: commensurate (Rc regime, KAM torus, stable)
/// - Near 1: incommensurate (Rd regime, chaos, must simulate)
#[must_use]
pub fn sin_c_half(omega_i: f64, omega_j: f64) -> f64 {
    let c = commensurability_residual(omega_i, omega_j);
    (c / 2.0).sin().abs()
}

/// Full Tilde formula Ũ(C, K) for stronger coupling regimes.
/// Ũ = sin(C/2) · exp[-K·exp(-πC/√K) - K·exp(-π(π-C)/√K)]
///
/// At K → 0: Ũ → sin(C/2) (the R-Effekt is exact).
/// At K → ∞: Ũ → 0 everywhere (global chaos).
#[must_use]
pub fn tilde_stability(c: f64, k: f64) -> f64 {
    if k < 1e-12 {
        return (c / 2.0).sin().abs();
    }
    let sqrt_k = k.sqrt();
    let exp1 = (-PI * c / sqrt_k).exp();
    let exp2 = (-PI * (PI - c) / sqrt_k).exp();
    (c / 2.0).sin().abs() * (-k * exp1 - k * exp2).exp()
}

/// Per-gate entangling strength: sin²(θ/2).
/// Each ZZ(θ) gate transfers Pauli weight from k=1 to k=2
/// with this probability.
#[must_use]
pub fn entangling_strength(theta: f64) -> f64 {
    let s = (theta / 2.0).sin();
    s * s
}

/// Effective coupling for a qubit pair.
/// K_eff(i,j) = K_base · |sin(C_ij/2)| · sin²(θ_ij/2)
#[must_use]
pub fn k_eff(omega_i: f64, omega_j: f64, theta: f64, k_base: f64) -> f64 {
    k_base * sin_c_half(omega_i, omega_j) * entangling_strength(theta)
}

/// Channel map: stores per-bond weights for MPS truncation.
///
/// Two modes:
/// - **Dense** (small N, ≤ ~10K): stores sin(C/2) for all N² pairs.
/// - **Sparse** (large N): only stores per-bond weights directly,
///   computed from local neighborhoods. O(N) memory and time.
#[derive(Debug, Clone)]
pub struct ChannelMap {
    n_qubits: usize,
    /// Per-bond weight (length n_qubits - 1).
    bond_weights: Vec<f64>,
    /// Per-bond K_eff (length n_qubits - 1). Reserved for XGBoost features.
    #[allow(dead_code)]
    bond_k_eff: Vec<f64>,
}

impl ChannelMap {
    /// Build dense channel map from frequencies. O(N²) — use for N ≤ ~10K.
    #[must_use]
    pub fn from_frequencies(omegas: &[f64], k_base: f64) -> Self {
        let n = omegas.len();
        let mut bond_weights = Vec::with_capacity(n.saturating_sub(1));
        let mut bond_k_eff = Vec::with_capacity(n.saturating_sub(1));

        for bond in 0..n.saturating_sub(1) {
            let mut total = 0.0;
            let mut weight_sum = 0.0;
            for a in 0..=bond {
                for b in (bond + 1)..n {
                    let dist = (bond - a + b - bond - 1) as f64;
                    let w = (-0.5 * dist).exp();
                    total += sin_c_half(omegas[a], omegas[b]) * w;
                    weight_sum += w;
                }
            }
            let bw = if weight_sum > 1e-15 {
                total / weight_sum
            } else {
                0.0
            };
            bond_weights.push(bw);
            bond_k_eff.push(k_base * bw);
        }

        Self {
            n_qubits: n,
            bond_weights,
            bond_k_eff,
        }
    }

    /// Build sparse channel map from frequencies. O(N × radius²) memory and time.
    ///
    /// Only considers pairs within `radius` bonds of each cut.
    /// For nearest-neighbor circuits, radius=3-5 captures >99% of the
    /// entanglement prediction at a fraction of the cost.
    #[must_use]
    pub fn from_frequencies_sparse(omegas: &[f64], k_base: f64, radius: usize) -> Self {
        let n = omegas.len();
        let mut bond_weights = Vec::with_capacity(n.saturating_sub(1));
        let mut bond_k_eff = Vec::with_capacity(n.saturating_sub(1));

        for bond in 0..n.saturating_sub(1) {
            let a_start = bond.saturating_sub(radius);
            let b_end = (bond + 2 + radius).min(n);

            let mut total = 0.0;
            let mut weight_sum = 0.0;
            for a in a_start..=bond {
                for b in (bond + 1)..b_end {
                    let dist = (bond - a + b - bond - 1) as f64;
                    let w = (-0.5 * dist).exp();
                    total += sin_c_half(omegas[a], omegas[b]) * w;
                    weight_sum += w;
                }
            }
            let bw = if weight_sum > 1e-15 {
                total / weight_sum
            } else {
                0.0
            };
            bond_weights.push(bw);
            bond_k_eff.push(k_base * bw);
        }

        Self {
            n_qubits: n,
            bond_weights,
            bond_k_eff,
        }
    }

    /// Per-MPS-bond weight (pre-computed).
    #[must_use]
    pub fn bond_weight(&self, bond: usize) -> f64 {
        self.bond_weights[bond]
    }

    /// Compute adaptive bond dimensions from sin(C/2) weights.
    #[must_use]
    pub fn adaptive_bond_dims(&self, chi_max: usize) -> Vec<usize> {
        let n = self.n_qubits;
        if n < 2 {
            return vec![];
        }

        let w_sum: f64 = self.bond_weights.iter().sum();
        if w_sum < 1e-15 {
            return vec![chi_max; n - 1];
        }

        let n_bonds = n - 1;
        self.bond_weights
            .iter()
            .map(|&w| {
                let frac = w / w_sum;
                let chi = chi_max as f64 * (frac * n_bonds as f64).sqrt();
                chi.round().max(2.0) as usize
            })
            .collect()
    }

    /// Number of qubit pairs N(N-1)/2.
    #[must_use]
    pub fn pair_count(&self) -> usize {
        self.n_qubits * (self.n_qubits - 1) / 2
    }

    /// Number of qubits.
    #[must_use]
    pub fn n_qubits(&self) -> usize {
        self.n_qubits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_commensurate_pair() {
        // 1:2 ratio → C ≈ 0 → sin(C/2) ≈ 0
        let sc = sin_c_half(1.0, 2.0);
        assert!(sc < 0.01, "1:2 should be commensurate, got {sc}");
    }

    #[test]
    fn irrational_pair() {
        // √2 : √7 → no low-order rational nearby
        let sc = sin_c_half(2.0_f64.sqrt(), 7.0_f64.sqrt());
        assert!(
            sc > 0.001,
            "sqrt(2):sqrt(7) should have nonzero sin(C/2), got {sc}"
        );
    }

    #[test]
    fn pair_counting() {
        let omegas = vec![1.0, 2.0, 3.0, 4.0];
        let cm = ChannelMap::from_frequencies(&omegas, 1.0);
        assert_eq!(cm.pair_count(), 6); // 4*3/2
    }

    #[test]
    fn bond_weights_commensurate_vs_mixed() {
        // All commensurate: bond weights should be near zero
        let cm_comm = ChannelMap::from_frequencies(&[1.0, 2.0, 3.0, 4.0], 1.0);
        let w_comm = cm_comm.bond_weight(1);

        // Mixed: some bonds should be heavier
        let cm_mixed =
            ChannelMap::from_frequencies(&[1.0, 2.0, 7.0_f64.sqrt(), 11.0_f64.sqrt()], 1.0);
        let w_mixed = cm_mixed.bond_weight(1);

        assert!(
            w_mixed > w_comm,
            "mixed should have heavier bond weight: {w_mixed} vs {w_comm}"
        );
    }

    #[test]
    fn adaptive_dims_give_more_to_heavy_bonds() {
        let omegas = vec![1.0, 2.0, 7.0_f64.sqrt(), 11.0_f64.sqrt()];
        let cm = ChannelMap::from_frequencies(&omegas, 1.0);
        let dims = cm.adaptive_bond_dims(32);
        assert_eq!(dims.len(), 3);
        // Bond 1 (between commensurate and irrational) should get more
        // than bond 0 (between two commensurate qubits)
        assert!(dims[1] >= dims[0], "bond 1 should get >= bond 0: {dims:?}");
    }

    #[test]
    fn tilde_reduces_to_sin_c_at_zero_k() {
        let c = 0.5;
        let t = tilde_stability(c, 0.0);
        let s = (c / 2.0).sin().abs();
        assert!((t - s).abs() < 1e-12);
    }

    #[test]
    fn entangling_strength_bounds() {
        assert!((entangling_strength(0.0)).abs() < 1e-12);
        assert!((entangling_strength(PI) - 1.0).abs() < 1e-12);
    }
}
