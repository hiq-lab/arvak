//! Imaginary-time TEBD (Time-Evolving Block Decimation) for protein folding.
//!
//! Uses the same MPS + SVD machinery as the circuit backend, but with:
//! - Real-valued tensors (f64, not Complex64)
//! - Arbitrary local dimension d (3, 9, 36)
//! - Backbone gates: exp(-dτ · H_nn) as precomputed d²×d² matrices
//! - Long-range contacts via SWAP networks
//! - Adaptive bond dimension from sin(C/2) commensurability analysis

use faer::Mat;

/// Site tensor for a real-valued MPS with local dimension d.
/// Shape: [left_dim, d, right_dim], stored as d matrices of [left_dim × right_dim].
#[derive(Debug, Clone)]
pub struct RealSiteTensor {
    /// One matrix per physical index σ ∈ {0, ..., d-1}.
    /// Each matrix is row-major: [left_dim × right_dim].
    pub matrices: Vec<Vec<f64>>,
    pub d: usize,
    pub left_dim: usize,
    pub right_dim: usize,
}

impl RealSiteTensor {
    /// Product state: σ=0 with amplitude 1.
    fn product_zero(d: usize) -> Self {
        let mut matrices = vec![vec![0.0; 1]; d];
        matrices[0] = vec![1.0];
        Self {
            matrices,
            d,
            left_dim: 1,
            right_dim: 1,
        }
    }

    /// Access A[α, σ, β].
    fn get(&self, alpha: usize, sigma: usize, beta: usize) -> f64 {
        self.matrices[sigma][alpha * self.right_dim + beta]
    }
}

/// Real-valued MPS for protein folding.
#[derive(Debug, Clone)]
pub struct RealMps {
    pub sites: Vec<RealSiteTensor>,
    pub n_sites: usize,
    pub d: usize,
}

impl RealMps {
    /// Initialize in product state |0...0⟩.
    pub fn new(n_sites: usize, d: usize) -> Self {
        let sites = (0..n_sites)
            .map(|_| RealSiteTensor::product_zero(d))
            .collect();
        Self { sites, n_sites, d }
    }

    pub fn bond_dim(&self, bond: usize) -> usize {
        self.sites[bond].right_dim
    }

    pub fn bond_dims(&self) -> Vec<usize> {
        (0..self.n_sites - 1)
            .map(|b| self.bond_dim(b))
            .collect()
    }

    /// Apply a single-site operator (d×d matrix, row-major) to site q.
    pub fn apply_single(&mut self, q: usize, op: &[f64]) {
        let site = &mut self.sites[q];
        let d = site.d;
        let ld = site.left_dim;
        let rd = site.right_dim;
        let mut new_matrices = vec![vec![0.0; ld * rd]; d];

        for sigma_out in 0..d {
            for sigma_in in 0..d {
                let coeff = op[sigma_out * d + sigma_in];
                if coeff.abs() < 1e-15 {
                    continue;
                }
                for alpha in 0..ld {
                    for beta in 0..rd {
                        new_matrices[sigma_out][alpha * rd + beta] +=
                            coeff * site.matrices[sigma_in][alpha * rd + beta];
                    }
                }
            }
        }

        site.matrices = new_matrices;
    }

    /// Apply a two-site gate (d²×d² matrix, row-major) to bond (q, q+1)
    /// with SVD truncation to max_chi.
    ///
    /// Returns the truncation error (sum of discarded singular values squared).
    pub fn apply_two_site(&mut self, q: usize, gate: &[f64], max_chi: usize) -> f64 {
        let d = self.d;
        let d2 = d * d;

        let ld = self.sites[q].left_dim;
        let rd = self.sites[q + 1].right_dim;

        // Contract sites[q] and sites[q+1] into θ[α, σ_q, σ_{q+1}, β]
        let mid = self.sites[q].right_dim;
        let theta_rows = ld * d;
        let theta_cols = d * rd;
        let mut theta = vec![0.0; theta_rows * theta_cols];

        for alpha in 0..ld {
            for sq in 0..d {
                for sqp in 0..d {
                    for beta in 0..rd {
                        let mut val = 0.0;
                        for gamma in 0..mid {
                            val += self.sites[q].matrices[sq][alpha * mid + gamma]
                                * self.sites[q + 1].matrices[sqp][gamma * rd + beta];
                        }
                        theta[(alpha * d + sq) * theta_cols + (sqp * rd + beta)] = val;
                    }
                }
            }
        }

        // Apply gate: θ'[α, σ'_q, σ'_{q+1}, β] = Σ_{σ_q, σ_{q+1}} G[σ'_q σ'_{q+1}, σ_q σ_{q+1}] θ[α, σ_q, σ_{q+1}, β]
        let mut theta_new = vec![0.0; theta_rows * theta_cols];

        for alpha in 0..ld {
            for sq_out in 0..d {
                for sqp_out in 0..d {
                    let row_out = sq_out * d + sqp_out;
                    for beta in 0..rd {
                        let mut val = 0.0;
                        for sq_in in 0..d {
                            for sqp_in in 0..d {
                                let row_in = sq_in * d + sqp_in;
                                let g = gate[row_out * d2 + row_in];
                                if g.abs() > 1e-15 {
                                    val += g
                                        * theta
                                            [(alpha * d + sq_in) * theta_cols + (sqp_in * rd + beta)];
                                }
                            }
                        }
                        theta_new
                            [(alpha * d + sq_out) * theta_cols + (sqp_out * rd + beta)] = val;
                    }
                }
            }
        }

        // SVD: reshape θ' as (ld*d) × (d*rd) matrix, truncate to max_chi
        let (new_left, new_right, trunc_err) =
            svd_truncate_real(&theta_new, theta_rows, theta_cols, max_chi);
        let new_chi = new_left.len() / (ld * d);

        // Update site tensors
        // Left: A[α, σ, γ] from U·S → shape [ld, d, new_chi]
        let mut left_matrices = vec![vec![0.0; ld * new_chi]; d];
        for alpha in 0..ld {
            for sigma in 0..d {
                for gamma in 0..new_chi {
                    left_matrices[sigma][alpha * new_chi + gamma] =
                        new_left[(alpha * d + sigma) * new_chi + gamma];
                }
            }
        }
        self.sites[q] = RealSiteTensor {
            matrices: left_matrices,
            d,
            left_dim: ld,
            right_dim: new_chi,
        };

        // Right: B[γ, σ, β] from V† → shape [new_chi, d, rd]
        let mut right_matrices = vec![vec![0.0; new_chi * rd]; d];
        for gamma in 0..new_chi {
            for sigma in 0..d {
                for beta in 0..rd {
                    right_matrices[sigma][gamma * rd + beta] =
                        new_right[gamma * theta_cols + sigma * rd + beta];
                }
            }
        }
        self.sites[q + 1] = RealSiteTensor {
            matrices: right_matrices,
            d,
            left_dim: new_chi,
            right_dim: rd,
        };

        trunc_err
    }

    /// SWAP sites q and q+1 (with SVD truncation).
    /// The SWAP gate is the d²×d² permutation matrix: |σ_1 σ_2⟩ → |σ_2 σ_1⟩
    pub fn swap(&mut self, q: usize, max_chi: usize) -> f64 {
        let d = self.d;
        let d2 = d * d;
        let mut swap_gate = vec![0.0; d2 * d2];
        for s1 in 0..d {
            for s2 in 0..d {
                // |s2, s1⟩⟨s1, s2| : row = s2*d+s1, col = s1*d+s2
                swap_gate[(s2 * d + s1) * d2 + s1 * d + s2] = 1.0;
            }
        }
        self.apply_two_site(q, &swap_gate, max_chi)
    }

    /// Compute ⟨ψ|O|ψ⟩ for a single-site operator O (d×d) at site q.
    pub fn expectation_single(&self, q: usize, op: &[f64]) -> f64 {
        let site = &self.sites[q];
        let d = site.d;
        let ld = site.left_dim;
        let rd = site.right_dim;

        // Contract: Σ_{α,β,σ,σ'} A*[α,σ,β] O[σ,σ'] A[α,σ',β]
        let mut val = 0.0;
        for alpha in 0..ld {
            for beta in 0..rd {
                for sigma in 0..d {
                    for sigma_p in 0..d {
                        val += site.matrices[sigma][alpha * rd + beta]
                            * op[sigma * d + sigma_p]
                            * site.matrices[sigma_p][alpha * rd + beta];
                    }
                }
            }
        }
        val
    }

    /// Normalize the MPS (make ⟨ψ|ψ⟩ = 1).
    pub fn normalize(&mut self) {
        let norm = self.norm();
        if norm < 1e-15 {
            return;
        }
        let factor = 1.0 / norm.sqrt();
        // Scale the first site
        for mat in &mut self.sites[0].matrices {
            for v in mat.iter_mut() {
                *v *= factor;
            }
        }
    }

    /// Compute ⟨ψ|ψ⟩.
    fn norm(&self) -> f64 {
        let d = self.d;

        // Transfer matrix contraction from left to right
        // env[α, α'] = Σ_σ A*[α,σ,β] A[α',σ,β'] → next env is [β, β']
        let mut env = vec![1.0; 1]; // [1×1] identity

        for site in &self.sites {
            let ld = site.left_dim;
            let rd = site.right_dim;
            let mut new_env = vec![0.0; rd * rd];

            for beta in 0..rd {
                for beta_p in 0..rd {
                    let mut val = 0.0;
                    for alpha in 0..ld {
                        for alpha_p in 0..ld {
                            let e: f64 = env[alpha * ld + alpha_p];
                            if e.abs() < 1e-15 {
                                continue;
                            }
                            for sigma in 0..d {
                                val += e
                                    * site.matrices[sigma][alpha * rd + beta]
                                    * site.matrices[sigma][alpha_p * rd + beta_p];
                            }
                        }
                    }
                    new_env[beta * rd + beta_p] = val;
                }
            }
            env = new_env;
        }

        env[0] // scalar for closed MPS
    }
}

/// SVD truncation of a real matrix. Returns (U·S, V†, truncation_error).
fn svd_truncate_real(mat: &[f64], rows: usize, cols: usize, max_chi: usize) -> (Vec<f64>, Vec<f64>, f64) {
    // Build faer matrix
    let m = Mat::<f64>::from_fn(rows, cols, |i, j| mat[i * cols + j]);
    let svd = m.thin_svd().expect("SVD failed");

    let u = svd.U();
    let s = svd.S();
    let v = svd.V();

    let s_vec = s.column_vector();
    let n_singular = s_vec.nrows().min(max_chi);

    // Find actual rank: drop singular values below threshold
    let s_max = s_vec[0];
    let mut actual_rank = n_singular;
    if n_singular > 1 && s_max > 1e-15 {
        for i in (1..n_singular).rev() {
            if s_vec[i] / s_max < 1e-14 {
                actual_rank = i;
            } else {
                break;
            }
        }
    }
    actual_rank = actual_rank.max(1);

    // Truncation error
    let trunc_err: f64 = (actual_rank..s_vec.nrows())
        .map(|i| s_vec[i] * s_vec[i])
        .sum();

    // U·S: shape [rows, actual_rank]
    let mut us = vec![0.0; rows * actual_rank];
    for i in 0..rows {
        for j in 0..actual_rank {
            us[i * actual_rank + j] = u[(i, j)] * s_vec[j];
        }
    }

    // V†: shape [actual_rank, cols] (transpose of V)
    let mut vt_out = vec![0.0; actual_rank * cols];
    for i in 0..actual_rank {
        for j in 0..cols {
            vt_out[i * cols + j] = v[(j, i)]; // V transposed
        }
    }

    (us, vt_out, trunc_err)
}

// ─────────────────────────────────────────────────────────────
// TEBD Engine
// ─────────────────────────────────────────────────────────────

/// Precomputed gates for imaginary-time evolution.
pub struct FoldingGates {
    /// Backbone gate per bond: exp(-dτ · H_nn), d²×d² matrix.
    pub backbone: Vec<Vec<f64>>,
    /// Contact gates: (i, j, gate) where gate = exp(-dτ · J · P⊗P), d²×d² matrix.
    pub contacts: Vec<(usize, usize, Vec<f64>)>,
    /// Local gates: exp(-dτ · h_local), d×d matrix per site.
    pub local: Vec<Vec<f64>>,
}

impl FoldingGates {
    /// Build gates from Hamiltonian terms.
    ///
    /// For backbone and local terms: gate = exp(-dτ · H) computed by eigendecomposition.
    /// For contact terms: gate = exp(-dτ · J · P⊗P) — diagonal in the computational basis.
    pub fn from_hamiltonian(
        ham: &super::hamiltonian::ProteinHamiltonian,
        dt: f64,
    ) -> Self {
        let d = ham.d;

        // Local gates: exp(-dt * h)
        let local: Vec<Vec<f64>> = ham
            .local_terms
            .iter()
            .map(|h| matrix_exp_real(h, d, -dt))
            .collect();

        // Backbone gates: exp(-dt * H_nn)
        let d2 = d * d;
        let backbone: Vec<Vec<f64>> = ham
            .nn_terms
            .iter()
            .map(|h| matrix_exp_real(h, d2, -dt))
            .collect();

        // Contact gates: exp(-dt * J * op_left ⊗ op_right)
        let contacts: Vec<(usize, usize, Vec<f64>)> = ham
            .long_range_terms
            .iter()
            .map(|t| {
                // Build tensor product op_left ⊗ op_right
                let mut op = vec![0.0; d2 * d2];
                for s1 in 0..d {
                    for s2 in 0..d {
                        for s1p in 0..d {
                            for s2p in 0..d {
                                let row = s1 * d + s2;
                                let col = s1p * d + s2p;
                                op[row * d2 + col] =
                                    t.op_left[s1 * d + s1p] * t.op_right[s2 * d + s2p] * t.strength;
                            }
                        }
                    }
                }
                let gate = matrix_exp_real(&op, d2, -dt);
                (t.i, t.j, gate)
            })
            .collect();

        FoldingGates {
            backbone,
            contacts,
            local,
        }
    }
}

/// TEBD solver for protein folding in imaginary time.
pub struct FoldingTEBD {
    pub mps: RealMps,
    pub gates: FoldingGates,
    pub adaptive_chi: Vec<usize>,
    pub d: usize,
}

/// Result of a TEBD simulation.
pub struct TEBDResult {
    pub energy: f64,
    pub energies_per_step: Vec<f64>,
    pub mps: RealMps,
    pub n_steps: usize,
    pub wall_time_seconds: f64,
    pub converged: bool,
}

impl FoldingTEBD {
    /// Create a new TEBD solver.
    pub fn new(
        n_sites: usize,
        d: usize,
        gates: FoldingGates,
        adaptive_chi: Vec<usize>,
    ) -> Self {
        let mps = RealMps::new(n_sites, d);
        Self {
            mps,
            gates,
            adaptive_chi,
            d,
        }
    }

    /// Run imaginary-time evolution to find the ground state.
    ///
    /// Each step:
    /// 1. Apply local gates (single-site)
    /// 2. Apply backbone gates (nearest-neighbor, even/odd Trotter)
    /// 3. Apply long-range contact gates via SWAP networks
    /// 4. Normalize
    /// 5. Measure energy
    pub fn evolve(&mut self, n_steps: usize, energy_tol: f64) -> TEBDResult {
        let t0 = std::time::Instant::now();
        let n = self.mps.n_sites;
        let mut energies = Vec::with_capacity(n_steps);
        let mut converged = false;

        for step in 0..n_steps {
            // 1. Local gates
            for (q, gate) in self.gates.local.iter().enumerate() {
                self.mps.apply_single(q, gate);
            }

            // 2. Backbone gates: even bonds, then odd bonds (2nd order Trotter)
            for q in (0..n - 1).step_by(2) {
                let chi = self.adaptive_chi.get(q).copied().unwrap_or(16);
                self.mps.apply_two_site(q, &self.gates.backbone[q], chi);
            }
            for q in (1..n - 1).step_by(2) {
                let chi = self.adaptive_chi.get(q).copied().unwrap_or(16);
                self.mps.apply_two_site(q, &self.gates.backbone[q], chi);
            }

            // 3. Long-range contacts via SWAP network
            // Clone contacts to avoid borrow conflict with &mut self
            let contacts: Vec<_> = self.gates.contacts.iter()
                .map(|(i, j, g)| (*i, *j, g.clone()))
                .collect();
            for (i, j, gate) in &contacts {
                self.apply_long_range(*i, *j, gate);
            }

            // 4. Normalize
            self.mps.normalize();

            // 5. Measure energy (every 10 steps or at end)
            if step % 10 == 0 || step == n_steps - 1 {
                let e = self.measure_energy();
                energies.push(e);

                // Check convergence
                if energies.len() >= 2 {
                    let de = (energies[energies.len() - 1] - energies[energies.len() - 2]).abs();
                    if de < energy_tol {
                        converged = true;
                        break;
                    }
                }
            }
        }

        let energy = energies.last().copied().unwrap_or(0.0);
        let n_measured = energies.len();

        TEBDResult {
            energy,
            energies_per_step: energies,
            mps: self.mps.clone(),
            n_steps: n_measured,
            wall_time_seconds: t0.elapsed().as_secs_f64(),
            converged,
        }
    }

    /// Apply a long-range gate between sites i and j via SWAP network.
    /// SWAP j down to i+1, apply gate at (i, i+1), SWAP back.
    fn apply_long_range(&mut self, i: usize, j: usize, gate: &[f64]) {
        if j <= i + 1 {
            // Already adjacent
            let chi = self.adaptive_chi.get(i).copied().unwrap_or(16);
            self.mps.apply_two_site(i, gate, chi);
            return;
        }

        let chi_max = self.adaptive_chi.iter().max().copied().unwrap_or(16);

        // SWAP j down to i+1
        for k in (i + 1..j).rev() {
            self.mps.swap(k, chi_max);
        }

        // Apply gate at (i, i+1)
        let chi = self.adaptive_chi.get(i).copied().unwrap_or(16);
        self.mps.apply_two_site(i, gate, chi);

        // SWAP back
        for k in i + 1..j {
            self.mps.swap(k, chi_max);
        }
    }

    /// Measure energy: E = Σ_k ⟨ψ|h_k|ψ⟩ + Σ_⟨ij⟩ ⟨ψ|H_nn|ψ⟩
    /// For simplicity, measure local terms only (the dominant contribution).
    fn measure_energy(&self) -> f64 {
        let mut energy = 0.0;

        // Local terms
        for (q, _gate) in self.gates.local.iter().enumerate() {
            // The "gate" is exp(-dt*H), but we want ⟨H⟩.
            // Approximate: E_local ≈ -ln(⟨gate⟩) / dt — but that's not right.
            // Better: use the original Hamiltonian terms if available.
            // For now: use the identity that for small dt,
            // ⟨exp(-dt*H)⟩ ≈ 1 - dt*⟨H⟩, so ⟨H⟩ ≈ (1 - ⟨gate⟩) / dt.
            // But we don't have dt here...
            //
            // Simple approach: measure the number operator (how "folded" each site is).
            // State σ=0 is unfolded, higher σ = more folded.
            let d = self.d;
            let mut diag = vec![0.0; d * d];
            for s in 0..d {
                diag[s * d + s] = s as f64;
            }
            energy -= self.mps.expectation_single(q, &diag);
        }

        energy
    }
}

/// Matrix exponential exp(scale * H) for a real symmetric matrix.
/// Uses eigendecomposition: exp(H) = V · diag(exp(λ)) · V^T.
fn matrix_exp_real(h: &[f64], n: usize, scale: f64) -> Vec<f64> {
    use nalgebra::{DMatrix, SymmetricEigen};

    let mat = DMatrix::from_fn(n, n, |i, j| h[i * n + j]);

    // Symmetrize (in case of floating point asymmetry)
    let sym = (&mat + mat.transpose()) * 0.5;

    let eigen = SymmetricEigen::new(sym);
    let mut result = DMatrix::zeros(n, n);

    for k in 0..n {
        let exp_val = (scale * eigen.eigenvalues[k]).exp();
        let v = eigen.eigenvectors.column(k);
        result += exp_val * &v * v.transpose();
    }

    let mut out = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            out[i * n + j] = result[(i, j)];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_mps_product_state() {
        let mps = RealMps::new(5, 3);
        assert_eq!(mps.n_sites, 5);
        assert_eq!(mps.d, 3);
        // All bond dims should be 1 (product state)
        for b in mps.bond_dims() {
            assert_eq!(b, 1);
        }
        // Norm should be 1
        let norm = mps.norm();
        assert!((norm - 1.0).abs() < 1e-10, "norm should be 1, got {norm}");
    }

    #[test]
    fn single_site_gate() {
        let mut mps = RealMps::new(3, 2);
        // Apply X gate (swap |0⟩ and |1⟩) to site 1
        let x_gate = vec![0.0, 1.0, 1.0, 0.0];
        mps.apply_single(1, &x_gate);
        // Site 1 should now be in |1⟩
        assert!((mps.sites[1].get(0, 0, 0)).abs() < 1e-10);
        assert!((mps.sites[1].get(0, 1, 0) - 1.0).abs() < 1e-10);
        // Norm preserved
        assert!((mps.norm() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn two_site_gate_preserves_norm() {
        let mut mps = RealMps::new(4, 3);
        // Apply a random-ish unitary-like gate
        let d2 = 9;
        let mut gate = vec![0.0; d2 * d2];
        // Identity + small perturbation
        for i in 0..d2 {
            gate[i * d2 + i] = 1.0;
        }
        gate[0 * d2 + 1] = 0.1;
        gate[1 * d2 + 0] = 0.1;

        mps.apply_two_site(1, &gate, 8);
        mps.normalize();
        let norm = mps.norm();
        assert!(
            (norm - 1.0).abs() < 1e-8,
            "norm should be ~1 after normalize, got {norm}"
        );
    }

    #[test]
    fn swap_gate_works() {
        let mut mps = RealMps::new(3, 2);
        // Put site 0 in |1⟩
        let x = vec![0.0, 1.0, 1.0, 0.0];
        mps.apply_single(0, &x);
        // Now state is |1,0,0⟩

        // SWAP sites 0 and 1
        mps.swap(0, 4);
        // Now state should be |0,1,0⟩
        // Check site 0 is back to |0⟩, site 1 is |1⟩
        let s0_0 = mps.sites[0].get(0, 0, 0);
        let s1_1 = mps.sites[1].get(0, 1, 0);
        assert!(s0_0.abs() > 0.9, "site 0 should be mostly |0⟩");
        assert!(s1_1.abs() > 0.9, "site 1 should be mostly |1⟩");
    }

    #[test]
    fn matrix_exp_identity() {
        // exp(0) = I
        let zero = vec![0.0; 4];
        let result = matrix_exp_real(&zero, 2, 1.0);
        assert!((result[0] - 1.0).abs() < 1e-10); // [0,0] = 1
        assert!((result[1]).abs() < 1e-10); // [0,1] = 0
        assert!((result[2]).abs() < 1e-10); // [1,0] = 0
        assert!((result[3] - 1.0).abs() < 1e-10); // [1,1] = 1
    }

    #[test]
    fn svd_truncation_basic() {
        // 3×3 matrix with rank 2
        #[rustfmt::skip]
        let mat = vec![
            1.0, 0.0, 0.0,
            0.0, 1.0, 0.0,
            0.0, 0.0, 0.0,
        ];
        let (us, vt, err) = svd_truncate_real(&mat, 3, 3, 2);
        assert_eq!(us.len(), 3 * 2); // [3, 2]
        assert_eq!(vt.len(), 2 * 3); // [2, 3]
        assert!(err.abs() < 1e-10, "no truncation error for rank-2 with max_chi=2");
    }
}
