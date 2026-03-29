//! Reassembly: combine stable and volatile results with interference correction.

use crate::channel::{BETA, DELTA_STRUCT};

/// Projected result with error bounds.
#[derive(Debug, Clone)]
pub struct ProjectedResult {
    /// Number of qubits in the original system.
    pub n_qubits: usize,
    /// Number of volatile qubits actually simulated.
    pub n_volatile: usize,
    /// Estimated fidelity of the projection.
    pub estimated_fidelity: f64,
    /// Sum of discarded singular values (truncation error).
    pub truncation_residual: f64,
    /// Pair-counting scaling prediction for ln(Γ_c).
    pub ln_gamma_c: f64,
}

/// Estimate projection fidelity from truncation residuals and pair-counting scaling.
///
/// Uses δ_struct ≈ 0.06 and β ≈ 0.327 from the pair-counting paper.
/// The truncation residual (sum of discarded singular values) gives a
/// direct bound on the state error: ‖ψ_exact − ψ_mps‖ ≤ Σ_bonds √(Σ s_discarded²).
#[must_use]
pub fn estimate_fidelity(
    n_qubits: usize,
    n_volatile: usize,
    truncation_residual: f64,
) -> ProjectedResult {
    let pairs = n_qubits * (n_qubits - 1) / 2;

    // Pair-counting scaling: ln(Γ_c) = a - β · N(N-1)/2
    // Using a ≈ -4.003 from the 5-point fit
    let ln_gamma_c = -4.003 - BETA * pairs as f64;

    // Fidelity estimate: F ≈ 1 - truncation_error²
    // The truncation residual bounds the L2 norm of the discarded part
    let fidelity = (1.0 - truncation_residual * truncation_residual).max(0.0);

    // Apply δ_struct correction: the actual interference effect
    // reduces the error by δ_struct per pair relative to independent channels
    let corrected_fidelity = fidelity + DELTA_STRUCT * (1.0 - fidelity);

    ProjectedResult {
        n_qubits,
        n_volatile,
        estimated_fidelity: corrected_fidelity.min(1.0),
        truncation_residual,
        ln_gamma_c,
    }
}

/// Scale a projection from n_qubits to a larger target system.
/// Uses pair-counting scaling to extrapolate.
#[must_use]
pub fn scale_projection(base: &ProjectedResult, target_qubits: usize) -> ProjectedResult {
    let base_pairs = base.n_qubits * (base.n_qubits - 1) / 2;
    let target_pairs = target_qubits * (target_qubits - 1) / 2;
    let delta_pairs = target_pairs - base_pairs;

    // Each additional pair reduces threshold by exp(-β)
    let scaling_factor = (-BETA * delta_pairs as f64).exp();

    let ln_gamma_c = -4.003 - BETA * target_pairs as f64;

    // Extrapolated fidelity: scales down with more pairs
    let extrapolated_fidelity = base.estimated_fidelity * scaling_factor;

    ProjectedResult {
        n_qubits: target_qubits,
        n_volatile: base.n_volatile, // conservative: same volatile count
        estimated_fidelity: extrapolated_fidelity.clamp(0.0, 1.0),
        truncation_residual: base.truncation_residual / scaling_factor,
        ln_gamma_c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_simulation() {
        let r = estimate_fidelity(10, 4, 0.0);
        assert!((r.estimated_fidelity - 1.0).abs() < 1e-12);
    }

    #[test]
    fn scaling_reduces_fidelity() {
        let base = estimate_fidelity(10, 4, 0.01);
        let scaled = scale_projection(&base, 100);
        assert!(scaled.estimated_fidelity < base.estimated_fidelity);
        assert_eq!(scaled.n_qubits, 100);
    }

    #[test]
    fn pair_counting_scaling() {
        let r10 = estimate_fidelity(10, 4, 0.01);
        let r20 = estimate_fidelity(20, 8, 0.01);
        // More pairs → lower Γ_c
        assert!(r20.ln_gamma_c < r10.ln_gamma_c);
    }
}
