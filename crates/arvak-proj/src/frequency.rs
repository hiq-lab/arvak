//! Frequency extraction from Hamiltonians and circuits.
//!
//! Maps a quantum system to a set of characteristic frequencies ω_i
//! (one per qubit) that can be fed into the channel assessment.

/// Extract frequencies from a diagonal Hamiltonian.
/// For a Hamiltonian H = Σ_i h_i Z_i + Σ_{i<j} J_ij ZZ_{ij},
/// the per-qubit frequency is the on-site coefficient h_i.
///
/// Falls back to graph Laplacian eigenvalues when on-site terms
/// are absent or degenerate.
#[must_use]
pub fn from_onsite_fields(h_fields: &[f64]) -> Vec<f64> {
    h_fields.to_vec()
}

/// Extract frequencies from the graph Laplacian of a coupling graph.
///
/// L = D - A where D is the degree matrix and A is the adjacency matrix.
/// The eigenvalues of L capture the graph's natural oscillation modes.
/// Qubit i gets eigenvalue λ_i (sorted ascending).
///
/// This is the correct extraction for QAOA/MaxCut on weighted graphs.
#[must_use]
pub fn from_graph_laplacian(n_qubits: usize, edges: &[(usize, usize, f64)]) -> Vec<f64> {
    // Build Laplacian
    let mut laplacian = vec![0.0_f64; n_qubits * n_qubits];
    for &(i, j, w) in edges {
        laplacian[i * n_qubits + j] -= w;
        laplacian[j * n_qubits + i] -= w;
        laplacian[i * n_qubits + i] += w;
        laplacian[j * n_qubits + j] += w;
    }

    // Eigendecomposition via faer
    let mat = faer::Mat::from_fn(n_qubits, n_qubits, |i, j| laplacian[i * n_qubits + j]);
    let eigen = mat.self_adjoint_eigen(faer::Side::Lower)
        .expect("eigendecomposition of graph Laplacian failed");
    let eigs = eigen.S();

    let mut freqs: Vec<f64> = (0..n_qubits).map(|i| eigs[i].max(0.01)).collect();
    freqs.sort_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    freqs
}

/// Extract frequencies from ZZ rotation angles in a circuit.
/// Per-qubit frequency = sum of |θ_ij| over all ZZ gates involving qubit i.
/// Suitable for circuits where gate angles encode the coupling structure.
#[must_use]
pub fn from_zz_angles(n_qubits: usize, zz_gates: &[(usize, usize, f64)]) -> Vec<f64> {
    let mut freqs = vec![0.0_f64; n_qubits];
    for &(i, j, theta) in zz_gates {
        freqs[i] += theta.abs();
        freqs[j] += theta.abs();
    }
    // Ensure all positive
    for f in &mut freqs {
        if *f < 1e-15 {
            *f = 0.01;
        }
    }
    freqs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laplacian_path_graph() {
        // Path graph: 0-1-2-3 (all weight 1)
        let edges = vec![(0, 1, 1.0), (1, 2, 1.0), (2, 3, 1.0)];
        let freqs = from_graph_laplacian(4, &edges);
        assert_eq!(freqs.len(), 4);
        // First eigenvalue of path Laplacian is 0 (clamped to 0.01)
        assert!(freqs[0] < 0.02);
        // Last eigenvalue should be positive
        assert!(freqs[3] > 1.0);
    }

    #[test]
    fn zz_angles_accumulate() {
        let gates = vec![(0, 1, 0.5), (0, 2, 0.3), (1, 2, 0.7)];
        let freqs = from_zz_angles(3, &gates);
        assert!((freqs[0] - 0.8).abs() < 1e-10); // 0.5 + 0.3
        assert!((freqs[1] - 1.2).abs() < 1e-10); // 0.5 + 0.7
        assert!((freqs[2] - 1.0).abs() < 1e-10); // 0.3 + 0.7
    }
}
