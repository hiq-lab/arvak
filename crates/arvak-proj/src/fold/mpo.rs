use std::collections::HashMap;

/// Matrix Product Operator for the protein Hamiltonian.
/// Each tensor has shape (chi_left, d, d, chi_right) stored as a flat `Vec<f64>`.
pub struct MPO {
    pub tensors: Vec<Vec<f64>>,
    pub bond_dims: Vec<usize>,
    pub phys_dim: usize,
    pub n_sites: usize,
}

impl MPO {
    /// Build MPO from Hamiltonian using Finite State Automaton construction.
    ///
    /// The FSA has states:
    /// - State 0: "idle" — identity propagation, collecting local+NN terms
    /// - States 1..M: one per active long-range term (propagating identity between i and j)
    /// - State M+1: "done" — accumulating completed interactions
    ///
    /// At site k:
    /// - If LR term m starts (i_m == k): row 0 -> col m, apply `op_left * strength`
    /// - If LR term m is active (i_m < k < j_m): row m -> col m, apply identity
    /// - If LR term m ends (j_m == k): row m -> col last, apply `op_right`
    /// - Always: row 0 -> col 0 = identity (start), row last -> col last = identity (end)
    /// - Local terms: row 0 -> col last
    /// - NN terms: handled by merging into local terms at adjacent sites
    pub fn from_hamiltonian(
        ham: &super::hamiltonian::ProteinHamiltonian,
        prune_threshold: Option<f64>,
    ) -> Self {
        let n = ham.n_sites;
        let d = ham.d;

        if n == 0 {
            return MPO {
                tensors: Vec::new(),
                bond_dims: Vec::new(),
                phys_dim: d,
                n_sites: 0,
            };
        }

        // Filter long-range terms by threshold
        let lr_terms: Vec<&super::hamiltonian::LongRangeTerm> = ham
            .long_range_terms
            .iter()
            .filter(|t| {
                if let Some(thresh) = prune_threshold {
                    t.strength.abs() >= thresh
                } else {
                    true
                }
            })
            .collect();

        // Bond dimension at bond k (between sites k and k+1):
        // 2 (idle + done) + number of LR threads active at that bond
        let mut bond_dims = Vec::with_capacity(n.saturating_sub(1));
        for k in 0..n.saturating_sub(1) {
            let n_threads = lr_terms.iter().filter(|t| t.i <= k && t.j > k + 1).count();
            bond_dims.push(2 + n_threads);
        }

        let mut tensors = Vec::with_capacity(n);

        for site in 0..n {
            let chi_l = if site == 0 { 1 } else { bond_dims[site - 1] };
            let chi_r = if site == n - 1 { 1 } else { bond_dims[site] };

            let size = chi_l * d * d * chi_r;
            let mut w = vec![0.0; size];

            // Thread maps: which LR terms are threaded through the left/right bonds
            let left_thread_map: HashMap<usize, usize> = lr_terms
                .iter()
                .enumerate()
                .filter(|(_, t)| t.i < site && t.j > site)
                .enumerate()
                .map(|(pos, (idx, _))| (idx, pos + 1))
                .collect();
            let right_thread_map: HashMap<usize, usize> = lr_terms
                .iter()
                .enumerate()
                .filter(|(_, t)| t.i <= site && t.j > site + 1)
                .enumerate()
                .map(|(pos, (idx, _))| (idx, pos + 1))
                .collect();

            let last_l = chi_l - 1;
            let last_r = chi_r - 1;

            // Helper closure: set W[a, s, sp, b]
            let idx = |a: usize, s: usize, sp: usize, b: usize| -> usize {
                ((a * d + s) * d + sp) * chi_r + b
            };

            // --- Identity: idle -> idle (row 0 -> col 0) ---
            if site > 0 && site < n - 1 {
                for s in 0..d {
                    w[idx(0, s, s, 0)] += 1.0;
                }
            }

            // --- Identity: done -> done (row last -> col last) ---
            if site > 0 && site < n - 1 {
                for s in 0..d {
                    w[idx(last_l, s, s, last_r)] += 1.0;
                }
            }

            // --- Local term: idle -> done (row 0 -> col last) ---
            if let Some(h_local) = ham.local_terms.get(site) {
                let a = 0;
                let b = last_r;
                for s in 0..d {
                    for sp in 0..d {
                        let val = h_local[s * d + sp];
                        if val.abs() > 1e-15 {
                            w[idx(a, s, sp, b)] += val;
                        }
                    }
                }
            }

            // --- LR terms starting here: idle -> thread (row 0 -> col thread_idx) ---
            for (lr_idx, t) in lr_terms.iter().enumerate() {
                if t.i == site {
                    if let Some(&col) = right_thread_map.get(&lr_idx) {
                        for s in 0..d {
                            for sp in 0..d {
                                let val = t.op_left[s * d + sp] * t.strength;
                                if val.abs() > 1e-15 {
                                    w[idx(0, s, sp, col)] += val;
                                }
                            }
                        }
                    }
                }
            }

            // --- LR terms propagating: thread -> thread (identity) ---
            for (lr_idx, _t) in lr_terms.iter().enumerate() {
                if let (Some(&row), Some(&col)) =
                    (left_thread_map.get(&lr_idx), right_thread_map.get(&lr_idx))
                {
                    for s in 0..d {
                        w[idx(row, s, s, col)] += 1.0;
                    }
                }
            }

            // --- LR terms ending here: thread -> done (row thread_idx -> col last) ---
            for (lr_idx, t) in lr_terms.iter().enumerate() {
                if t.j == site {
                    if let Some(&row) = left_thread_map.get(&lr_idx) {
                        for s in 0..d {
                            for sp in 0..d {
                                let val = t.op_right[s * d + sp];
                                if val.abs() > 1e-15 {
                                    w[idx(row, s, sp, last_r)] += val;
                                }
                            }
                        }
                    }
                }
            }

            // --- Boundary: first site (chi_l = 1) ---
            if site == 0 && n > 1 {
                // idle -> idle pass-through (col 0 = idle)
                for s in 0..d {
                    w[idx(0, s, s, 0)] += 1.0;
                }
            }

            // --- Boundary: last site (chi_r = 1) ---
            if site == n - 1 && n > 1 {
                // done -> done pass-through (col 0 = the only column)
                for s in 0..d {
                    w[idx(last_l, s, s, 0)] += 1.0;
                }
            }

            // --- Single-site chain (n == 1): tensor is just the local term ---
            if n == 1 {
                // Already filled by local term above (row 0 -> col 0)
                // Nothing else needed
            }

            tensors.push(w);
        }

        MPO {
            tensors,
            bond_dims,
            phys_dim: d,
            n_sites: n,
        }
    }

    /// Get MPO tensor element W[alpha, sigma, sigma_prime, beta] at site k.
    pub fn get(&self, site: usize, a: usize, s: usize, sp: usize, b: usize) -> f64 {
        let chi_r = if site == self.n_sites - 1 {
            1
        } else {
            self.bond_dims[site]
        };
        let d = self.phys_dim;
        self.tensors[site][((a * d + s) * d + sp) * chi_r + b]
    }

    /// Maximum MPO bond dimension.
    pub fn max_bond_dim(&self) -> usize {
        self.bond_dims.iter().max().copied().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::hamiltonian::{LongRangeTerm, ProteinHamiltonian};

    fn make_trivial_hamiltonian(n: usize, d: usize) -> ProteinHamiltonian {
        let mut local_terms = Vec::new();
        for _ in 0..n {
            let mut h = vec![0.0; d * d];
            for s in 0..d {
                h[s * d + s] = 0.1 * s as f64;
            }
            local_terms.push(h);
        }
        let nn_terms = (0..n.saturating_sub(1))
            .map(|_| vec![0.0; d * d * d * d])
            .collect();
        ProteinHamiltonian {
            n_sites: n,
            d,
            local_terms,
            nn_terms,
            long_range_terms: Vec::new(),
        }
    }

    #[test]
    fn mpo_no_long_range_has_bond_dim_2() {
        let ham = make_trivial_hamiltonian(5, 3);
        let mpo = MPO::from_hamiltonian(&ham, None);
        assert_eq!(mpo.n_sites, 5);
        assert_eq!(mpo.phys_dim, 3);
        for &chi in &mpo.bond_dims {
            assert_eq!(chi, 2);
        }
    }

    #[test]
    fn mpo_with_long_range_increases_bond_dim() {
        let d = 3;
        let mut ham = make_trivial_hamiltonian(6, d);
        let mut op = vec![0.0; d * d];
        for s in 1..d {
            op[s * d + s] = 1.0;
        }
        ham.long_range_terms.push(LongRangeTerm {
            i: 1,
            j: 4,
            op_left: op.clone(),
            op_right: op,
            strength: 0.5,
        });
        let mpo = MPO::from_hamiltonian(&ham, None);
        // Bond between sites 1 and 2, and 2 and 3 should be 3 (2 + 1 active thread)
        assert_eq!(mpo.bond_dims[1], 3); // bond 1-2
        assert_eq!(mpo.bond_dims[2], 3); // bond 2-3
        // Bond 0-1 and 3-4 and 4-5: thread not active
        assert_eq!(mpo.bond_dims[0], 2); // bond 0-1
        assert_eq!(mpo.bond_dims[3], 2); // bond 3-4
    }

    #[test]
    fn mpo_empty_chain() {
        let ham = make_trivial_hamiltonian(0, 3);
        let mpo = MPO::from_hamiltonian(&ham, None);
        assert_eq!(mpo.n_sites, 0);
        assert!(mpo.tensors.is_empty());
    }

    #[test]
    fn mpo_single_site() {
        let ham = make_trivial_hamiltonian(1, 3);
        let mpo = MPO::from_hamiltonian(&ham, None);
        assert_eq!(mpo.n_sites, 1);
        assert!(mpo.bond_dims.is_empty());
        assert_eq!(mpo.tensors.len(), 1);
        // Tensor shape: 1 * 3 * 3 * 1 = 9
        assert_eq!(mpo.tensors[0].len(), 9);
    }

    #[test]
    fn prune_threshold_filters_weak_terms() {
        let d = 3;
        let mut ham = make_trivial_hamiltonian(6, d);
        let mut op = vec![0.0; d * d];
        for s in 1..d {
            op[s * d + s] = 1.0;
        }
        ham.long_range_terms.push(LongRangeTerm {
            i: 1,
            j: 4,
            op_left: op.clone(),
            op_right: op.clone(),
            strength: 0.01, // weak
        });
        ham.long_range_terms.push(LongRangeTerm {
            i: 0,
            j: 5,
            op_left: op.clone(),
            op_right: op,
            strength: 1.0, // strong
        });

        let mpo_all = MPO::from_hamiltonian(&ham, None);
        let mpo_pruned = MPO::from_hamiltonian(&ham, Some(0.1));

        assert!(mpo_all.max_bond_dim() >= mpo_pruned.max_bond_dim());
    }
}
