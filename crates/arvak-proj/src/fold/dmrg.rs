use nalgebra::DMatrix;

/// Configuration for the two-site DMRG solver.
pub struct DMRGConfig {
    pub max_sweeps: usize,
    pub energy_tol: f64,
    /// Adaptive bond dimension per bond (from commensurability).
    /// Empty means uniform default.
    pub chi_profile: Vec<usize>,
    pub lanczos_max_iter: usize,
    pub lanczos_tol: f64,
    /// Noise amplitude per sweep (empty = no noise).
    pub noise: Vec<f64>,
}

impl Default for DMRGConfig {
    fn default() -> Self {
        Self {
            max_sweeps: 20,
            energy_tol: 1e-8,
            chi_profile: Vec::new(),
            lanczos_max_iter: 100,
            lanczos_tol: 1e-12,
            noise: vec![1e-4, 1e-5, 1e-6, 0.0],
        }
    }
}

/// Result of a DMRG ground-state optimization.
pub struct DMRGResult {
    pub energy: f64,
    pub energies_per_sweep: Vec<f64>,
    pub mps_tensors: Vec<Vec<f64>>,
    pub mps_bond_dims: Vec<usize>,
    pub phys_dim: usize,
    pub converged: bool,
    pub n_sweeps: usize,
    pub wall_time_seconds: f64,
}

/// Two-site DMRG solver for real-valued MPS ground states.
///
/// MPS tensors are stored as `Vec<f64>` in row-major order with shape
/// `[chi_l, d, chi_r]` per site. Environment blocks have shape
/// `[mps_chi, mpo_chi, mps_chi]`.
pub struct DMRG {
    mpo: super::mpo::MPO,
    /// MPS tensors: `mps[k]` has shape `[chi_l, d, chi_r]`.
    mps: Vec<Vec<f64>>,
    /// Dimensions `[chi_l, d, chi_r]` for each site tensor.
    mps_dims: Vec<[usize; 3]>,
    config: DMRGConfig,
    n_sites: usize,
    d: usize,
    /// Left environment blocks: `left_env[k]` shape `[mps_chi, mpo_chi, mps_chi]`.
    left_env: Vec<Vec<f64>>,
    /// Right environment blocks: `right_env[k]` shape `[mps_chi, mpo_chi, mps_chi]`.
    right_env: Vec<Vec<f64>>,
}

// ---------------------------------------------------------------------------
// Deterministic xorshift64 RNG (no external dependency)
// ---------------------------------------------------------------------------

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }

    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        // Map to (0, 1)
        (x as f64) / (u64::MAX as f64)
    }
}

// ---------------------------------------------------------------------------
// Index helpers
// ---------------------------------------------------------------------------

/// Row-major index for a 3-D tensor `[d0, d1, d2]`.
#[inline]
fn idx3(i0: usize, i1: usize, i2: usize, d1: usize, d2: usize) -> usize {
    (i0 * d1 + i1) * d2 + i2
}

/// Row-major index for a 4-D tensor `[d0, d1, d2, d3]`.
#[inline]
fn idx4(i0: usize, i1: usize, i2: usize, i3: usize, d1: usize, d2: usize, d3: usize) -> usize {
    ((i0 * d1 + i1) * d2 + i2) * d3 + i3
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl DMRG {
    /// Create a new DMRG solver for the given MPO Hamiltonian.
    ///
    /// The MPS is initialised with small random values and right-canonicalised
    /// via QR decomposition.
    pub fn new(mpo: super::mpo::MPO, config: DMRGConfig) -> Self {
        let n = mpo.n_sites;
        let d = mpo.phys_dim;

        let mut solver = Self {
            n_sites: n,
            d,
            mps: Vec::new(),
            mps_dims: Vec::new(),
            left_env: Vec::new(),
            right_env: Vec::new(),
            mpo,
            config,
        };

        if n > 0 {
            solver.initialize_mps();
        }
        solver
    }

    /// Main DMRG loop: sweep left/right until convergence.
    pub fn solve(&mut self) -> DMRGResult {
        let start = std::time::Instant::now();

        if self.n_sites == 0 {
            return DMRGResult {
                energy: 0.0,
                energies_per_sweep: Vec::new(),
                mps_tensors: self.mps.clone(),
                mps_bond_dims: self.current_bond_dims(),
                phys_dim: self.d,
                converged: true,
                n_sweeps: 0,
                wall_time_seconds: 0.0,
            };
        }

        if self.n_sites == 1 {
            // Single site: no bonds to optimise.  Energy = minimum eigenvalue
            // of the single MPO tensor treated as a d x d matrix.
            let energy = self.single_site_energy();
            return DMRGResult {
                energy,
                energies_per_sweep: vec![energy],
                mps_tensors: self.mps.clone(),
                mps_bond_dims: Vec::new(),
                phys_dim: self.d,
                converged: true,
                n_sweeps: 1,
                wall_time_seconds: start.elapsed().as_secs_f64(),
            };
        }

        // Build initial right environments (right to left).
        self.build_right_environments();

        // Initialise left_env[0] to trivial scalar 1.
        self.left_env[0] = vec![1.0];

        let mut energies: Vec<f64> = Vec::new();
        let mut converged = false;

        for sweep in 0..self.config.max_sweeps {
            let noise = self.config.noise.get(sweep).copied().unwrap_or(0.0);

            // Left-to-right half sweep
            let mut energy = 0.0;
            for bond in 0..self.n_sites - 1 {
                energy = self.optimize_bond(bond, true, noise);
            }

            // Right-to-left half sweep
            for bond in (0..self.n_sites - 1).rev() {
                energy = self.optimize_bond(bond, false, noise);
            }

            energies.push(energy);

            // Convergence check
            if energies.len() >= 2 {
                let de = (energies[energies.len() - 1] - energies[energies.len() - 2]).abs();
                if de < self.config.energy_tol {
                    converged = true;
                    break;
                }
            }
        }

        let wall = start.elapsed().as_secs_f64();
        let n_sweeps = energies.len();
        let energy = energies.last().copied().unwrap_or(0.0);

        DMRGResult {
            energy,
            energies_per_sweep: energies,
            mps_tensors: self.mps.clone(),
            mps_bond_dims: self.current_bond_dims(),
            phys_dim: self.d,
            converged,
            n_sweeps,
            wall_time_seconds: wall,
        }
    }

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise random MPS in right-canonical form.
    fn initialize_mps(&mut self) {
        let n = self.n_sites;
        let d = self.d;
        let default_chi = 2; // initial bond dimension (small)
        let mut rng = Xorshift64::new(42);

        // Determine initial bond dimensions
        let mut bond_dims = vec![default_chi; n.saturating_sub(1)];
        // Clamp boundary bonds: chi cannot exceed d^k or d^(n-k)
        for (k, bd) in bond_dims.iter_mut().enumerate() {
            let left_max = d.pow(u32::try_from(k + 1).unwrap_or(u32::MAX).min(10));
            let right_max = d.pow(u32::try_from(n - 1 - k).unwrap_or(u32::MAX).min(10));
            *bd = (*bd).min(left_max).min(right_max);
        }

        // Create random tensors
        self.mps.clear();
        self.mps_dims.clear();
        for k in 0..n {
            let chi_l = if k == 0 { 1 } else { bond_dims[k - 1] };
            let chi_r = if k == n - 1 { 1 } else { bond_dims[k] };
            let size = chi_l * d * chi_r;
            let mut t = Vec::with_capacity(size);
            for _ in 0..size {
                t.push(rng.next_f64() - 0.5);
            }
            self.mps.push(t);
            self.mps_dims.push([chi_l, d, chi_r]);
        }

        // Right-canonicalise: QR from right to left
        for k in (1..n).rev() {
            let [chi_l, d_k, chi_r] = self.mps_dims[k];
            // Reshape mps[k] as (chi_l, d_k * chi_r) and do QR
            let rows = chi_l;
            let cols = d_k * chi_r;
            let mat = DMatrix::from_row_slice(rows, cols, &self.mps[k]);
            // Transpose to get (cols, rows), do QR there, get R^T as the
            // left factor.
            // Actually: we want M = Q R where Q is (chi_l, chi_l) and R
            // is (chi_l, d*chi_r).  But chi_l <= d*chi_r typically.
            // nalgebra QR: for (m, n) with m >= n gives Q (m,n) and R (n,n).
            // We want thin QR of M^T: (d*chi_r, chi_l) = Q_t R_t
            // Then M = R_t^T Q_t^T.
            // Q_t^T is the new mps[k] (shape chi_new, d_k * chi_r)
            // R_t^T is absorbed into mps[k-1].
            let mt = mat.transpose();
            let qr = mt.qr();
            let q_t = qr.q(); // (d*chi_r, min(d*chi_r, chi_l))
            let r_t = qr.r(); // (min(d*chi_r, chi_l), chi_l)

            let chi_new = r_t.nrows().min(r_t.ncols()); // = min(d*chi_r, chi_l) = chi_l typically

            // New mps[k]: Q_t^T has shape (chi_new, d*chi_r)
            let qt_t = q_t.columns(0, chi_new).transpose();
            let mut new_tensor = vec![0.0; chi_new * d_k * chi_r];
            for r in 0..chi_new {
                for c in 0..d_k * chi_r {
                    new_tensor[r * (d_k * chi_r) + c] = qt_t[(r, c)];
                }
            }
            self.mps[k] = new_tensor;
            self.mps_dims[k] = [chi_new, d_k, chi_r];

            // R_t^T has shape (chi_l, chi_new)
            // Absorb into mps[k-1]: reshape mps[k-1] as (chi_l_prev * d_{k-1}, chi_l)
            // then multiply by R_t^T to get (chi_l_prev * d_{k-1}, chi_new)
            let [chi_l_prev, d_prev, _old_chi_r] = self.mps_dims[k - 1];
            let rt_t = r_t.transpose(); // (chi_l, chi_new)
            let m_prev = DMatrix::from_row_slice(chi_l_prev * d_prev, _old_chi_r, &self.mps[k - 1]);
            let m_new = &m_prev * &rt_t; // (chi_l_prev * d_prev, chi_new)
            let mut new_prev = vec![0.0; chi_l_prev * d_prev * chi_new];
            for r in 0..chi_l_prev * d_prev {
                for c in 0..chi_new {
                    new_prev[r * chi_new + c] = m_new[(r, c)];
                }
            }
            self.mps[k - 1] = new_prev;
            self.mps_dims[k - 1] = [chi_l_prev, d_prev, chi_new];
        }

        // Normalise mps[0]
        let norm: f64 = self.mps[0].iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 1e-15 {
            for v in &mut self.mps[0] {
                *v /= norm;
            }
        }

        // Allocate environment storage
        self.left_env = vec![Vec::new(); n];
        self.right_env = vec![Vec::new(); n];
    }

    // -----------------------------------------------------------------------
    // Environment blocks
    // -----------------------------------------------------------------------

    /// Build all right environment blocks from right to left.
    fn build_right_environments(&mut self) {
        let n = self.n_sites;
        // right_env[n-1] is a trivial scalar 1
        self.right_env[n - 1] = vec![1.0];

        for k in (0..n - 1).rev() {
            self.update_right_env(k);
        }
    }

    /// Update `right_env[k]` by contracting site k+1 into `right_env[k+1]`.
    ///
    /// `right_env[k]` shape: `[chi_r_mps_k, chi_r_mpo_k, chi_r_mps_k]`
    /// where chi_r refers to the bond between sites k and k+1.
    fn update_right_env(&mut self, k: usize) {
        let site = k + 1;
        let [chi_l, d_s, chi_r] = self.mps_dims[site];
        let mpo_chi_l = self.mpo_chi_l(site);
        let mpo_chi_r = self.mpo_chi_r(site);

        // right_env[site] shape: [chi_r, mpo_chi_r, chi_r]
        let r_env = &self.right_env[site];

        // Result shape: [chi_l, mpo_chi_l, chi_l]
        let mut new_env = vec![0.0; chi_l * mpo_chi_l * chi_l];

        // Contract:
        // new_env[a_l, w_l, b_l] =
        //   sum_{s, sp, a_r, w_r, b_r}
        //     mps[site][a_l, s, a_r]^* * W[site][w_l, s, sp, w_r]
        //     * mps[site][b_l, sp, b_r] * right_env[site][a_r, w_r, b_r]
        // (mps is real so ^* is identity)

        for a_l in 0..chi_l {
            for b_l in 0..chi_l {
                for w_l in 0..mpo_chi_l {
                    let mut val = 0.0;
                    for s in 0..d_s {
                        for sp in 0..d_s {
                            for w_r in 0..mpo_chi_r {
                                let w_elem = self.mpo.get(site, w_l, s, sp, w_r);
                                if w_elem.abs() < 1e-15 {
                                    continue;
                                }
                                for a_r in 0..chi_r {
                                    let mps_a = self.mps[site][idx3(a_l, s, a_r, d_s, chi_r)];
                                    if mps_a.abs() < 1e-15 {
                                        continue;
                                    }
                                    for b_r in 0..chi_r {
                                        let mps_b = self.mps[site][idx3(b_l, sp, b_r, d_s, chi_r)];
                                        let r_val = r_env[idx3(a_r, w_r, b_r, mpo_chi_r, chi_r)];
                                        val += mps_a * w_elem * mps_b * r_val;
                                    }
                                }
                            }
                        }
                    }
                    new_env[idx3(a_l, w_l, b_l, mpo_chi_l, chi_l)] = val;
                }
            }
        }

        self.right_env[k] = new_env;
    }

    /// Update `left_env[k+1]` by contracting site k into `left_env[k]`.
    ///
    /// `left_env[k+1]` shape: `[chi_r_mps_k, chi_r_mpo_k, chi_r_mps_k]`
    fn update_left_env(&mut self, k: usize) {
        let [chi_l, d_s, chi_r] = self.mps_dims[k];
        let mpo_chi_l = self.mpo_chi_l(k);
        let mpo_chi_r = self.mpo_chi_r(k);

        let l_env = &self.left_env[k];

        let mut new_env = vec![0.0; chi_r * mpo_chi_r * chi_r];

        for a_r in 0..chi_r {
            for b_r in 0..chi_r {
                for w_r in 0..mpo_chi_r {
                    let mut val = 0.0;
                    for s in 0..d_s {
                        for sp in 0..d_s {
                            for w_l in 0..mpo_chi_l {
                                let w_elem = self.mpo.get(k, w_l, s, sp, w_r);
                                if w_elem.abs() < 1e-15 {
                                    continue;
                                }
                                for a_l in 0..chi_l {
                                    let mps_a = self.mps[k][idx3(a_l, s, a_r, d_s, chi_r)];
                                    if mps_a.abs() < 1e-15 {
                                        continue;
                                    }
                                    for b_l in 0..chi_l {
                                        let mps_b = self.mps[k][idx3(b_l, sp, b_r, d_s, chi_r)];
                                        let l_val = l_env[idx3(a_l, w_l, b_l, mpo_chi_l, chi_l)];
                                        val += mps_a * w_elem * mps_b * l_val;
                                    }
                                }
                            }
                        }
                    }
                    new_env[idx3(a_r, w_r, b_r, mpo_chi_r, chi_r)] = val;
                }
            }
        }

        if k + 1 < self.n_sites {
            self.left_env[k + 1] = new_env;
        }
    }

    // -----------------------------------------------------------------------
    // Bond optimisation
    // -----------------------------------------------------------------------

    /// Optimise the bond between sites `k` and `k+1`.
    ///
    /// If `moving_right` is true, the left-canonical form is enforced on site
    /// k and the left environment is updated.  Otherwise, right-canonical on
    /// k+1 and the right environment is updated.
    ///
    /// Returns the variational energy from the Lanczos solve.
    fn optimize_bond(&mut self, k: usize, moving_right: bool, noise: f64) -> f64 {
        let k1 = k + 1;
        let [chi_l, d0, _chi_m0] = self.mps_dims[k];
        let [_chi_m1, d1, chi_r] = self.mps_dims[k1];
        let d = self.d;
        debug_assert_eq!(d0, d);
        debug_assert_eq!(d1, d);

        // Form two-site tensor theta[a_l, s0, s1, a_r]
        let chi_m = self.mps_dims[k][2]; // shared bond
        let theta_len = chi_l * d * d * chi_r;
        let mut theta = vec![0.0; theta_len];
        for a_l in 0..chi_l {
            for s0 in 0..d {
                for s1 in 0..d {
                    for a_r in 0..chi_r {
                        let mut v = 0.0;
                        for m in 0..chi_m {
                            v += self.mps[k][idx3(a_l, s0, m, d, chi_m)]
                                * self.mps[k1][idx3(m, s1, a_r, d, chi_r)];
                        }
                        theta[idx4(a_l, s0, s1, a_r, d, d, chi_r)] = v;
                    }
                }
            }
        }

        // Gather environment and MPO info for the effective Hamiltonian
        let mpo_chi_l = self.mpo_chi_l(k);
        let mpo_chi_m = self.mpo_chi_r(k); // = mpo_chi_l of site k+1
        let mpo_chi_r = self.mpo_chi_r(k1);

        let l_env = self.left_env[k].clone();
        let r_env = self.right_env[k1].clone();

        // Lanczos eigensolver
        let (energy, theta_opt) = self.lanczos(
            &theta, chi_l, chi_r, mpo_chi_l, mpo_chi_m, mpo_chi_r, k, &l_env, &r_env,
        );

        // SVD of theta reshaped as (chi_l * d, d * chi_r)
        let rows = chi_l * d;
        let cols = d * chi_r;
        let mat = DMatrix::from_row_slice(rows, cols, &theta_opt);
        let svd = mat.svd(true, true);
        let u_mat = svd.u.expect("SVD failed to produce U");
        let vt_mat = svd.v_t.expect("SVD failed to produce V^T");
        let sigma = &svd.singular_values;

        // Determine truncation bond dimension
        let max_chi = if self.config.chi_profile.is_empty() {
            16
        } else {
            self.config.chi_profile.get(k).copied().unwrap_or(16)
        };
        let n_sv = sigma.len();
        let chi_new = n_sv.min(max_chi).max(1);

        // Build U_trunc * S and V_trunc
        if moving_right {
            // mps[k] = U[:, :chi_new] reshaped to (chi_l, d, chi_new) — left canonical
            let mut new_k = vec![0.0; chi_l * d * chi_new];
            for r in 0..rows {
                for c in 0..chi_new {
                    new_k[r * chi_new + c] = u_mat[(r, c)];
                }
            }
            self.mps[k] = new_k;
            self.mps_dims[k] = [chi_l, d, chi_new];

            // mps[k+1] = diag(S[:chi_new]) * V^T[:chi_new, :] reshaped to (chi_new, d, chi_r)
            let mut new_k1 = vec![0.0; chi_new * d * chi_r];
            for r in 0..chi_new {
                let s_val = sigma[r] + noise;
                for c in 0..cols {
                    new_k1[r * cols + c] = s_val * vt_mat[(r, c)];
                }
            }
            self.mps[k1] = new_k1;
            self.mps_dims[k1] = [chi_new, d, chi_r];

            // Update left environment
            self.update_left_env(k);
        } else {
            // mps[k] = U[:, :chi_new] * diag(S[:chi_new]) reshaped to (chi_l, d, chi_new)
            let mut new_k = vec![0.0; chi_l * d * chi_new];
            for r in 0..rows {
                for c in 0..chi_new {
                    new_k[r * chi_new + c] = u_mat[(r, c)] * (sigma[c] + noise);
                }
            }
            self.mps[k] = new_k;
            self.mps_dims[k] = [chi_l, d, chi_new];

            // mps[k+1] = V^T[:chi_new, :] reshaped to (chi_new, d, chi_r) — right canonical
            let mut new_k1 = vec![0.0; chi_new * d * chi_r];
            for r in 0..chi_new {
                for c in 0..cols {
                    new_k1[r * cols + c] = vt_mat[(r, c)];
                }
            }
            self.mps[k1] = new_k1;
            self.mps_dims[k1] = [chi_new, d, chi_r];

            // Update right environment
            self.update_right_env(k);
        }

        energy
    }

    // -----------------------------------------------------------------------
    // Lanczos eigensolver
    // -----------------------------------------------------------------------

    /// Lanczos iteration for the lowest eigenpair of the effective two-site
    /// Hamiltonian.  The Hamiltonian is applied as a matrix-free linear
    /// operator via [`apply_effective_hamiltonian`].
    #[allow(clippy::too_many_arguments)]
    fn lanczos(
        &self,
        theta0: &[f64],
        chi_l: usize,
        chi_r: usize,
        mpo_chi_l: usize,
        mpo_chi_m: usize,
        mpo_chi_r: usize,
        site_k: usize,
        l_env: &[f64],
        r_env: &[f64],
    ) -> (f64, Vec<f64>) {
        let d = self.d;
        let n = chi_l * d * d * chi_r;
        let max_iter = self.config.lanczos_max_iter.min(n);
        let tol = self.config.lanczos_tol;

        // Normalise initial vector
        let norm0: f64 = theta0.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mut v: Vec<f64> = if norm0 > 1e-15 {
            theta0.iter().map(|x| x / norm0).collect()
        } else {
            let mut v0 = vec![0.0; n];
            if !v0.is_empty() {
                v0[0] = 1.0;
            }
            v0
        };

        let mut alphas: Vec<f64> = Vec::with_capacity(max_iter);
        let mut betas: Vec<f64> = Vec::with_capacity(max_iter);
        let mut v_basis: Vec<Vec<f64>> = Vec::with_capacity(max_iter);

        let mut v_prev: Vec<f64> = vec![0.0; n];
        let mut beta_prev: f64 = 0.0;

        for _iter in 0..max_iter {
            v_basis.push(v.clone());

            let mut w = self.apply_effective_hamiltonian(
                &v, chi_l, chi_r, mpo_chi_l, mpo_chi_m, mpo_chi_r, site_k, l_env, r_env,
            );

            let alpha: f64 = w.iter().zip(&v).map(|(a, b)| a * b).sum();
            alphas.push(alpha);

            // w = w - alpha * v - beta_prev * v_prev
            for i in 0..n {
                w[i] -= alpha * v[i] + beta_prev * v_prev[i];
            }

            // Full re-orthogonalisation against all previous Lanczos vectors
            for vj in &v_basis {
                let overlap: f64 = w.iter().zip(vj).map(|(a, b)| a * b).sum();
                for i in 0..n {
                    w[i] -= overlap * vj[i];
                }
            }

            let beta: f64 = w.iter().map(|x| x * x).sum::<f64>().sqrt();
            betas.push(beta);

            if beta < tol {
                break;
            }

            v_prev.clone_from(&v);
            v = w.iter().map(|x| x / beta).collect();
            beta_prev = beta;
        }

        // Solve tridiagonal eigenvalue problem
        let m = alphas.len();
        if m == 0 {
            return (0.0, theta0.to_vec());
        }

        let (eval, evec_tri) = Self::tridiag_lowest_eigenpair(&alphas, &betas);

        // Reconstruct eigenvector in the full space
        let mut result = vec![0.0; n];
        for (j, coeff) in evec_tri.iter().enumerate() {
            for i in 0..n {
                result[i] += coeff * v_basis[j][i];
            }
        }

        // Normalise
        let norm: f64 = result.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm > 1e-15 {
            for x in &mut result {
                *x /= norm;
            }
        }

        (eval, result)
    }

    /// Apply the effective two-site Hamiltonian to a theta vector.
    ///
    /// ```text
    /// result[a_l, s_k, s_{k+1}, a_r] =
    ///     sum  L[a_l', w_l, a_l] * W_k[w_l, s_k, s'_k, w_m]
    ///          * W_{k+1}[w_m, s_{k+1}, s'_{k+1}, w_r]
    ///          * R[a_r', w_r, a_r] * theta[a_l', s'_k, s'_{k+1}, a_r']
    /// ```
    ///
    /// Contraction is done index-by-index for correctness.  The dimensions
    /// are typically small enough (chi ~ 16, d ~ 9) that this is adequate.
    #[allow(clippy::too_many_arguments)]
    fn apply_effective_hamiltonian(
        &self,
        theta: &[f64],
        chi_l: usize,
        chi_r: usize,
        mpo_chi_l: usize,
        mpo_chi_m: usize,
        mpo_chi_r: usize,
        site_k: usize,
        l_env: &[f64],
        r_env: &[f64],
    ) -> Vec<f64> {
        let d = self.d;
        let site_k1 = site_k + 1;
        let n = chi_l * d * d * chi_r;
        let mut result = vec![0.0; n];

        // L[al, wl, bl] shape [chi_l, mpo_chi_l, chi_l]
        // R[ar, wr, br] shape [chi_r, mpo_chi_r, chi_r]
        // theta[bl, sp_k, sp_k1, br] shape [chi_l, d, d, chi_r]
        // W_k[wl, sk, sp_k, wm] shape [mpo_chi_l, d, d, mpo_chi_m]
        // W_{k+1}[wm, sk1, sp_k1, wr] shape [mpo_chi_m, d, d, mpo_chi_r]

        for al in 0..chi_l {
            for sk in 0..d {
                for sk1 in 0..d {
                    for ar in 0..chi_r {
                        let mut val = 0.0;
                        for wl in 0..mpo_chi_l {
                            for wm in 0..mpo_chi_m {
                                for wr in 0..mpo_chi_r {
                                    for bl in 0..chi_l {
                                        let l_val = l_env[idx3(al, wl, bl, mpo_chi_l, chi_l)];
                                        if l_val.abs() < 1e-15 {
                                            continue;
                                        }
                                        for br in 0..chi_r {
                                            let r_val = r_env[idx3(ar, wr, br, mpo_chi_r, chi_r)];
                                            if r_val.abs() < 1e-15 {
                                                continue;
                                            }
                                            let lr = l_val * r_val;
                                            for spk in 0..d {
                                                let wk_elem = self.mpo.get(site_k, wl, sk, spk, wm);
                                                if wk_elem.abs() < 1e-15 {
                                                    continue;
                                                }
                                                for spk1 in 0..d {
                                                    let wk1_elem =
                                                        self.mpo.get(site_k1, wm, sk1, spk1, wr);
                                                    if wk1_elem.abs() < 1e-15 {
                                                        continue;
                                                    }
                                                    let th =
                                                        theta[idx4(bl, spk, spk1, br, d, d, chi_r)];
                                                    val += lr * wk_elem * wk1_elem * th;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        result[idx4(al, sk, sk1, ar, d, d, chi_r)] = val;
                    }
                }
            }
        }

        result
    }

    // -----------------------------------------------------------------------
    // Tridiagonal eigenvalue solver (QR algorithm)
    // -----------------------------------------------------------------------

    /// Find the lowest eigenpair of the tridiagonal matrix defined by
    /// diagonal `alphas` and sub-diagonal `betas`.
    fn tridiag_lowest_eigenpair(alphas: &[f64], betas: &[f64]) -> (f64, Vec<f64>) {
        let m = alphas.len();
        if m == 1 {
            return (alphas[0], vec![1.0]);
        }

        // Build full tridiagonal matrix and use nalgebra's symmetric eigendecomposition
        let mut mat = DMatrix::zeros(m, m);
        for i in 0..m {
            mat[(i, i)] = alphas[i];
            if i + 1 < m && i < betas.len() {
                mat[(i, i + 1)] = betas[i];
                mat[(i + 1, i)] = betas[i];
            }
        }

        let eig = mat.symmetric_eigen();
        // Find index of smallest eigenvalue
        let mut min_idx = 0;
        let mut min_val = eig.eigenvalues[0];
        for i in 1..m {
            if eig.eigenvalues[i] < min_val {
                min_val = eig.eigenvalues[i];
                min_idx = i;
            }
        }

        let evec: Vec<f64> = eig.eigenvectors.column(min_idx).iter().copied().collect();
        (min_val, evec)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// MPO left bond dimension at site k.
    fn mpo_chi_l(&self, k: usize) -> usize {
        if k == 0 { 1 } else { self.mpo.bond_dims[k - 1] }
    }

    /// MPO right bond dimension at site k.
    fn mpo_chi_r(&self, k: usize) -> usize {
        if k == self.n_sites - 1 {
            1
        } else {
            self.mpo.bond_dims[k]
        }
    }

    /// Current MPS bond dimensions (between each pair of adjacent sites).
    fn current_bond_dims(&self) -> Vec<usize> {
        if self.n_sites <= 1 {
            return Vec::new();
        }
        (0..self.n_sites - 1).map(|k| self.mps_dims[k][2]).collect()
    }

    /// Single-site energy: minimum eigenvalue of the MPO tensor at site 0.
    fn single_site_energy(&self) -> f64 {
        let d = self.d;
        // MPO tensor at site 0 has shape (1, d, d, 1) — extract the d x d matrix
        let mut mat = DMatrix::zeros(d, d);
        for s in 0..d {
            for sp in 0..d {
                mat[(s, sp)] = self.mpo.get(0, 0, s, sp, 0);
            }
        }
        let eig = mat.symmetric_eigen();
        eig.eigenvalues
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::hamiltonian::{LongRangeTerm, ProteinHamiltonian};
    use crate::fold::mpo::MPO;

    fn make_zero_hamiltonian(n: usize, d: usize) -> ProteinHamiltonian {
        ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms: (0..n).map(|_| vec![0.0; d * d]).collect(),
            nn_terms: (0..n.saturating_sub(1))
                .map(|_| vec![0.0; d * d * d * d])
                .collect(),
            long_range_terms: Vec::new(),
        }
    }

    fn make_local_hamiltonian(n: usize, d: usize) -> ProteinHamiltonian {
        // Each site has h_local = diag(0, 1, 2, ..., d-1)
        let mut local_terms = Vec::new();
        for _ in 0..n {
            let mut h = vec![0.0; d * d];
            for s in 0..d {
                h[s * d + s] = s as f64;
            }
            local_terms.push(h);
        }
        ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms,
            nn_terms: (0..n.saturating_sub(1))
                .map(|_| vec![0.0; d * d * d * d])
                .collect(),
            long_range_terms: Vec::new(),
        }
    }

    #[test]
    fn zero_hamiltonian_gives_zero_energy() {
        let ham = make_zero_hamiltonian(4, 3);
        let mpo = MPO::from_hamiltonian(&ham, None);
        let config = DMRGConfig {
            max_sweeps: 10,
            ..DMRGConfig::default()
        };
        let mut dmrg = DMRG::new(mpo, config);
        let result = dmrg.solve();
        assert!(
            result.energy.abs() < 1e-6,
            "expected ~0 energy for zero Hamiltonian, got {}",
            result.energy
        );
    }

    #[test]
    fn local_hamiltonian_energy_equals_sum_of_minima() {
        // Each site: eigenvalues 0, 1, 2.  Minimum per site = 0.
        // Total ground state energy = 0.
        let ham = make_local_hamiltonian(4, 3);
        let mpo = MPO::from_hamiltonian(&ham, None);
        let config = DMRGConfig {
            max_sweeps: 15,
            ..DMRGConfig::default()
        };
        let mut dmrg = DMRG::new(mpo, config);
        let result = dmrg.solve();
        assert!(
            result.energy.abs() < 1e-6,
            "expected ~0 energy (sum of minima), got {}",
            result.energy
        );
    }

    #[test]
    fn local_hamiltonian_nonzero_minimum() {
        // Each site: eigenvalues 1, 2, 3 → minimum per site = 1.
        // 4 sites → total = 4.
        let n = 4;
        let d = 3;
        let mut local_terms = Vec::new();
        for _ in 0..n {
            let mut h = vec![0.0; d * d];
            for s in 0..d {
                h[s * d + s] = (s + 1) as f64;
            }
            local_terms.push(h);
        }
        let ham = ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms,
            nn_terms: (0..n - 1).map(|_| vec![0.0; d * d * d * d]).collect(),
            long_range_terms: Vec::new(),
        };
        let mpo = MPO::from_hamiltonian(&ham, None);
        let config = DMRGConfig {
            max_sweeps: 20,
            ..DMRGConfig::default()
        };
        let mut dmrg = DMRG::new(mpo, config);
        let result = dmrg.solve();
        assert!(
            (result.energy - 4.0).abs() < 1e-4,
            "expected energy ~4.0, got {}",
            result.energy
        );
    }

    #[test]
    fn energy_decreases_across_sweeps() {
        // Use a Hamiltonian with some interaction so it takes multiple sweeps
        let n = 4;
        let d = 3;
        let mut ham = make_local_hamiltonian(n, d);
        let mut op = vec![0.0; d * d];
        for s in 0..d {
            op[s * d + s] = 1.0;
        }
        ham.long_range_terms.push(LongRangeTerm {
            i: 0,
            j: 3,
            op_left: op.clone(),
            op_right: op,
            strength: -0.5,
        });
        let mpo = MPO::from_hamiltonian(&ham, None);
        let config = DMRGConfig {
            max_sweeps: 10,
            noise: Vec::new(), // no noise for monotonicity check
            ..DMRGConfig::default()
        };
        let mut dmrg = DMRG::new(mpo, config);
        let result = dmrg.solve();

        // Energy should be non-increasing (within numerical tolerance)
        for i in 1..result.energies_per_sweep.len() {
            assert!(
                result.energies_per_sweep[i] <= result.energies_per_sweep[i - 1] + 1e-10,
                "energy increased at sweep {}: {} -> {}",
                i,
                result.energies_per_sweep[i - 1],
                result.energies_per_sweep[i]
            );
        }
    }
}
