//! Max-Cut problem definition for QAOA.
//!
//! The Max-Cut problem: Given a graph G = (V, E), partition vertices into
//! two sets S and T to maximize the number of edges between S and T.
//!
//! This is an NP-hard combinatorial optimization problem with applications
//! in circuit layout, statistical physics, and network design.

use serde::{Deserialize, Serialize};

/// A graph for the Max-Cut problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    /// Number of nodes.
    pub n_nodes: usize,
    /// Edges as (`node_a`, `node_b`, weight).
    pub edges: Vec<(usize, usize, f64)>,
}

impl Graph {
    /// Create a new graph.
    pub fn new(n_nodes: usize, edges: Vec<(usize, usize)>) -> Self {
        Self {
            n_nodes,
            edges: edges.into_iter().map(|(a, b)| (a, b, 1.0)).collect(),
        }
    }

    /// Create a new weighted graph.
    pub fn weighted(n_nodes: usize, edges: Vec<(usize, usize, f64)>) -> Self {
        Self { n_nodes, edges }
    }

    /// Create a 4-node square graph (simple demo case).
    ///
    /// ```text
    /// 0 --- 1
    /// |     |
    /// 3 --- 2
    /// ```
    pub fn square_4() -> Self {
        Self::new(4, vec![(0, 1), (1, 2), (2, 3), (3, 0)])
    }

    /// Create a 4-node complete graph K4.
    pub fn complete_4() -> Self {
        Self::new(4, vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)])
    }

    /// Create a 6-node ring graph.
    pub fn ring_6() -> Self {
        Self::new(6, vec![(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0)])
    }

    /// Create a 6-node graph with cross edges.
    ///
    /// ```text
    /// 0 --- 1 --- 2
    /// |  X  |  X  |
    /// 3 --- 4 --- 5
    /// ```
    pub fn grid_6() -> Self {
        Self::new(
            6,
            vec![
                (0, 1),
                (1, 2),
                (3, 4),
                (4, 5),
                (0, 3),
                (1, 4),
                (2, 5),
                (0, 4),
                (1, 3),
                (1, 5),
                (2, 4),
            ],
        )
    }

    /// Create a random graph with given edge probability.
    pub fn random(n_nodes: usize, edge_probability: f64, seed: u64) -> Self {
        use std::collections::HashSet;

        // Simple LCG random number generator for reproducibility
        let mut state = seed;
        let mut rand = || {
            state = state.wrapping_mul(1103515245).wrapping_add(12345);
            ((state >> 16) & 0x7fff) as f64 / 32768.0
        };

        let mut edges = HashSet::new();
        for i in 0..n_nodes {
            for j in (i + 1)..n_nodes {
                if rand() < edge_probability {
                    edges.insert((i, j));
                }
            }
        }

        Self::new(n_nodes, edges.into_iter().collect())
    }

    /// Get the number of edges.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Calculate the cut value for a given bitstring assignment.
    ///
    /// `assignment[i] = true` means node i is in set S.
    pub fn cut_value(&self, assignment: &[bool]) -> f64 {
        self.edges
            .iter()
            .filter(|(a, b, _)| assignment[*a] != assignment[*b])
            .map(|(_, _, w)| w)
            .sum()
    }

    /// Calculate the cut value from a bitstring (integer).
    pub fn cut_value_from_bitstring(&self, bitstring: usize) -> f64 {
        let assignment: Vec<bool> = (0..self.n_nodes)
            .map(|i| (bitstring >> i) & 1 == 1)
            .collect();
        self.cut_value(&assignment)
    }

    /// Find the maximum cut value by brute force (for small graphs).
    pub fn max_cut_brute_force(&self) -> (usize, f64) {
        assert!(self.n_nodes <= 20, "Brute force limited to 20 nodes");
        let mut best_bitstring = 0;
        let mut best_value = 0.0;

        for bitstring in 0..(1 << self.n_nodes) {
            let value = self.cut_value_from_bitstring(bitstring);
            if value > best_value {
                best_value = value;
                best_bitstring = bitstring;
            }
        }

        (best_bitstring, best_value)
    }

    /// Convert bitstring to human-readable partition.
    pub fn bitstring_to_partition(&self, bitstring: usize) -> (Vec<usize>, Vec<usize>) {
        let mut set_s = vec![];
        let mut set_t = vec![];

        for i in 0..self.n_nodes {
            if (bitstring >> i) & 1 == 1 {
                set_s.push(i);
            } else {
                set_t.push(i);
            }
        }

        (set_s, set_t)
    }

    /// Get the Ising Hamiltonian for this Max-Cut problem.
    ///
    /// Max-Cut maps to finding the ground state of:
    /// H = -1/2 Σ_{(i,j) ∈ E} w_{ij} (1 - `Z_i` `Z_j`)
    ///   = const - 1/2 Σ_{(i,j) ∈ E} w_{ij} `Z_i` `Z_j`
    ///
    /// The ground state corresponds to the maximum cut.
    pub fn to_ising_coefficients(&self) -> (f64, Vec<(usize, usize, f64)>) {
        let offset: f64 = self.edges.iter().map(|(_, _, w)| w / 2.0).sum();
        let zz_terms: Vec<_> = self
            .edges
            .iter()
            .map(|(i, j, w)| (*i, *j, w / 2.0))
            .collect();
        (offset, zz_terms)
    }
}

impl std::fmt::Display for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Graph ({} nodes, {} edges):",
            self.n_nodes,
            self.edges.len()
        )?;
        for (a, b, w) in &self.edges {
            if (*w - 1.0).abs() < 1e-10 {
                writeln!(f, "  {a} -- {b}")?;
            } else {
                writeln!(f, "  {a} -- {b} (weight: {w:.2})")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_graph() {
        let g = Graph::square_4();
        assert_eq!(g.n_nodes, 4);
        assert_eq!(g.num_edges(), 4);
    }

    #[test]
    fn test_cut_value() {
        let g = Graph::square_4();

        // All in same set: cut = 0
        assert_eq!(g.cut_value(&[true, true, true, true]), 0.0);

        // Alternating: cut = 4 (all edges cut)
        assert_eq!(g.cut_value(&[true, false, true, false]), 4.0);

        // Half-half: cut = 2
        assert_eq!(g.cut_value(&[true, true, false, false]), 2.0);
    }

    #[test]
    fn test_max_cut_brute_force() {
        let g = Graph::square_4();
        let (best, value) = g.max_cut_brute_force();

        // For square, max cut is 4 (alternating pattern)
        assert_eq!(value, 4.0);
        // Best should be 0101 (5) or 1010 (10)
        assert!(best == 5 || best == 10);
    }

    #[test]
    fn test_partition() {
        let g = Graph::square_4();
        let (s, t) = g.bitstring_to_partition(5); // 0101
        assert_eq!(s, vec![0, 2]);
        assert_eq!(t, vec![1, 3]);
    }

    #[test]
    fn test_ising_coefficients() {
        let g = Graph::square_4();
        let (offset, terms) = g.to_ising_coefficients();
        assert_eq!(offset, 2.0); // 4 edges * 0.5
        assert_eq!(terms.len(), 4);
    }
}
