use nalgebra::{DMatrix, SymmetricEigen};

pub struct ANMResult {
    pub eigenvalues: Vec<f64>,
    pub frequencies: Vec<f64>,
    pub eigenvectors: Vec<Vec<f64>>,
    pub n_modes: usize,
    pub n_residues: usize,
}

impl ANMResult {
    pub fn compute(
        chain: &super::pdb::ProteinChain,
        cutoff: f64,
        gamma: f64,
        n_modes: Option<usize>,
    ) -> Self {
        let n = chain.len();
        let dim = 3 * n;

        // Build Hessian matrix (3N x 3N)
        let mut hess = DMatrix::<f64>::zeros(dim, dim);

        for i in 0..n {
            for j in i + 1..n {
                let d = chain.distance(i, j);
                if d > cutoff || d < 1e-10 {
                    continue;
                }

                let ci = &chain.residues[i].coords;
                let cj = &chain.residues[j].coords;
                let d2 = d * d;

                // Off-diagonal 3x3 block: -gamma * (r_ij (x) r_ij) / |r_ij|^2
                for a in 0..3 {
                    for b in 0..3 {
                        let val = -gamma * (ci[a] - cj[a]) * (ci[b] - cj[b]) / d2;
                        hess[(3 * i + a, 3 * j + b)] = val;
                        hess[(3 * j + b, 3 * i + a)] = val;
                        // Diagonal blocks: subtract off-diagonal contributions
                        hess[(3 * i + a, 3 * i + b)] -= val;
                        hess[(3 * j + a, 3 * j + b)] -= val;
                    }
                }
            }
        }

        // Eigendecomposition
        let eigen = SymmetricEigen::new(hess);
        let mut eig_pairs: Vec<(f64, Vec<f64>)> = eigen
            .eigenvalues
            .iter()
            .enumerate()
            .map(|(idx, &val)| {
                let vec: Vec<f64> = eigen.eigenvectors.column(idx).iter().copied().collect();
                (val, vec)
            })
            .collect();

        // Sort by eigenvalue ascending
        eig_pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // Skip first 6 trivial modes (translation + rotation)
        let non_trivial: Vec<_> = eig_pairs
            .into_iter()
            .filter(|(val, _)| *val > 1e-6)
            .collect();

        let keep = n_modes.unwrap_or(non_trivial.len()).min(non_trivial.len());

        let eigenvalues: Vec<f64> = non_trivial[..keep].iter().map(|(v, _)| *v).collect();
        let frequencies: Vec<f64> = eigenvalues.iter().map(|v| v.sqrt()).collect();
        let eigenvectors: Vec<Vec<f64>> =
            non_trivial[..keep].iter().map(|(_, v)| v.clone()).collect();

        ANMResult {
            eigenvalues,
            frequencies,
            eigenvectors,
            n_modes: keep,
            n_residues: n,
        }
    }

    pub fn residue_participation(&self, residue: usize) -> Vec<(usize, f64)> {
        let mut participations: Vec<(usize, f64)> = self
            .eigenvectors
            .iter()
            .enumerate()
            .map(|(mode_idx, evec)| {
                let base = 3 * residue;
                let p = evec[base].powi(2) + evec[base + 1].powi(2) + evec[base + 2].powi(2);
                (mode_idx, p)
            })
            .collect();
        participations.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        participations
    }

    pub fn local_frequencies(&self, residue: usize, n: usize) -> Vec<f64> {
        self.residue_participation(residue)
            .into_iter()
            .take(n)
            .map(|(idx, _)| self.frequencies[idx])
            .collect()
    }

    pub fn level_spacing_ratio(&self) -> f64 {
        if self.eigenvalues.len() < 3 {
            return 0.0;
        }
        let spacings: Vec<f64> = self
            .eigenvalues
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .collect();
        if spacings.len() < 2 {
            return 0.0;
        }
        let ratios: Vec<f64> = spacings
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let (min, max) = if a < b { (a, b) } else { (b, a) };
                if max < 1e-15 { 0.0 } else { min / max }
            })
            .collect();
        ratios.iter().sum::<f64>() / ratios.len() as f64
    }
}
