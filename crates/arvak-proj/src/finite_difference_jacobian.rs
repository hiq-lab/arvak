//! Finite-difference Jacobian for input → bond-observable sensitivity.
//!
//! Provides a clean public API for computing
//!
//! ```text
//!   J[k, i] = ∂(observable_k) / ∂(input_i)
//! ```
//!
//! where the simulation is parametrized by an N-vector of input parameters
//! (e.g. site frequencies or gate angles) and produces an M-vector of
//! per-bond observables (e.g. discarded weight, entropy, fidelity).
//!
//! This is the **operationally faithful** finite-difference equivalent of
//! forward-mode automatic differentiation: same numerical result up to
//! O(δ²) discretization error, same asymptotic cost (O(N · primal_cost)),
//! but no symbolic differentiation through SVD.
//!
//! # Usage
//!
//! ```ignore
//! use arvak_proj::jacobian::{InputJacobian, JacobianConfig};
//!
//! let factory = |inputs: &[f64]| {
//!     let mut mps = build_mps_from_inputs(inputs);
//!     mps
//! };
//! let observe = |mps: &Mps| -> Vec<f64> {
//!     mps.discarded_weight_per_bond.clone()
//! };
//!
//! let cfg = JacobianConfig::default();
//! let j = InputJacobian::compute(&base_inputs, factory, observe, &cfg);
//! // j.matrix[k][i] = ∂(observable_k)/∂(input_i)
//! ```
//!
//! # Cost
//!
//! For N inputs and primal cost C, the Jacobian costs `2 N C` (central
//! differences). Each row of the matrix is computed from the same M
//! observables but with different perturbed inputs.
//!
//! # Adaptive bond dimension via Jacobian
//!
//! After computing J, the per-row participation ratio
//!
//! ```text
//!   PR[k] = (Σ_i |J[k,i]|)² / Σ_i J[k,i]²
//! ```
//!
//! predicts which bonds need higher χ. Bonds with **high** PR are
//! "globally determined" — many inputs contribute roughly equally — and
//! tend to accumulate the most discarded weight. Bonds with **low** PR
//! depend on a few dominant inputs and are easier to compress.
//!
//! See `participation_ratio` and `chi_allocation_from_jacobian`.

/// Configuration for Jacobian computation.
#[derive(Debug, Clone, Copy)]
pub struct JacobianConfig {
    /// Step size for the central difference. Default: 1e-3.
    /// Sweet spot for f64 is around √eps ≈ 1.5e-8 but for noisy MPS
    /// observables a larger step (1e-3 to 1e-2) is more stable.
    pub delta: f64,
    /// If true, sample only every Nth input direction (for very large N).
    /// 1 = every input, 2 = every other, etc.
    pub stride: usize,
}

impl Default for JacobianConfig {
    fn default() -> Self {
        Self {
            delta: 1e-3,
            stride: 1,
        }
    }
}

/// Result of a Jacobian computation: a dense `Vec<Vec<f64>>` plus metadata.
#[derive(Debug, Clone)]
pub struct InputJacobian {
    /// `matrix[k][i] = ∂(observable_k)/∂(input_i)`
    /// Shape: `[n_outputs][n_inputs]`. May contain NaN if `stride > 1`
    /// and the column was skipped.
    pub matrix: Vec<Vec<f64>>,
    pub n_inputs: usize,
    pub n_outputs: usize,
    pub delta: f64,
    pub stride: usize,
}

impl InputJacobian {
    /// Compute the Jacobian via central finite differences.
    ///
    /// `factory(inputs)` builds and runs a simulation, returning the final
    /// state from which `observe` extracts an output vector.
    ///
    /// `base_inputs` is the unperturbed parameter vector. The Jacobian is
    /// evaluated at this point.
    pub fn compute<F, S, O>(
        base_inputs: &[f64],
        mut factory: F,
        mut observe: O,
        cfg: &JacobianConfig,
    ) -> Self
    where
        F: FnMut(&[f64]) -> S,
        O: FnMut(&S) -> Vec<f64>,
    {
        let n_inputs = base_inputs.len();
        // Probe to learn n_outputs
        let probe = factory(base_inputs);
        let probe_obs = observe(&probe);
        let n_outputs = probe_obs.len();

        // matrix[k][i]
        let mut matrix = vec![vec![f64::NAN; n_inputs]; n_outputs];

        let stride = cfg.stride.max(1);
        let delta = cfg.delta;
        let inv_two_delta = 1.0 / (2.0 * delta);

        let mut perturbed = base_inputs.to_vec();

        for i in (0..n_inputs).step_by(stride) {
            // +δ
            let saved = perturbed[i];
            perturbed[i] = saved + delta;
            let s_plus = factory(&perturbed);
            let obs_plus = observe(&s_plus);

            // -δ
            perturbed[i] = saved - delta;
            let s_minus = factory(&perturbed);
            let obs_minus = observe(&s_minus);

            // Restore
            perturbed[i] = saved;

            // Central difference: J[k][i] = (obs_plus[k] - obs_minus[k]) / (2δ)
            for k in 0..n_outputs {
                let plus = obs_plus.get(k).copied().unwrap_or(0.0);
                let minus = obs_minus.get(k).copied().unwrap_or(0.0);
                matrix[k][i] = (plus - minus) * inv_two_delta;
            }
        }

        Self {
            matrix,
            n_inputs,
            n_outputs,
            delta,
            stride,
        }
    }

    /// Number of evaluated input columns (n_inputs / stride, rounded up).
    #[must_use]
    pub fn n_evaluated_columns(&self) -> usize {
        (self.n_inputs + self.stride - 1) / self.stride
    }
}

/// Participation ratio of a Jacobian row: a measure of how many inputs
/// contribute roughly equally to the output. NaN entries are skipped.
///
/// ```text
///   PR = (Σ_i |J_i|)² / Σ_i J_i²
/// ```
///
/// `PR → 1` means the output is determined by ONE input.
/// `PR → N` means the output is uniformly determined by all N inputs.
#[must_use]
pub fn participation_ratio(row: &[f64]) -> f64 {
    let mut l1 = 0.0_f64;
    let mut l2_sq = 0.0_f64;
    for &v in row {
        if v.is_finite() {
            let abs = v.abs();
            l1 += abs;
            l2_sq += abs * abs;
        }
    }
    if l2_sq < 1e-30 {
        0.0
    } else {
        (l1 * l1) / l2_sq
    }
}

/// Per-bond participation ratio for the entire Jacobian.
#[must_use]
pub fn participation_ratio_profile(jacobian: &InputJacobian) -> Vec<f64> {
    jacobian
        .matrix
        .iter()
        .map(|row| participation_ratio(row))
        .collect()
}

/// L1 norm (total sensitivity) of each output to all inputs.
/// Useful as an alternative to PR — measures total magnitude rather
/// than spread.
#[must_use]
pub fn total_sensitivity_profile(jacobian: &InputJacobian) -> Vec<f64> {
    jacobian
        .matrix
        .iter()
        .map(|row| row.iter().filter(|v| v.is_finite()).map(|v| v.abs()).sum())
        .collect()
}

/// Allocate a per-bond χ budget proportional to a "complexity" score
/// derived from the Jacobian. Two strategies:
///
/// - `JacobianAllocation::ParticipationRatio`: χ_k ∝ PR_k (more inputs ⇒
///   more chi)
/// - `JacobianAllocation::TotalSensitivity`: χ_k ∝ ||J_k||_1 (larger
///   gradients ⇒ more chi)
///
/// The result is normalised so that `Σ χ_k = chi_max · n_bonds` (uniform
/// budget) but redistributed.
#[derive(Debug, Clone, Copy)]
pub enum JacobianAllocation {
    ParticipationRatio,
    TotalSensitivity,
}

#[must_use]
pub fn chi_allocation_from_jacobian(
    jacobian: &InputJacobian,
    chi_min: usize,
    chi_max: usize,
    strategy: JacobianAllocation,
) -> Vec<usize> {
    let scores: Vec<f64> = match strategy {
        JacobianAllocation::ParticipationRatio => participation_ratio_profile(jacobian),
        JacobianAllocation::TotalSensitivity => total_sensitivity_profile(jacobian),
    };

    if scores.is_empty() {
        return Vec::new();
    }

    let max_score = scores.iter().cloned().fold(0.0_f64, f64::max);
    if max_score < 1e-30 {
        return vec![chi_min; scores.len()];
    }

    let chi_range = chi_max.saturating_sub(chi_min) as f64;

    scores
        .iter()
        .map(|&s| {
            let frac = (s / max_score).sqrt();
            let chi = chi_min + (chi_range * frac) as usize;
            chi.clamp(chi_min, chi_max)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jacobian_of_linear_function_is_identity() {
        // f(x) = x, so J = identity
        let inputs = vec![1.0, 2.0, 3.0];
        let factory = |x: &[f64]| x.to_vec();
        let observe = |s: &Vec<f64>| s.clone();
        let j = InputJacobian::compute(&inputs, factory, observe, &JacobianConfig::default());

        assert_eq!(j.n_inputs, 3);
        assert_eq!(j.n_outputs, 3);

        for k in 0..3 {
            for i in 0..3 {
                let expected = if k == i { 1.0 } else { 0.0 };
                assert!(
                    (j.matrix[k][i] - expected).abs() < 1e-6,
                    "J[{k}][{i}] = {} != {expected}",
                    j.matrix[k][i]
                );
            }
        }
    }

    #[test]
    fn jacobian_of_squared_function() {
        // f_k(x) = x_k², so J[k][i] = 2x_k δ_{ki}
        let inputs = vec![1.0, 2.0, 3.0];
        let factory = |x: &[f64]| x.iter().map(|v| v * v).collect::<Vec<f64>>();
        let observe = |s: &Vec<f64>| s.clone();
        let j = InputJacobian::compute(&inputs, factory, observe, &JacobianConfig::default());

        for k in 0..3 {
            for i in 0..3 {
                let expected = if k == i { 2.0 * inputs[k] } else { 0.0 };
                assert!(
                    (j.matrix[k][i] - expected).abs() < 1e-3,
                    "J[{k}][{i}] = {} != {expected}",
                    j.matrix[k][i]
                );
            }
        }
    }

    #[test]
    fn participation_ratio_sanity() {
        // One dominant input → PR = 1
        let row = vec![5.0, 0.0, 0.0, 0.0];
        assert!((participation_ratio(&row) - 1.0).abs() < 1e-10);

        // All equal → PR = N
        let row = vec![1.0, 1.0, 1.0, 1.0];
        assert!((participation_ratio(&row) - 4.0).abs() < 1e-10);

        // Mix
        let row = vec![3.0, 1.0];
        // l1 = 4, l2² = 10, PR = 16/10 = 1.6
        assert!((participation_ratio(&row) - 1.6).abs() < 1e-10);

        // Empty
        assert_eq!(participation_ratio(&[]), 0.0);

        // NaN ignored
        let row = vec![1.0, f64::NAN, 1.0];
        let pr = participation_ratio(&row);
        assert!((pr - 2.0).abs() < 1e-10, "got {pr}");
    }

    #[test]
    fn chi_allocation_uses_full_range() {
        // Two outputs: one local (PR≈1), one global (PR≈4)
        let j = InputJacobian {
            matrix: vec![
                vec![5.0, 0.0, 0.0, 0.0], // local
                vec![1.0, 1.0, 1.0, 1.0], // global
            ],
            n_inputs: 4,
            n_outputs: 2,
            delta: 1e-3,
            stride: 1,
        };

        let chi = chi_allocation_from_jacobian(&j, 4, 64, JacobianAllocation::ParticipationRatio);
        assert_eq!(chi.len(), 2);
        // Local bond should get less chi than global bond
        assert!(
            chi[0] < chi[1],
            "expected chi[local] < chi[global], got {chi:?}"
        );
    }
}
