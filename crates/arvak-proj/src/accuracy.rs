//! Accuracy test: export arvak-proj statevectors for comparison with Aer.

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::mps::{self, Mps};

    struct LCG {
        state: u64,
    }
    impl LCG {
        fn new(seed: u64) -> Self { Self { state: seed } }
        fn next(&mut self) -> f64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.state >> 33) as f64 / (1u64 << 31) as f64 * 2.0 - 1.0
        }
    }

    fn run_circuit(n: usize, max_bond: usize, angles: &[f64]) -> Mps {
        let mut state = Mps::new(n);
        let mut idx = 0;

        // HF init
        let x_gate = mps::rx(std::f64::consts::PI);
        for i in 0..n / 2 {
            state.apply_single(i, x_gate);
        }

        for _layer in 0..3 {
            for i in (0..n - 1).step_by(2) {
                let theta = angles[idx]; idx += 1;
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
                state.apply_single(i + 1, mps::ry(theta));
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
            }
            for i in (1..n - 1).step_by(2) {
                let theta = angles[idx]; idx += 1;
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
                state.apply_single(i + 1, mps::ry(theta));
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
            }
            for i in 0..n {
                let phi = angles[idx]; idx += 1;
                state.apply_single(i, mps::rz(phi));
            }
        }
        state
    }

    /// Export probability distributions for comparison with Aer.
    #[test]
    fn accuracy_vs_aer() {
        println!("\n  ══════════════════════════════════════════════════════");
        println!("  ACCURACY: arvak-proj vs Aer ground truth");
        println!("  ══════════════════════════════════════════════════════\n");

        println!(
            "  {:>4} | {:>6} | {:>10} | {:>10} | {:>10} | {:>10} | {:>8}",
            "N", "chi", "fidelity", "tvd", "kl_div", "max_err", "time"
        );
        println!("  {}", "-".repeat(75));

        for &n in &[14, 18] {
            // Load Aer ground truth
            let aer_path = format!("../../experiments/aer_probs_{}q.npy", n);
            let aer_probs = load_npy_f64(&aer_path);

            if aer_probs.is_empty() {
                println!("  {:>4} | (no ground truth file: {})", n, aer_path);
                continue;
            }

            // Load angles
            let angles_path = format!("../../experiments/angles_{}q.npy", n);
            let angles = load_npy_f64(&angles_path);
            if angles.is_empty() {
                println!("  {:>4} | (no angles file: {})", n, angles_path);
                continue;
            }

            for &chi in &[4, 8, 16, 32, 64, 128] {
                let t0 = Instant::now();
                let mps = run_circuit(n, chi, &angles);
                let sim_time = t0.elapsed();

                let t0 = Instant::now();
                let psi = mps.to_statevector();
                let contract_time = t0.elapsed();

                // Probability distribution (bit-reversed to match Aer convention)
                let probs_raw: Vec<f64> = psi.iter().map(|c| c.norm_sqr()).collect();
                let norm: f64 = probs_raw.iter().sum();
                let mut probs_normed = vec![0.0; probs_raw.len()];
                for (i, &p) in probs_raw.iter().enumerate() {
                    let rev = reverse_bits(i, n);
                    probs_normed[rev] = p / norm;
                }

                // Fidelity: F = (Σ √(p_i · q_i))²
                let bc: f64 = aer_probs
                    .iter()
                    .zip(probs_normed.iter())
                    .map(|(a, b)| (a * b).sqrt())
                    .sum();
                let fidelity = bc * bc;

                // Total Variation Distance: TVD = ½ Σ |p_i - q_i|
                let tvd: f64 = aer_probs
                    .iter()
                    .zip(probs_normed.iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    / 2.0;

                // KL divergence: Σ p_i · ln(p_i / q_i) (clamped)
                let kl: f64 = aer_probs
                    .iter()
                    .zip(probs_normed.iter())
                    .map(|(a, b)| {
                        if *a > 1e-15 && *b > 1e-15 {
                            a * (a / b).ln()
                        } else {
                            0.0
                        }
                    })
                    .sum();

                // Max absolute error in any probability
                let max_err: f64 = aer_probs
                    .iter()
                    .zip(probs_normed.iter())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f64, f64::max);

                let total_ms = (sim_time + contract_time).as_secs_f64() * 1000.0;

                let marker = if fidelity > 0.999 { " ✓" } else if fidelity > 0.99 { " ~" } else { "" };

                println!(
                    "  {:>4} | {:>6} | {:>10.6} | {:>10.6} | {:>10.4e} | {:>10.4e} | {:>6.1}ms{}",
                    n, chi, fidelity, tvd, kl, max_err, total_ms, marker,
                );

                // Debug: dump top MPS probabilities for chi=128
                if chi == 128 {
                    let mut indexed: Vec<(usize, f64)> = probs_normed.iter().copied().enumerate().collect();
                    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    println!("    MPS top 5: {:?}", &indexed[..5]);
                    // Also top Aer
                    let mut aer_idx: Vec<(usize, f64)> = aer_probs.iter().copied().enumerate().collect();
                    aer_idx.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                    println!("    Aer top 5: {:?}", &aer_idx[..5]);
                }

                if fidelity > 0.9999 {
                    break; // converged
                }
            }
            println!();
        }
    }

    fn reverse_bits(val: usize, n_bits: usize) -> usize {
        let mut result = 0;
        let mut v = val;
        for _ in 0..n_bits {
            result = (result << 1) | (v & 1);
            v >>= 1;
        }
        result
    }

    /// Minimal NPY reader for f64 arrays (C-contiguous, little-endian).
    fn load_npy_f64(path: &str) -> Vec<f64> {
        use std::io::Read;
        let mut file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return vec![],
        };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();

        // NPY format: 6-byte magic, 2-byte version, 2-byte header_len, then header, then data
        if buf.len() < 10 || &buf[..6] != b"\x93NUMPY" {
            return vec![];
        }
        let header_len = u16::from_le_bytes([buf[8], buf[9]]) as usize;
        let data_start = 10 + header_len;

        let data = &buf[data_start..];
        let n_floats = data.len() / 8;
        let mut result = Vec::with_capacity(n_floats);
        for i in 0..n_floats {
            let bytes: [u8; 8] = data[i * 8..(i + 1) * 8].try_into().unwrap();
            result.push(f64::from_le_bytes(bytes));
        }
        result
    }
}
