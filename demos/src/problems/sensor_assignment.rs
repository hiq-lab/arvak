//! Tactical sensor network graphs for QAOA demos.
//!
//! Predefined weighted graphs modeling sensor network optimization problems:
//! patrol partitioning, frequency deconfliction, and area coverage.

use super::maxcut::Graph;

/// 6-zone drone patrol partitioning.
///
/// Partition patrol zones into two groups to maximize coverage of
/// contested boundaries. Weights represent boundary priority.
///
/// ```text
/// 0 --3.0-- 1 --2.0-- 2
/// |         |         |
/// 1.5      2.5       1.0
/// |         |         |
/// 3 --2.0-- 4 --3.5-- 5
/// ```
pub fn drone_patrol_6() -> Graph {
    Graph::weighted(
        6,
        vec![
            (0, 1, 3.0),
            (1, 2, 2.0),
            (0, 3, 1.5),
            (1, 4, 2.5),
            (2, 5, 1.0),
            (3, 4, 2.0),
            (4, 5, 3.5),
        ],
    )
}

/// 8-station radar frequency deconfliction.
///
/// Assign radar stations to two frequency bands to minimize mutual
/// interference. Weights represent interference cost between stations.
pub fn radar_deconfliction_8() -> Graph {
    Graph::weighted(
        8,
        vec![
            (0, 1, 4.0),
            (0, 2, 1.5),
            (1, 2, 3.0),
            (1, 3, 2.0),
            (2, 3, 2.5),
            (2, 4, 1.0),
            (3, 4, 3.5),
            (3, 5, 2.0),
            (4, 5, 4.0),
            (4, 6, 1.5),
            (5, 6, 2.5),
            (5, 7, 3.0),
            (6, 7, 2.0),
            (0, 7, 1.0),
        ],
    )
}

/// 10-node surveillance area coverage.
///
/// Partition surveillance nodes into two overlapping coverage zones.
/// Weights represent the overlap cost between sensor footprints.
pub fn surveillance_grid_10() -> Graph {
    Graph::weighted(
        10,
        vec![
            // Grid backbone
            (0, 1, 3.0),
            (1, 2, 2.5),
            (2, 3, 2.0),
            (3, 4, 3.5),
            (5, 6, 2.0),
            (6, 7, 3.0),
            (7, 8, 2.5),
            (8, 9, 1.5),
            // Cross links
            (0, 5, 1.5),
            (1, 6, 2.0),
            (2, 7, 2.5),
            (3, 8, 1.0),
            (4, 9, 3.0),
            // Diagonals
            (0, 6, 1.0),
            (1, 7, 1.5),
            (3, 9, 2.0),
        ],
    )
}

/// Generate a random sensor network graph.
///
/// # Arguments
/// * `n` - Number of sensor nodes
/// * `connectivity` - Edge probability (0.0 to 1.0)
/// * `seed` - Random seed for reproducibility
pub fn random_sensor_network(n: usize, connectivity: f64, seed: u64) -> Graph {
    let mut state = seed;
    let mut rand = || {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        ((state >> 16) & 0x7fff) as f64 / 32768.0
    };

    let mut edges = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if rand() < connectivity {
                // Weights between 0.5 and 5.0
                let weight = 0.5 + rand() * 4.5;
                edges.push((i, j, weight));
            }
        }
    }

    Graph::weighted(n, edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drone_patrol_6() {
        let g = drone_patrol_6();
        assert_eq!(g.n_nodes, 6);
        assert_eq!(g.num_edges(), 7);
    }

    #[test]
    fn test_radar_deconfliction_8() {
        let g = radar_deconfliction_8();
        assert_eq!(g.n_nodes, 8);
        assert_eq!(g.num_edges(), 14);
    }

    #[test]
    fn test_surveillance_grid_10() {
        let g = surveillance_grid_10();
        assert_eq!(g.n_nodes, 10);
        assert_eq!(g.num_edges(), 16);
    }

    #[test]
    fn test_random_sensor_network() {
        let g = random_sensor_network(12, 0.4, 42);
        assert_eq!(g.n_nodes, 12);
        assert!(g.num_edges() > 0);

        // Deterministic: same seed gives same graph
        let g2 = random_sensor_network(12, 0.4, 42);
        assert_eq!(g.num_edges(), g2.num_edges());
    }
}
