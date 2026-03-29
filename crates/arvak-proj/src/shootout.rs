//! Shootout: arvak-proj vs the clock.
//! Outputs timing data that Python compares against Aer/DDSIM.

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use std::time::Instant;

    use crate::mps::{self, Mps};

    /// Build the same VQE-like circuit as the Python shootout:
    /// HF init + 3 layers of (CNOT+RY+CNOT on even pairs, then odd pairs, then RZ all)
    fn run_vqe_circuit(n: usize, max_bond: usize) -> (f64, Vec<usize>) {
        let mut state = Mps::new(n);
        let seed: u64 = 42;
        let mut rng_state = seed;

        // Simple LCG for reproducible "random" angles matching Python's RandomState(42)
        let mut next_f64 = || -> f64 {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            // Map to [-1, 1] for RY or [-pi, pi] for RZ
            (rng_state >> 33) as f64 / (1u64 << 31) as f64 * 2.0 - 1.0
        };

        // HF init: X on first n/2 qubits
        let x_gate = mps::rx(std::f64::consts::PI);
        for i in 0..n / 2 {
            state.apply_single(i, x_gate);
        }

        let t0 = Instant::now();

        for _layer in 0..3 {
            // Even pairs: CNOT + RY + CNOT
            for i in (0..n - 1).step_by(2) {
                let theta = next_f64();
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
                state.apply_single(i + 1, mps::ry(theta));
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
            }
            // Odd pairs
            for i in (1..n - 1).step_by(2) {
                let theta = next_f64();
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
                state.apply_single(i + 1, mps::ry(theta));
                state.apply_two_qubit(i, mps::cx(), max_bond).unwrap();
            }
            // RZ all
            for i in 0..n {
                let phi = next_f64() * std::f64::consts::PI;
                state.apply_single(i, mps::rz(phi));
            }
        }

        let elapsed = t0.elapsed();
        let bonds = state.bond_dims();
        (elapsed.as_secs_f64(), bonds)
    }

    #[test]
    fn shootout_arvak_proj() {
        println!("\n  ══════════════════════════════════════════════════════");
        println!("  arvak-proj SHOOTOUT: VQE-like circuit, 3 layers");
        println!("  ══════════════════════════════════════════════════════\n");

        println!(
            "  {:>6} | {:>10} | {:>10} | {:>8} | {:>8}",
            "qubits", "chi=32", "chi=64", "max_bond", "gates"
        );
        println!("  {}", "-".repeat(55));

        for &n in &[14, 18, 22, 24, 26, 28, 30, 50, 100, 200, 500, 1000] {
            let n_gates = n / 2 // HF init
                + 3 * ((n - 1) / 2 * 3 + (n - 2) / 2 * 3 + n); // 3 layers

            let (dt32, bonds32) = run_vqe_circuit(n, 32);
            let max_b32 = bonds32.iter().max().copied().unwrap_or(0);

            let (dt64, bonds64) = run_vqe_circuit(n, 64);
            let max_b64 = bonds64.iter().max().copied().unwrap_or(0);

            let fmt_t = |t: f64| -> String {
                if t < 0.001 {
                    format!("{:.0}µs", t * 1e6)
                } else if t < 1.0 {
                    format!("{:.1}ms", t * 1e3)
                } else {
                    format!("{:.2}s", t)
                }
            };

            println!(
                "  {:>6} | {:>10} | {:>10} | {:>8} | {:>8}",
                n,
                fmt_t(dt32),
                fmt_t(dt64),
                max_b32,
                n_gates,
            );

            if dt64 > 60.0 {
                println!("  (stopping)");
                break;
            }
        }
    }
}
