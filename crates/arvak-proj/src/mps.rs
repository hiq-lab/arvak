//! Matrix Product State (MPS) simulator with adaptive per-bond truncation.
//!
//! Core data structure: a chain of rank-3 tensors A_i[α, σ, β] where
//! α is the left bond index, σ ∈ {0,1} is the physical (qubit) index,
//! and β is the right bond index. Bond dimension χ controls accuracy.

use faer::Mat;
use num_complex::Complex64;
use rayon::prelude::*;

use crate::error::Result;

type C = Complex64;

/// SVD truncation mode.
///
/// `Absolute` is the original behaviour: keep up to `max_chi` singular values,
/// drop those below `s_max * 1e-14`. The error is unbounded — it depends on the
/// SV distribution.
///
/// `DiscardedWeight` keeps singular values until the cumulative discarded weight
/// `Σ σ²` (over the discarded SVs) reaches a fraction `eps` of the total weight.
/// The truncation error is then **exactly** `√(discarded_weight)` in 2-norm,
/// which composes cleanly with the Bianchi projection (Frobenius norm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TruncationMode {
    /// Drop SVs below `s_max * 1e-14`. Default for backward compatibility.
    Absolute,
    /// Drop SVs until cumulative `Σ σ² / total ≥ 1 - eps`.
    DiscardedWeight { eps: f64 },
}

impl Default for TruncationMode {
    fn default() -> Self {
        Self::Absolute
    }
}

/// A single MPS site tensor: shape [left_dim, 2, right_dim].
/// Stored as two matrices (one per physical index σ=0, σ=1),
/// each of shape [left_dim × right_dim].
#[derive(Debug, Clone)]
pub struct SiteTensor {
    /// Matrix for σ = 0: shape [left_dim, right_dim]
    pub m0: Vec<C>,
    /// Matrix for σ = 1: shape [left_dim, right_dim]
    pub m1: Vec<C>,
    pub left_dim: usize,
    pub right_dim: usize,
    /// Singular values on the right bond (length = right_dim).
    /// `None` for product states or before any SVD has happened on this bond.
    /// Populated by `apply_two_qubit` after SVD truncation.
    /// Used by the Bianchi diagnostic and projection (see `bianchi.rs`).
    pub lambda: Option<Vec<f64>>,
}

impl SiteTensor {
    /// Create a new site tensor initialized to |0⟩ product state.
    /// For site i: A[0, σ=0, 0] = 1, A[0, σ=1, 0] = 0.
    #[must_use]
    pub fn product_zero() -> Self {
        Self {
            m0: vec![C::new(1.0, 0.0)],
            m1: vec![C::new(0.0, 0.0)],
            left_dim: 1,
            right_dim: 1,
            lambda: None,
        }
    }

    /// Access element A[α, σ, β].
    #[must_use]
    pub fn get(&self, alpha: usize, sigma: usize, beta: usize) -> C {
        let mat = if sigma == 0 { &self.m0 } else { &self.m1 };
        mat[alpha * self.right_dim + beta]
    }

    /// Mutable access.
    pub fn get_mut(&mut self, alpha: usize, sigma: usize, beta: usize) -> &mut C {
        let mat = if sigma == 0 {
            &mut self.m0
        } else {
            &mut self.m1
        };
        &mut mat[alpha * self.right_dim + beta]
    }
}

/// Matrix Product State for n qubits.
#[derive(Debug, Clone)]
pub struct Mps {
    pub sites: Vec<SiteTensor>,
    pub n_qubits: usize,
    /// SVD truncation mode used by `apply_two_qubit`. Default: `Absolute`.
    pub truncation_mode: TruncationMode,
    /// Cumulative discarded spectral weight per bond, length = n_qubits - 1.
    /// Each entry is `Σ σᵢ²` over all SVs that have been discarded by SVD
    /// truncation at this bond throughout the lifetime of the MPS.
    /// This is the **true** 2-norm truncation error accumulator.
    pub discarded_weight_per_bond: Vec<f64>,
}

impl Mps {
    /// Initialize MPS in |0...0⟩ product state.
    #[must_use]
    pub fn new(n_qubits: usize) -> Self {
        let sites = (0..n_qubits).map(|_| SiteTensor::product_zero()).collect();
        Self {
            sites,
            n_qubits,
            truncation_mode: TruncationMode::default(),
            discarded_weight_per_bond: vec![0.0; n_qubits.saturating_sub(1)],
        }
    }

    /// Reset the cumulative discarded weight tracker.
    pub fn reset_discarded_weight(&mut self) {
        self.discarded_weight_per_bond
            .iter_mut()
            .for_each(|w| *w = 0.0);
    }

    /// Cumulative discarded weight at bond `k`.
    #[must_use]
    pub fn discarded_weight(&self, bond: usize) -> f64 {
        self.discarded_weight_per_bond
            .get(bond)
            .copied()
            .unwrap_or(0.0)
    }

    /// Total cumulative discarded weight summed over all bonds.
    /// This is the **true** total truncation error in 2-norm² incurred so far.
    #[must_use]
    pub fn total_discarded_weight(&self) -> f64 {
        self.discarded_weight_per_bond.iter().sum()
    }

    /// Set the SVD truncation mode for subsequent two-qubit gate applications.
    pub fn set_truncation_mode(&mut self, mode: TruncationMode) {
        self.truncation_mode = mode;
    }

    /// Sum of cube of bond dimensions — proxy for compute footprint of one
    /// MPS pass. Useful for comparing adaptive-χ profiles.
    #[must_use]
    pub fn get_cost(&self) -> u64 {
        self.sites
            .iter()
            .map(|s| {
                let chi = s.right_dim as u64;
                chi.saturating_mul(chi).saturating_mul(chi)
            })
            .sum()
    }

    /// Bond dimension between site i and site i+1.
    #[must_use]
    pub fn bond_dim(&self, bond: usize) -> usize {
        self.sites[bond].right_dim
    }

    /// All bond dimensions.
    #[must_use]
    pub fn bond_dims(&self) -> Vec<usize> {
        (0..self.n_qubits - 1).map(|b| self.bond_dim(b)).collect()
    }

    /// Total memory (sum of χ² across bonds, rough measure).
    #[must_use]
    pub fn memory(&self) -> usize {
        self.bond_dims().iter().map(|d| d * d).sum()
    }

    /// Apply a single-qubit gate U (2×2 matrix) to qubit q.
    pub fn apply_single(&mut self, q: usize, u: [[C; 2]; 2]) {
        apply_single_to_site(&mut self.sites[q], u);
    }

    /// Apply the same single-qubit gate to ALL qubits in parallel.
    pub fn apply_single_all(&mut self, u: [[C; 2]; 2]) {
        self.sites.par_iter_mut().for_each(|site| {
            apply_single_to_site(site, u);
        });
    }

    /// Apply per-qubit single-qubit gates in parallel.
    /// `gates[i]` is applied to qubit i.
    pub fn apply_single_layer(&mut self, gates: &[[[C; 2]; 2]]) {
        self.sites
            .par_iter_mut()
            .zip(gates.par_iter())
            .for_each(|(site, u)| {
                apply_single_to_site(site, *u);
            });
    }

    /// Apply a layer of two-qubit gates to even bonds (0-1, 2-3, 4-5, ...)
    /// in parallel, then odd bonds (1-2, 3-4, 5-6, ...) in parallel.
    /// Each gate gets its own max_bond from `chi_per_bond`.
    pub fn apply_two_qubit_layer_parallel(
        &mut self,
        gate: [[C; 4]; 4],
        chi_per_bond: &[usize],
        angles: &[f64],
    ) -> Result<()> {
        let n = self.n_qubits;

        // Even bonds: 0-1, 2-3, 4-5, ... (non-overlapping)
        let even_bonds: Vec<usize> = (0..n - 1).step_by(2).collect();
        self.apply_bond_set_parallel(&even_bonds, &gate, chi_per_bond, angles)?;

        // Odd bonds: 1-2, 3-4, 5-6, ... (non-overlapping)
        let odd_bonds: Vec<usize> = (1..n - 1).step_by(2).collect();
        self.apply_bond_set_parallel(&odd_bonds, &gate, chi_per_bond, angles)?;

        Ok(())
    }

    /// Apply two-qubit gates to a set of non-overlapping bonds in parallel.
    fn apply_bond_set_parallel(
        &mut self,
        bonds: &[usize],
        _gate_template: &[[C; 4]; 4],
        chi_per_bond: &[usize],
        angles: &[f64],
    ) -> Result<()> {
        // Extract pairs of site tensors, process in parallel, write back
        // We need to split the sites vec into non-overlapping mutable slices.

        // For non-overlapping bonds (e.g., 0,2,4,...), each bond touches
        // sites[q] and sites[q+1]. Since bonds are ≥2 apart, the slices
        // don't overlap, but Rust's borrow checker can't prove this with
        // simple indexing. Use split_at_mut chains.

        // Sequential for now — the SVD is the bottleneck and faer
        // already uses SIMD internally. True parallel SVD needs unsafe
        // or a different data layout. TODO: unsafe parallel with raw ptrs.
        for &bond in bonds {
            let theta = angles.get(bond).copied().unwrap_or(0.0);
            let max_chi = chi_per_bond.get(bond).copied().unwrap_or(8);

            // Always go through the SVD path. The earlier `apply_zz_fast`
            // shortcut absorbed the diagonal phase into the left site only,
            // which is silently wrong for ZZ as soon as both bond dimensions
            // exceed 1 (it conflates `σ_q=0,σ_{q+1}=1` with `σ_q=0,σ_{q+1}=0`).
            // The correct full SVD path is the only safe option.
            let gate = zz(theta);
            self.apply_two_qubit(bond, gate, max_chi)?;
        }
        Ok(())
    }

    /// Apply a two-qubit gate U (4×4 matrix) to adjacent qubits (q, q+1).
    /// Then SVD-truncate the bond to `max_bond`.
    ///
    /// The gate U acts on basis |σ_q σ_{q+1}⟩ ordered as |00⟩, |01⟩, |10⟩, |11⟩.
    pub fn apply_two_qubit(&mut self, q: usize, u: [[C; 4]; 4], max_bond: usize) -> Result<()> {
        let ld = self.sites[q].left_dim;
        let rd = self.sites[q + 1].right_dim;
        let chi_l = self.sites[q].right_dim; // = sites[q+1].left_dim

        // Step 1: Contract sites q and q+1 into a rank-4 tensor
        // Θ[α, σ_q, σ_{q+1}, β] = Σ_γ A_q[α, σ_q, γ] · A_{q+1}[γ, σ_{q+1}, β]
        // Then apply gate: Θ'[α, σ'_q, σ'_{q+1}, β] = Σ_{σ_q, σ_{q+1}} U[σ'_q σ'_{q+1}, σ_q σ_{q+1}] · Θ
        //
        // Reshape Θ' as matrix M[α·σ'_q, σ'_{q+1}·β] of shape [ld*2, 2*rd]
        // SVD → truncate → new site tensors.

        let rows = ld * 2;
        let cols = 2 * rd;
        let mut theta = vec![C::new(0.0, 0.0); rows * cols];

        // Contract + apply gate
        for a in 0..ld {
            for sp_q in 0..2_usize {
                for sp_r in 0..2_usize {
                    for b in 0..rd {
                        let row = a * 2 + sp_q;
                        let col = sp_r * rd + b;
                        let mut val = C::new(0.0, 0.0);

                        for s_q in 0..2_usize {
                            for s_r in 0..2_usize {
                                let gate_row = sp_q * 2 + sp_r;
                                let gate_col = s_q * 2 + s_r;
                                let u_elem = u[gate_row][gate_col];

                                // Contract over γ
                                for g in 0..chi_l {
                                    let a_q = if s_q == 0 {
                                        self.sites[q].m0[a * chi_l + g]
                                    } else {
                                        self.sites[q].m1[a * chi_l + g]
                                    };
                                    let a_r = if s_r == 0 {
                                        self.sites[q + 1].m0[g * rd + b]
                                    } else {
                                        self.sites[q + 1].m1[g * rd + b]
                                    };
                                    val += u_elem * a_q * a_r;
                                }
                            }
                        }

                        theta[row * cols + col] = val;
                    }
                }
            }
        }

        // Step 2: SVD of theta matrix [rows × cols]
        let new_chi = max_bond.min(rows).min(cols);
        let (new_m0_q, new_m1_q, new_m0_r, new_m1_r, actual_chi, singular_values, discarded_weight) =
            svd_truncate(&theta, rows, cols, ld, rd, new_chi, self.truncation_mode)?;

        // Accumulate the per-bond discarded-weight tracker. This is the
        // honest 2-norm² truncation error contributed by THIS gate at THIS
        // bond. Summed over time, it gives the total information loss at
        // each bond — the basis for data-driven χ allocation.
        if let Some(slot) = self.discarded_weight_per_bond.get_mut(q) {
            *slot += discarded_weight;
        }

        // Step 3: Update site tensors. Store the singular values on the LEFT
        // tensor's right bond (the bond between q and q+1) for Bianchi diagnostic.
        let prev_lambda_right = self.sites[q + 1].lambda.take();
        self.sites[q] = SiteTensor {
            m0: new_m0_q,
            m1: new_m1_q,
            left_dim: ld,
            right_dim: actual_chi,
            lambda: Some(singular_values),
        };
        self.sites[q + 1] = SiteTensor {
            m0: new_m0_r,
            m1: new_m1_r,
            left_dim: actual_chi,
            right_dim: rd,
            lambda: prev_lambda_right,
        };

        Ok(())
    }

    /// Compute `<ψ|ψ>` for the current MPS by sweeping the left-environment
    /// transfer matrix from left to right. Cost O(N · d · χ³).
    ///
    /// Used internally to normalise expectation values when the MPS is not
    /// guaranteed to satisfy `<ψ|ψ> = 1` (which is the case after SVD
    /// truncation in balanced canonical form).
    #[must_use]
    pub fn norm_squared(&self) -> f64 {
        // Left environment: 1×1 = (1)
        let mut env: Vec<C> = vec![C::new(1.0, 0.0)];
        let mut env_ld = 1_usize;

        for site in &self.sites {
            let ld = site.left_dim;
            let rd = site.right_dim;
            debug_assert_eq!(env_ld, ld, "environment dim mismatch in norm_squared");

            // env'[β',β] = Σ_{α',α,σ} env[α',α] · m_σ[α',β']^* · m_σ[α,β]
            let mut new_env = vec![C::new(0.0, 0.0); rd * rd];
            for ap in 0..ld {
                for a in 0..ld {
                    let e = env[ap * ld + a];
                    if e.re == 0.0 && e.im == 0.0 {
                        continue;
                    }
                    for bp in 0..rd {
                        let m0_apbp = site.m0[ap * rd + bp].conj();
                        let m1_apbp = site.m1[ap * rd + bp].conj();
                        for b in 0..rd {
                            let m0_ab = site.m0[a * rd + b];
                            let m1_ab = site.m1[a * rd + b];
                            new_env[bp * rd + b] += e * (m0_apbp * m0_ab + m1_apbp * m1_ab);
                        }
                    }
                }
            }
            env = new_env;
            env_ld = rd;
        }

        debug_assert_eq!(env.len(), 1, "norm_squared environment did not collapse to 1×1");
        env[0].re
    }

    /// Compute `<ψ|Z_q|ψ> / <ψ|ψ>` for the Pauli-Z operator on qubit `target`.
    ///
    /// Sweeps the left environment from site 0 to site N-1, inserting the
    /// Z eigenvalue (`+1` for |0⟩, `-1` for |1⟩) at the target site. Cost
    /// O(N · d · χ³). Works for any MPS regardless of canonical form, because
    /// it computes both `<ψ|Z|ψ>` and `<ψ|ψ>` in a single sweep and divides.
    #[must_use]
    pub fn expectation_z(&self, target: usize) -> f64 {
        assert!(target < self.n_qubits, "target qubit out of range");

        // Two parallel left environments: one for the numerator (with Z
        // inserted at `target`) and one for the denominator (identity).
        let mut env_z: Vec<C> = vec![C::new(1.0, 0.0)];
        let mut env_n: Vec<C> = vec![C::new(1.0, 0.0)];
        let mut env_ld = 1_usize;

        for (site_idx, site) in self.sites.iter().enumerate() {
            let ld = site.left_dim;
            let rd = site.right_dim;
            debug_assert_eq!(env_ld, ld, "environment dim mismatch in expectation_z");

            // Z eigenvalue at this site: +1 on σ=0, -1 on σ=1.
            // For non-target sites, both spins contribute with sign +1 (identity).
            let z_sign_1: f64 = if site_idx == target { -1.0 } else { 1.0 };

            let mut new_env_z = vec![C::new(0.0, 0.0); rd * rd];
            let mut new_env_n = vec![C::new(0.0, 0.0); rd * rd];

            for ap in 0..ld {
                for a in 0..ld {
                    let ez = env_z[ap * ld + a];
                    let en = env_n[ap * ld + a];
                    if (ez.re == 0.0 && ez.im == 0.0) && (en.re == 0.0 && en.im == 0.0) {
                        continue;
                    }
                    for bp in 0..rd {
                        let m0_apbp = site.m0[ap * rd + bp].conj();
                        let m1_apbp = site.m1[ap * rd + bp].conj();
                        for b in 0..rd {
                            let m0_ab = site.m0[a * rd + b];
                            let m1_ab = site.m1[a * rd + b];

                            let term0 = m0_apbp * m0_ab;
                            let term1 = m1_apbp * m1_ab;

                            // Numerator: +1·term0 + z_sign_1·term1
                            new_env_z[bp * rd + b] += ez * (term0 + term1 * z_sign_1);
                            // Denominator: identity (both spins +1)
                            new_env_n[bp * rd + b] += en * (term0 + term1);
                        }
                    }
                }
            }
            env_z = new_env_z;
            env_n = new_env_n;
            env_ld = rd;
        }

        debug_assert_eq!(env_z.len(), 1);
        debug_assert_eq!(env_n.len(), 1);
        let denom = env_n[0].re;
        if denom.abs() < 1e-30 {
            return 0.0;
        }
        env_z[0].re / denom
    }

    /// Contract the full MPS into a dense state vector (for validation).
    /// Only feasible for small n_qubits (≤ ~25).
    #[must_use]
    pub fn to_statevector(&self) -> Vec<C> {
        let dim = 1 << self.n_qubits;
        let mut psi = vec![C::new(0.0, 0.0); dim];

        for (idx, amplitude) in psi.iter_mut().enumerate() {
            let mut vec = vec![C::new(1.0, 0.0)];
            let mut current_dim = 1;

            for q in 0..self.n_qubits {
                let bit = (idx >> (self.n_qubits - 1 - q)) & 1;
                let site = &self.sites[q];
                let mat = if bit == 0 { &site.m0 } else { &site.m1 };
                let rd = site.right_dim;

                let mut new_vec = vec![C::new(0.0, 0.0); rd];
                for b in 0..rd {
                    for a in 0..current_dim {
                        new_vec[b] += vec[a] * mat[a * rd + b];
                    }
                }
                vec = new_vec;
                current_dim = rd;
            }

            *amplitude = vec[0];
        }

        psi
    }
}

/// SVD truncation of the merged tensor.
/// Input: flat matrix `theta` of shape [rows × cols] where rows = ld*2, cols = 2*rd.
/// Returns: (m0_left, m1_left, m0_right, m1_right, actual_chi, singular_values, discarded_weight).
/// `singular_values` has length `actual_chi`.
/// `discarded_weight` is `Σ σᵢ²` over all SVs that were dropped (for the
/// cumulative discarded-weight accumulator on the MPS bond).
#[allow(clippy::type_complexity)]
fn svd_truncate(
    theta: &[C],
    rows: usize,
    cols: usize,
    ld: usize,
    rd: usize,
    max_chi: usize,
    mode: TruncationMode,
) -> Result<(Vec<C>, Vec<C>, Vec<C>, Vec<C>, usize, Vec<f64>, f64)> {
    // Build faer matrix
    let mat = Mat::from_fn(rows, cols, |i, j| {
        let c = theta[i * cols + j];
        faer::c64::new(c.re, c.im)
    });

    let svd = mat
        .thin_svd()
        .map_err(|_| crate::error::ProjError::SvdFailed(0))?;
    let u = svd.U();
    let s = svd.S();
    let v = svd.V();

    // Determine actual chi based on truncation mode.
    let n_singular = s.column_vector().nrows().min(max_chi);
    let mut actual_chi = n_singular;

    match mode {
        TruncationMode::Absolute => {
            // Drop negligible singular values: σ_i / σ_max < 1e-14
            if n_singular > 1 {
                let s_max = s.column_vector()[0].re;
                if s_max > 1e-15 {
                    for i in (1..n_singular).rev() {
                        if s.column_vector()[i].re / s_max < 1e-14 {
                            actual_chi = i;
                        } else {
                            break;
                        }
                    }
                }
            }
        }
        TruncationMode::DiscardedWeight { eps } => {
            // Keep SVs until cumulative kept-weight ≥ (1 - eps) of total.
            // This bounds the truncation error to √eps in 2-norm.
            let total: f64 = (0..s.column_vector().nrows())
                .map(|i| {
                    let v = s.column_vector()[i].re;
                    v * v
                })
                .sum();
            if total > 1e-30 {
                let target = (1.0 - eps) * total;
                let mut cumulative = 0.0_f64;
                let mut keep = 0_usize;
                for i in 0..n_singular {
                    let v = s.column_vector()[i].re;
                    cumulative += v * v;
                    keep = i + 1;
                    if cumulative >= target {
                        break;
                    }
                }
                actual_chi = keep;
            }
        }
    }
    actual_chi = actual_chi.max(1);

    // Compute discarded weight: Σ σᵢ² for i ≥ actual_chi
    let mut discarded_weight = 0.0_f64;
    for i in actual_chi..s.column_vector().nrows() {
        let v = s.column_vector()[i].re;
        discarded_weight += v * v;
    }

    // Build left site tensors: A_q[α, σ, γ] = U[α*2+σ, γ] * sqrt(S[γ])
    // Build right site tensors: A_{q+1}[γ, σ, β] = sqrt(S[γ]) * V†[γ, σ*rd+β]
    let mut m0_q = vec![C::new(0.0, 0.0); ld * actual_chi];
    let mut m1_q = vec![C::new(0.0, 0.0); ld * actual_chi];
    let mut m0_r = vec![C::new(0.0, 0.0); actual_chi * rd];
    let mut m1_r = vec![C::new(0.0, 0.0); actual_chi * rd];

    // Singular values for the truncated bond (kept for Bianchi diagnostic).
    let mut singular_values = Vec::with_capacity(actual_chi);

    for g in 0..actual_chi {
        let s_g = s.column_vector()[g].re;
        singular_values.push(s_g);
        let sqrt_s = s_g.sqrt();

        // Left: U[a*2 + sigma, g] * sqrt(s)
        for a in 0..ld {
            let u00 = u[(a * 2, g)];
            m0_q[a * actual_chi + g] = C::new(u00.re * sqrt_s, u00.im * sqrt_s);
            let u10 = u[(a * 2 + 1, g)];
            m1_q[a * actual_chi + g] = C::new(u10.re * sqrt_s, u10.im * sqrt_s);
        }

        // Right: sqrt(s) * V†[g, sigma*rd + b] = sqrt(s) * conj(V[sigma*rd + b, g])
        for b in 0..rd {
            let v00 = v[(b, g)]; // V†[g, 0*rd+b] = conj(V[0*rd+b, g])
            m0_r[g * rd + b] = C::new(v00.re * sqrt_s, -v00.im * sqrt_s);
            let v10 = v[(rd + b, g)];
            m1_r[g * rd + b] = C::new(v10.re * sqrt_s, -v10.im * sqrt_s);
        }
    }

    Ok((
        m0_q,
        m1_q,
        m0_r,
        m1_r,
        actual_chi,
        singular_values,
        discarded_weight,
    ))
}

/// Apply a single-qubit gate to a site tensor (free function for parallel use).
fn apply_single_to_site(site: &mut SiteTensor, u: [[C; 2]; 2]) {
    let ld = site.left_dim;
    let rd = site.right_dim;

    let mut new_m0 = vec![C::new(0.0, 0.0); ld * rd];
    let mut new_m1 = vec![C::new(0.0, 0.0); ld * rd];

    for a in 0..ld {
        for b in 0..rd {
            let v0 = site.m0[a * rd + b];
            let v1 = site.m1[a * rd + b];
            new_m0[a * rd + b] = u[0][0] * v0 + u[0][1] * v1;
            new_m1[a * rd + b] = u[1][0] * v0 + u[1][1] * v1;
        }
    }

    site.m0 = new_m0;
    site.m1 = new_m1;
}

// ── Standard quantum gates as 2×2 / 4×4 matrices ────────────────────

/// RX(θ) gate.
#[must_use]
pub fn rx(theta: f64) -> [[C; 2]; 2] {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    [
        [C::new(c, 0.0), C::new(0.0, -s)],
        [C::new(0.0, -s), C::new(c, 0.0)],
    ]
}

/// RY(θ) gate.
#[must_use]
pub fn ry(theta: f64) -> [[C; 2]; 2] {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    [
        [C::new(c, 0.0), C::new(-s, 0.0)],
        [C::new(s, 0.0), C::new(c, 0.0)],
    ]
}

/// RZ(θ) gate.
#[must_use]
pub fn rz(theta: f64) -> [[C; 2]; 2] {
    [
        [C::from_polar(1.0, -theta / 2.0), C::new(0.0, 0.0)],
        [C::new(0.0, 0.0), C::from_polar(1.0, theta / 2.0)],
    ]
}

/// Hadamard gate.
#[must_use]
pub fn h() -> [[C; 2]; 2] {
    let s = 1.0 / 2.0_f64.sqrt();
    [
        [C::new(s, 0.0), C::new(s, 0.0)],
        [C::new(s, 0.0), C::new(-s, 0.0)],
    ]
}

/// ZZ(θ) = exp(-i θ Z⊗Z) as 4×4 diagonal gate.
#[must_use]
pub fn zz(theta: f64) -> [[C; 4]; 4] {
    let mut u = [[C::new(0.0, 0.0); 4]; 4];
    u[0][0] = C::from_polar(1.0, -theta); // |00⟩: eigenvalue +1
    u[1][1] = C::from_polar(1.0, theta); // |01⟩: eigenvalue -1
    u[2][2] = C::from_polar(1.0, theta); // |10⟩: eigenvalue -1
    u[3][3] = C::from_polar(1.0, -theta); // |11⟩: eigenvalue +1
    u
}

/// ITE-ZZ: exp(-τJ Z⊗Z) — imaginary time evolution gate (non-unitary).
///
/// Diagonal real gate for ground-state finding via ITE-TEBD.
#[must_use]
pub fn ite_zz(tau_j: f64) -> [[C; 4]; 4] {
    let mut u = [[C::new(0.0, 0.0); 4]; 4];
    u[0][0] = C::new((-tau_j).exp(), 0.0); // |00⟩: e^{-τJ}
    u[1][1] = C::new(tau_j.exp(), 0.0); // |01⟩: e^{+τJ}
    u[2][2] = C::new(tau_j.exp(), 0.0); // |10⟩: e^{+τJ}
    u[3][3] = C::new((-tau_j).exp(), 0.0); // |11⟩: e^{-τJ}
    u
}

/// ITE-X: exp(-τh X) — imaginary time evolution of transverse field (non-unitary).
#[must_use]
pub fn ite_x(tau_h: f64) -> [[C; 2]; 2] {
    let c = tau_h.cosh();
    let s = tau_h.sinh();
    [
        [C::new(c, 0.0), C::new(s, 0.0)],
        [C::new(s, 0.0), C::new(c, 0.0)],
    ]
}

/// CNOT gate.
#[must_use]
pub fn cx() -> [[C; 4]; 4] {
    let o = C::new(0.0, 0.0);
    let i = C::new(1.0, 0.0);
    [[i, o, o, o], [o, i, o, o], [o, o, o, i], [o, o, i, o]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_state() {
        let mps = Mps::new(4);
        let psi = mps.to_statevector();
        assert_eq!(psi.len(), 16);
        assert!((psi[0].norm() - 1.0).abs() < 1e-12);
        for amp in psi.iter().take(16).skip(1) {
            assert!(amp.norm() < 1e-12);
        }
    }

    #[test]
    fn single_qubit_x() {
        let mut mps = Mps::new(2);
        // Apply X = RX(π) to qubit 0
        mps.apply_single(0, rx(std::f64::consts::PI));
        let psi = mps.to_statevector();
        // Should be |10⟩
        assert!(psi[0].norm() < 1e-10);
        assert!((psi[2].norm() - 1.0).abs() < 1e-10); // |10⟩ = index 2
    }

    #[test]
    fn bell_state() {
        let mut mps = Mps::new(2);
        // H on qubit 0
        mps.apply_single(0, h());
        // CNOT
        mps.apply_two_qubit(0, cx(), 4).unwrap();
        let psi = mps.to_statevector();
        // Bell state: (|00⟩ + |11⟩)/√2
        let expected = 1.0 / 2.0_f64.sqrt();
        assert!((psi[0].norm() - expected).abs() < 1e-10);
        assert!(psi[1].norm() < 1e-10);
        assert!(psi[2].norm() < 1e-10);
        assert!((psi[3].norm() - expected).abs() < 1e-10);
    }

    #[test]
    fn ghz_state() {
        let n = 5;
        let mut mps = Mps::new(n);
        mps.apply_single(0, h());
        for q in 0..n - 1 {
            mps.apply_two_qubit(q, cx(), 8).unwrap();
        }
        let psi = mps.to_statevector();
        let expected = 1.0 / 2.0_f64.sqrt();
        assert!((psi[0].norm() - expected).abs() < 1e-10);
        assert!((psi[(1 << n) - 1].norm() - expected).abs() < 1e-10);
        // All other amplitudes should be zero
        for (i, amp) in psi.iter().enumerate().take((1 << n) - 1).skip(1) {
            assert!(amp.norm() < 1e-10, "psi[{i}] = {}", amp.norm());
        }
    }

    #[test]
    fn bond_dims_grow_with_entanglement() {
        let mut mps = Mps::new(6);
        mps.apply_single(0, h());
        for q in 0..5 {
            mps.apply_two_qubit(q, cx(), 64).unwrap();
        }
        let dims = mps.bond_dims();
        // GHZ state has bond dim 2 everywhere
        assert!(dims.iter().all(|&d| d == 2), "dims = {dims:?}");
    }
}
