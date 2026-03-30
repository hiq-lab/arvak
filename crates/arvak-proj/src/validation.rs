//! Fidelity validation against statevector ground truth
//! and sparse radius convergence analysis.

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use std::time::Instant;

    use crate::channel::ChannelMap;
    use crate::mps::{self, Mps};

    /// Fidelity of MPS state against exact statevector.
    fn fidelity(psi_exact: &[num_complex::Complex64], psi_mps: &[num_complex::Complex64]) -> f64 {
        let dot: num_complex::Complex64 = psi_exact
            .iter()
            .zip(psi_mps.iter())
            .map(|(a, b)| a.conj() * b)
            .sum();
        let norm_e: f64 = psi_exact
            .iter()
            .map(num_complex::Complex64::norm_sqr)
            .sum::<f64>()
            .sqrt();
        let norm_m: f64 = psi_mps
            .iter()
            .map(num_complex::Complex64::norm_sqr)
            .sum::<f64>()
            .sqrt();
        let f = dot.norm() / (norm_e * norm_m);
        f * f
    }

    /// Build mixed-frequency QKR circuit gates for MPS.
    /// Returns (rz_layer, rx_layer, zz_angles, freqs).
    fn build_qkr_layers(
        n: usize,
        k_kick: f64,
    ) -> (
        Vec<[[num_complex::Complex64; 2]; 2]>,
        Vec<[[num_complex::Complex64; 2]; 2]>,
        Vec<f64>,
        Vec<f64>,
    ) {
        let half = n / 2;
        let ps = small_primes(n - half);
        let mut freqs: Vec<f64> = (1..=half).map(|i| i as f64).collect();
        freqs.extend(ps.iter().map(|&p| f64::from(p).sqrt()));

        let rz_layer: Vec<_> = (0..n)
            .map(|i| {
                let phi = (1.0_f64 * (4.0_f64).powi(i.min(30) as i32) / 2.0)
                    .rem_euclid(2.0 * std::f64::consts::PI);
                mps::rz(phi)
            })
            .collect();
        let rx_layer: Vec<_> = (0..n).map(|i| mps::rx(k_kick * freqs[i])).collect();
        let zz_angles: Vec<f64> = (0..n - 1)
            .map(|i| {
                (1.0_f64 * (2.0_f64).powi(((2 + i).min(30)) as i32) / 2.0)
                    .rem_euclid(2.0 * std::f64::consts::PI)
            })
            .collect();

        (rz_layer, rx_layer, zz_angles, freqs)
    }

    /// Run MPS simulation with given chi per bond, return state.
    fn run_mps(
        n: usize,
        n_steps: usize,
        chi_per_bond: &[usize],
        rz_layer: &[[[num_complex::Complex64; 2]; 2]],
        rx_layer: &[[[num_complex::Complex64; 2]; 2]],
        zz_angles: &[f64],
    ) -> Mps {
        let mut state = Mps::new(n);
        for _step in 0..n_steps {
            state.apply_single_layer(rz_layer);
            for i in 0..n - 1 {
                let chi = chi_per_bond[i];
                state
                    .apply_two_qubit(i, mps::zz(zz_angles[i]), chi)
                    .unwrap();
            }
            state.apply_single_layer(rx_layer);
        }
        state
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 1: Fidelity curve — χ_bound vs χ_required for F≥0.99
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn fidelity_curve() {
        println!("\n  ══════════════════════════════════════════════");
        println!("  FIDELITY CURVE: adaptive vs uniform vs exact");
        println!("  ══════════════════════════════════════════════\n");

        let k_kick = 1.5;
        let n_steps = 4;

        println!(
            "  {:>4} | {:>5} | {:>10} | {:>10} | {:>10} | {:>10} | {:>8}",
            "N", "chi", "F_uniform", "F_adaptive", "F_diff", "chi_adapt", "SV_time"
        );
        println!("  {}", "-".repeat(80));

        // N≤20: to_statevector is O(2^N × χ), feasible on laptop.
        // N>20 needs the Mac Studio (48GB) for ground truth.
        for &n in &[8, 10, 12, 14, 16, 18, 20] {
            let (rz_layer, rx_layer, zz_angles, freqs) = build_qkr_layers(n, k_kick);

            // Exact statevector via MPS with huge chi (no truncation)
            let t0 = Instant::now();
            let chi_exact = 1 << (n / 2); // 2^(n/2) — enough for exact
            let chi_exact = chi_exact.min(512); // cap for sanity
            let exact_bonds = vec![chi_exact; n - 1];
            let mps_exact = run_mps(n, n_steps, &exact_bonds, &rz_layer, &rx_layer, &zz_angles);
            let psi_exact = mps_exact.to_statevector();
            let sv_time = t0.elapsed();

            // Channel analysis
            let channels = ChannelMap::from_frequencies(&freqs, 1.0);

            for &chi_max in &[4, 8, 16, 32, 64] {
                // Skip if chi_max > exact (no truncation anyway)
                if chi_max >= chi_exact {
                    continue;
                }

                // Uniform chi
                let uniform_bonds = vec![chi_max; n - 1];
                let mps_u = run_mps(n, n_steps, &uniform_bonds, &rz_layer, &rx_layer, &zz_angles);
                let psi_u = mps_u.to_statevector();
                let f_u = fidelity(&psi_exact, &psi_u);

                // Adaptive chi (same budget)
                let adaptive = channels.adaptive_bond_dims(chi_max);
                let mps_a = run_mps(n, n_steps, &adaptive, &rz_layer, &rx_layer, &zz_angles);
                let psi_a = mps_a.to_statevector();
                let f_a = fidelity(&psi_exact, &psi_a);

                let f_diff = f_a - f_u;
                let chi_range = format!(
                    "{}-{}",
                    adaptive.iter().min().unwrap_or(&0),
                    adaptive.iter().max().unwrap_or(&0)
                );

                let marker = if f_diff > 0.001 {
                    " <<"
                } else if f_diff < -0.001 {
                    " !!"
                } else {
                    ""
                };

                println!(
                    "  {:>4} | {:>5} | {:>10.6} | {:>10.6} | {:>+10.6} | {:>10} | {:>7.1}s{}",
                    n,
                    chi_max,
                    f_u,
                    f_a,
                    f_diff,
                    chi_range,
                    sv_time.as_secs_f64(),
                    marker,
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 2: Sparse radius convergence — why r=5?
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn radius_convergence() {
        println!("\n  ══════════════════════════════════════════════");
        println!("  RADIUS CONVERGENCE: sparse vs dense channel map");
        println!("  ══════════════════════════════════════════════\n");

        let k_kick = 1.5;
        let n_steps = 4;

        // Use N=16 where we can compare all radii and have ground truth
        for &n in &[16, 20, 24] {
            let (rz_layer, rx_layer, zz_angles, freqs) = build_qkr_layers(n, k_kick);

            // Ground truth: exact MPS
            let chi_exact = (1 << (n / 2)).min(512);
            let exact_bonds = vec![chi_exact; n - 1];
            let mps_exact = run_mps(n, n_steps, &exact_bonds, &rz_layer, &rx_layer, &zz_angles);
            let psi_exact = mps_exact.to_statevector();

            // Dense (r=∞) channel map as reference
            let channels_dense = ChannelMap::from_frequencies(&freqs, 1.0);
            let adaptive_dense = channels_dense.adaptive_bond_dims(32);
            let mps_dense = run_mps(
                n,
                n_steps,
                &adaptive_dense,
                &rz_layer,
                &rx_layer,
                &zz_angles,
            );
            let psi_dense = mps_dense.to_statevector();
            let f_dense = fidelity(&psi_exact, &psi_dense);

            println!("  N={n}: chi=32, F(dense/r=∞) = {f_dense:.8}");
            println!(
                "  {:>6} | {:>10} | {:>12} | {:>12} | {:>10}",
                "radius", "F_sparse", "F_diff", "max_bond_diff", "time"
            );
            println!("  {}", "-".repeat(65));

            for r in [1, 2, 3, 5, 8, 12] {
                let t0 = Instant::now();
                let channels_sparse = ChannelMap::from_frequencies_sparse(&freqs, 1.0, r);
                let adaptive_sparse = channels_sparse.adaptive_bond_dims(32);
                let mps_s = run_mps(
                    n,
                    n_steps,
                    &adaptive_sparse,
                    &rz_layer,
                    &rx_layer,
                    &zz_angles,
                );
                let psi_s = mps_s.to_statevector();
                let f_s = fidelity(&psi_exact, &psi_s);
                let dt = t0.elapsed();

                // Max difference in recommended chi between sparse and dense
                let max_chi_diff: usize = adaptive_sparse
                    .iter()
                    .zip(adaptive_dense.iter())
                    .map(|(s, d)| (*s as isize - *d as isize).unsigned_abs())
                    .max()
                    .unwrap_or(0);

                let converged = if (f_s - f_dense).abs() < 1e-6 {
                    " ✓"
                } else {
                    ""
                };

                println!(
                    "  {:>6} | {:>10.8} | {:>+12.2e} | {:>12} | {:>9.1}ms{}",
                    r,
                    f_s,
                    f_s - f_dense,
                    max_chi_diff,
                    dt.as_secs_f64() * 1000.0,
                    converged,
                );
            }
            println!();
        }
    }

    fn small_primes(n: usize) -> Vec<u32> {
        let mut out = Vec::new();
        let mut candidate = 2u32;
        while out.len() < n {
            if out.iter().all(|&p| candidate % p != 0) {
                out.push(candidate);
            }
            candidate += 1;
        }
        out
    }
}
