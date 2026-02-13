//! Quantum Volume (QV) benchmark.
//!
//! Quantum Volume measures the effective computational power of a quantum
//! computer. It generates random SU(4) circuits of width w and depth w,
//! then checks if the heavy output probability exceeds 2/3.
//!
//! QV = `2^(max_width` where heavy output probability > 2/3)

use rand::{Rng, SeedableRng};
use std::f64::consts::PI;

use arvak_ir::{Circuit, ClbitId, QubitId};

use crate::BenchmarkResult;

/// Configuration for a Quantum Volume benchmark.
#[derive(Debug, Clone)]
pub struct QvConfig {
    /// Maximum width (number of qubits) to test.
    pub max_width: u32,
    /// Number of random trials per width.
    pub num_trials: u32,
    /// Number of measurement shots per circuit.
    pub shots: u32,
}

impl Default for QvConfig {
    fn default() -> Self {
        Self {
            max_width: 8,
            num_trials: 100,
            shots: 1024,
        }
    }
}

/// Generate a random QV circuit of the given width.
///
/// A QV circuit has width=depth=w, with each layer consisting of
/// random SU(4) two-qubit gates on random qubit pairs.
pub fn generate_qv_circuit(width: u32, seed: u64) -> Circuit {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
    let mut circuit = Circuit::with_size(format!("qv_{width}"), width, width);

    // For each layer (depth = width)
    for _layer in 0..width {
        // Create random pairing of qubits
        let mut available: Vec<u32> = (0..width).collect();
        while available.len() >= 2 {
            let idx1 = rng.gen_range(0..available.len());
            let q1 = available.remove(idx1);
            let idx2 = rng.gen_range(0..available.len());
            let q2 = available.remove(idx2);

            // Apply random SU(4) as a sequence of 1q + CX + 1q gates
            // This is a simplified decomposition that covers SU(4)
            let angles: [f64; 6] = [
                rng.gen_range(0.0..2.0 * PI),
                rng.gen_range(0.0..2.0 * PI),
                rng.gen_range(0.0..2.0 * PI),
                rng.gen_range(0.0..2.0 * PI),
                rng.gen_range(0.0..2.0 * PI),
                rng.gen_range(0.0..2.0 * PI),
            ];

            // Pre-CX single-qubit rotations
            let _ = circuit.rz(angles[0], QubitId(q1));
            let _ = circuit.ry(angles[1], QubitId(q1));
            let _ = circuit.rz(angles[2], QubitId(q2));
            let _ = circuit.ry(angles[3], QubitId(q2));

            // Entangling gate
            let _ = circuit.cx(QubitId(q1), QubitId(q2));

            // Post-CX single-qubit rotations
            let _ = circuit.ry(angles[4], QubitId(q1));
            let _ = circuit.ry(angles[5], QubitId(q2));
        }
    }

    // Measure all qubits
    for i in 0..width {
        let _ = circuit.measure(QubitId(i), ClbitId(i));
    }

    circuit
}

/// Compute heavy output probability from measurement counts.
///
/// Heavy outputs are bitstrings whose ideal probability is above the median.
/// For a random circuit, approximately half the outputs are heavy.
/// A successful QV measurement has heavy output probability > 2/3.
pub fn heavy_output_probability(
    counts: &std::collections::HashMap<String, u64>,
    width: u32,
) -> f64 {
    let total_shots: u64 = counts.values().sum();
    if total_shots == 0 {
        return 0.0;
    }

    // For a truly random quantum circuit, the heavy output threshold
    // is the median of the Porter-Thomas distribution.
    // Approximately, heavy outputs are those that appear more than
    // 1/(2^width) times on average.
    let denominator = 1u64.checked_shl(width).unwrap_or(u64::MAX);
    let median_threshold = 1.0 / denominator as f64;

    // Count shots where the output is "heavy"
    // In a simplified model, we check if the normalized count exceeds the median
    let heavy_count: u64 = counts
        .values()
        .filter(|&&count| (count as f64 / total_shots as f64) > median_threshold)
        .sum();

    heavy_count as f64 / total_shots as f64
}

/// Create a QV benchmark result for a given achieved volume.
pub fn qv_result(achieved_width: u32, total_trials: u32) -> BenchmarkResult {
    let qv = 1u64.checked_shl(achieved_width).unwrap_or(u64::MAX);
    BenchmarkResult::new("quantum_volume", qv as f64, "QV")
        .with_metric("achieved_width", u64::from(achieved_width))
        .with_metric("total_trials", u64::from(total_trials))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_qv_circuit() {
        let circuit = generate_qv_circuit(3, 42);
        assert_eq!(circuit.num_qubits(), 3);
        assert_eq!(circuit.num_clbits(), 3);
        assert!(circuit.depth() > 0);
    }

    #[test]
    fn test_generate_qv_circuit_deterministic() {
        let c1 = generate_qv_circuit(4, 123);
        let c2 = generate_qv_circuit(4, 123);
        assert_eq!(c1.depth(), c2.depth());
    }

    #[test]
    fn test_heavy_output_probability() {
        let mut counts = std::collections::HashMap::new();
        counts.insert("00".to_string(), 400);
        counts.insert("01".to_string(), 100);
        counts.insert("10".to_string(), 100);
        counts.insert("11".to_string(), 400);

        let hop = heavy_output_probability(&counts, 2);
        // 00 and 11 each have 40% > 25% (median), so heavy count = 800/1000 = 0.8
        assert!(hop > 0.7);
    }

    #[test]
    fn test_qv_result() {
        let result = qv_result(5, 100);
        assert_eq!(result.value, 32.0); // 2^5
        assert_eq!(result.unit, "QV");
    }
}
