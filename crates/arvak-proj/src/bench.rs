//! End-to-end benchmark: 50-qubit projector pipeline.

#[cfg(test)]
#[allow(clippy::needless_return, clippy::type_complexity, clippy::manual_clamp)]
mod tests {
    use std::time::Instant;

    use crate::channel::ChannelMap;
    use crate::mps::{self, Mps};
    use crate::partition::{self, BondClass};
    use crate::reassembly;

    /// Scale test: how far can we go?
    #[test]
    fn bench_scaling() {
        println!("\n  ══════════════════════════════════════════════");
        println!("  SCALING TEST: arvak-proj MPS projector");
        println!("  ══════════════════════════════════════════════\n");
        println!(
            "  {:>8} | {:>8} | {:>5} | {:>5} | {:>8} | {:>10} | {:>10}",
            "qubits", "gates", "stable", "volat", "max_chi", "time", "gates/s"
        );
        println!("  {}", "-".repeat(78));

        for &n in &[50, 100, 500, 1_000, 5_000, 10_000, 50_000, 100_000] {
            let result = run_pipeline(n);
            let time_str = if result.time_ms >= 1000.0 {
                format!("{:.1}s", result.time_ms / 1000.0)
            } else {
                format!("{:.1}ms", result.time_ms)
            };
            println!(
                "  {:>8} | {:>8} | {:>5} | {:>5} | {:>8} | {:>10} | {:>10.0}",
                n,
                result.gates,
                result.n_stable,
                result.n_volatile,
                result.max_bond,
                time_str,
                result.gates_per_sec,
            );

            if result.time_ms > 600_000.0 {
                println!("  (stopping — exceeded 10 min)");
                break;
            }
        }
        println!();
    }

    struct PipelineResult {
        gates: u64,
        n_stable: usize,
        n_volatile: usize,
        max_bond: usize,
        time_ms: f64,
        gates_per_sec: f64,
    }

    fn run_pipeline(n: usize) -> PipelineResult {
        use crate::channel::ChannelMap;
        use crate::mps::{self, Mps};
        use crate::partition;

        let k_kick = 1.5_f64;
        let n_steps = 4; // fewer steps to keep feasible at scale

        // Mixed frequencies: half commensurate, half irrational
        let half = n / 2;
        let ps = small_primes(n - half);
        let mut freqs: Vec<f64> = (1..=half).map(|i| i as f64).collect();
        freqs.extend(ps.iter().map(|&p| f64::from(p).sqrt()));

        let t_total = Instant::now();

        let channels = if n > 5_000 {
            ChannelMap::from_frequencies_sparse(&freqs, 1.0, 5)
        } else {
            ChannelMap::from_frequencies(&freqs, 1.0)
        };
        let part = partition::partition_adaptive(&channels, 64, 0.60);

        let n_stable = part
            .bond_classes
            .iter()
            .filter(|&&c| c == partition::BondClass::Stable)
            .count();
        let n_volatile = part
            .bond_classes
            .iter()
            .filter(|&&c| c == partition::BondClass::Volatile)
            .count();

        let mut mps_state = Mps::new(n);
        let mut gates_applied = 0_u64;

        // Pre-compute gate layers for parallel application
        let rz_layer: Vec<_> = (0..n)
            .map(|i| {
                let phi = (1.0_f64 * (4.0_f64).powi(i.min(30) as i32) / 2.0)
                    .rem_euclid(2.0 * std::f64::consts::PI);
                mps::rz(phi)
            })
            .collect();
        let rx_layer: Vec<_> = (0..n).map(|i| mps::rx(k_kick * freqs[i])).collect();

        // Pre-compute ZZ angles
        let zz_angles: Vec<f64> = (0..n - 1)
            .map(|i| {
                (1.0_f64 * (2.0_f64).powi(((2 + i).min(30)) as i32) / 2.0)
                    .rem_euclid(2.0 * std::f64::consts::PI)
            })
            .collect();

        for _step in 0..n_steps {
            // T: all Rz in parallel
            mps_state.apply_single_layer(&rz_layer);
            gates_applied += n as u64;

            // T: ZZ layer — even/odd sweep
            mps_state
                .apply_two_qubit_layer_parallel(
                    mps::zz(0.0), // template (overridden per-bond)
                    &part.recommended_chi,
                    &zz_angles,
                )
                .unwrap();
            gates_applied += (n - 1) as u64;

            // V: all Rx kicks in parallel
            mps_state.apply_single_layer(&rx_layer);
            gates_applied += n as u64;
        }

        let total = t_total.elapsed();
        let time_ms = total.as_secs_f64() * 1000.0;
        let max_bond = mps_state.bond_dims().iter().max().copied().unwrap_or(0);

        PipelineResult {
            gates: gates_applied,
            n_stable,
            n_volatile,
            max_bond,
            time_ms,
            gates_per_sec: gates_applied as f64 / total.as_secs_f64(),
        }
    }

    /// 50-qubit QKR-like circuit: full pipeline timed.
    #[test]
    fn bench_50q_pipeline() {
        let n = 50;
        let k_kick = 1.5_f64;
        let n_steps = 6;

        let t_total = Instant::now();

        // ── 1. Frequency extraction ─────────────────────────────
        let t0 = Instant::now();

        // Mixed frequencies: 25 commensurate (integers) + 25 sqrt-primes
        let ps = small_primes(25);
        let mut freqs: Vec<f64> = (1..=25).map(f64::from).collect();
        freqs.extend(ps.iter().map(|&p| f64::from(p).sqrt()));

        let freq_time = t0.elapsed();
        println!("\n  [1] Frequency extraction: {:?}", freq_time);
        println!(
            "      {} qubits, {} commensurate + {} irrational",
            n, 25, 25
        );

        // ── 2. Channel assessment ───────────────────────────────
        let t0 = Instant::now();

        let channels = ChannelMap::from_frequencies(&freqs, 1.0);
        let pairs = channels.pair_count();

        let channel_time = t0.elapsed();
        println!("  [2] Channel assessment:   {:?}", channel_time);
        println!("      {} pairs evaluated", pairs);

        // ── 3. Partitioning ─────────────────────────────────────
        let t0 = Instant::now();

        let chi_max = 64;
        // Bonds needing ≤ 50% of chi_max are stable (commensurate region)
        let stable_fraction = 0.50;
        let part = partition::partition_adaptive(&channels, chi_max, stable_fraction);

        let partition_time = t0.elapsed();
        println!(
            "  [3] Partitioning:         {:?}  (stable_fraction={:.0}%)",
            partition_time,
            stable_fraction * 100.0
        );
        println!(
            "      {} volatile bonds, {} volatile qubits (of {})",
            part.n_volatile_bonds, part.n_volatile_qubits, n
        );
        println!(
            "      {} stable bonds (analytical, chi=2)",
            n - 1 - part.n_volatile_bonds
        );

        // Show bond classification
        let n_stable = part
            .bond_classes
            .iter()
            .filter(|&&c| c == BondClass::Stable)
            .count();
        let n_volatile = part
            .bond_classes
            .iter()
            .filter(|&&c| c == BondClass::Volatile)
            .count();
        println!(
            "      Classification: {} stable / {} volatile",
            n_stable, n_volatile
        );

        // ── 4. Adaptive bond dimensions ─────────────────────────
        let t0 = Instant::now();

        let adaptive_chi = channels.adaptive_bond_dims(chi_max);
        let mem_uniform: usize = (0..n - 1)
            .map(|_| chi_max * chi_max)
            .collect::<Vec<_>>()
            .iter()
            .sum();
        let mem_adaptive: usize = adaptive_chi
            .iter()
            .map(|&c| c * c)
            .collect::<Vec<_>>()
            .iter()
            .sum();

        let adapt_time = t0.elapsed();
        println!("  [4] Adaptive bond dims:   {:?}", adapt_time);
        println!(
            "      Memory: uniform={} vs adaptive={} (ratio {:.2}x)",
            mem_uniform,
            mem_adaptive,
            mem_uniform as f64 / mem_adaptive as f64
        );
        println!(
            "      Chi range: {}-{}",
            adaptive_chi.iter().min().unwrap_or(&0),
            adaptive_chi.iter().max().unwrap_or(&0)
        );

        // ── 5. MPS simulation (volatile partition only) ─────────
        let t0 = Instant::now();

        let mut mps = Mps::new(n);

        // Build and apply QKR circuit with adaptive chi per bond
        let hbar = 1.0_f64;
        let mut gates_applied = 0_u64;

        for _step in 0..n_steps {
            // T: single-qubit Rz
            for i in 0..n {
                let phi = hbar * (4.0_f64).powi(i.min(30) as i32) / 2.0;
                // Clamp to avoid overflow for high bits
                let phi_clamped = phi.rem_euclid(2.0 * std::f64::consts::PI);
                mps.apply_single(i, mps::rz(phi_clamped));
                gates_applied += 1;
            }

            // T: pairwise ZZ (only adjacent for MPS — long-range gates
            // would require SWAP routing, skipped for benchmark)
            for i in 0..n - 1 {
                let theta_ij = hbar * (2.0_f64).powi(((2 + i).min(30)) as i32) / 2.0;
                let theta_clamped = theta_ij.rem_euclid(2.0 * std::f64::consts::PI);
                let chi_bond = part.recommended_chi[i];
                mps.apply_two_qubit(i, mps::zz(theta_clamped), chi_bond)
                    .unwrap();
                gates_applied += 1;
            }

            // V: kick
            for (i, &freq) in freqs.iter().enumerate().take(n) {
                mps.apply_single(i, mps::rx(k_kick * freq));
                gates_applied += 1;
            }
        }

        let sim_time = t0.elapsed();
        let bonds = mps.bond_dims();
        let max_bond = bonds.iter().max().copied().unwrap_or(0);
        let mem_actual = mps.memory();

        println!("  [5] MPS simulation:       {:?}", sim_time);
        println!("      {} gates applied in {} steps", gates_applied, n_steps);
        println!(
            "      Max bond dim: {}, memory: {} (vs uniform {})",
            max_bond, mem_actual, mem_uniform
        );
        println!(
            "      Bonds: min={} max={} avg={:.1}",
            bonds.iter().min().unwrap_or(&0),
            bonds.iter().max().unwrap_or(&0),
            bonds.iter().sum::<usize>() as f64 / bonds.len() as f64
        );

        // ── 5b. Sweep stable_fraction ──────────────────────────
        println!("\n  [5b] Stable fraction sweep:");
        println!("      frac  | stable | volatile | volatile_q | time");
        for pct in [20, 30, 40, 50, 60, 70, 80] {
            let frac = f64::from(pct) / 100.0;
            let p = partition::partition_adaptive(&channels, chi_max, frac);
            let n_s = p
                .bond_classes
                .iter()
                .filter(|&&c| c == BondClass::Stable)
                .count();
            let n_v = p
                .bond_classes
                .iter()
                .filter(|&&c| c == BondClass::Volatile)
                .count();

            let t0b = Instant::now();
            let mut mps_b = Mps::new(n);
            for _step in 0..n_steps {
                for i in 0..n {
                    let phi = (1.0_f64 * (4.0_f64).powi(i.min(30) as i32) / 2.0)
                        .rem_euclid(2.0 * std::f64::consts::PI);
                    mps_b.apply_single(i, mps::rz(phi));
                }
                for i in 0..n - 1 {
                    let theta = (1.0_f64 * (2.0_f64).powi(((2 + i).min(30)) as i32) / 2.0)
                        .rem_euclid(2.0 * std::f64::consts::PI);
                    mps_b
                        .apply_two_qubit(i, mps::zz(theta), p.recommended_chi[i])
                        .unwrap();
                }
                for (i, &freq) in freqs.iter().enumerate().take(n) {
                    mps_b.apply_single(i, mps::rx(k_kick * freq));
                }
            }
            let dt = t0b.elapsed();
            println!(
                "      {:3}%  |   {:2}   |    {:2}    |     {:2}     | {:.1}ms",
                pct,
                n_s,
                n_v,
                p.n_volatile_qubits,
                dt.as_secs_f64() * 1000.0
            );
        }

        // ── 6. Reassembly & projection ──────────────────────────
        let t0 = Instant::now();

        let result = reassembly::estimate_fidelity(n, part.n_volatile_qubits, 0.01);
        let proj_100 = reassembly::scale_projection(&result, 100);
        let proj_1000 = reassembly::scale_projection(&result, 1000);
        let proj_10000 = reassembly::scale_projection(&result, 10_000);

        let reasm_time = t0.elapsed();
        println!("  [6] Reassembly:           {:?}", reasm_time);
        println!("      50q  → F_est = {:.6}", result.estimated_fidelity);
        println!("      100q → F_est = {:.6}", proj_100.estimated_fidelity);
        println!("      1Kq  → F_est = {:.6}", proj_1000.estimated_fidelity);
        println!("      10Kq → F_est = {:.6}", proj_10000.estimated_fidelity);

        // ── Total ───────────────────────────────────────────────
        let total = t_total.elapsed();
        println!("\n  TOTAL: {:?}", total);
        println!("  ──────────────────────────────────────────");
        println!(
            "  {} qubits, {} gates, {:.1} ms",
            n,
            gates_applied,
            total.as_secs_f64() * 1000.0
        );
        println!(
            "  {:.0} gates/sec",
            gates_applied as f64 / total.as_secs_f64()
        );

        // Sanity checks
        assert_eq!(mps.n_qubits, 50);
        assert!(total.as_secs() < 60, "50q should complete in < 60s");
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

    // ─────────────────────────────────────────────────────────────────────
    // QKR helper used by the Jacobian validation test below
    // ─────────────────────────────────────────────────────────────────────

    /// Apply a QKR layer sequence to the given MPS using the supplied
    /// per-bond χ profile and per-site frequencies.
    fn run_qkr(
        mps: &mut Mps,
        n: usize,
        n_steps: usize,
        chi_per_bond: &[usize],
        freqs: &[f64],
    ) -> std::time::Duration {
        let k_kick = 1.5_f64;
        let two_pi = 2.0 * std::f64::consts::PI;

        let rz_layer: Vec<_> = (0..n)
            .map(|i| {
                let phi = (4.0_f64).powi(i.min(30) as i32) / 2.0;
                mps::rz(phi.rem_euclid(two_pi))
            })
            .collect();
        let rx_layer: Vec<_> = (0..n).map(|i| mps::rx(k_kick * freqs[i])).collect();
        let zz_angles: Vec<f64> = (0..n - 1)
            .map(|i| {
                let theta = (2.0_f64).powi(((2 + i).min(30)) as i32) / 2.0;
                theta.rem_euclid(two_pi)
            })
            .collect();

        let t0 = Instant::now();
        for _step in 0..n_steps {
            mps.apply_single_layer(&rz_layer);
            mps.apply_two_qubit_layer_parallel(mps::zz(0.0), chi_per_bond, &zz_angles)
                .unwrap();
            mps.apply_single_layer(&rx_layer);
        }
        t0.elapsed()
    }

    // ─────────────────────────────────────────────────────────────────────
    // Statistics helpers
    // ─────────────────────────────────────────────────────────────────────

    /// Pearson correlation coefficient between two equal-length vectors.
    /// Returns NaN if either vector is constant.
    fn pearson(x: &[f64], y: &[f64]) -> f64 {
        let n = x.len().min(y.len()) as f64;
        if n < 2.0 {
            return f64::NAN;
        }
        let mx = x.iter().take(x.len().min(y.len())).sum::<f64>() / n;
        let my = y.iter().take(x.len().min(y.len())).sum::<f64>() / n;
        let mut cov = 0.0_f64;
        let mut vx = 0.0_f64;
        let mut vy = 0.0_f64;
        for (xi, yi) in x.iter().zip(y.iter()) {
            let dx = xi - mx;
            let dy = yi - my;
            cov += dx * dy;
            vx += dx * dx;
            vy += dy * dy;
        }
        if vx < 1e-30 || vy < 1e-30 {
            return f64::NAN;
        }
        cov / (vx.sqrt() * vy.sqrt())
    }

    /// Spearman rank correlation between two equal-length vectors.
    #[allow(dead_code)]
    fn spearman(x: &[f64], y: &[f64]) -> f64 {
        let rx = rank(x);
        let ry = rank(y);
        pearson(&rx, &ry)
    }

    /// Convert values to fractional ranks (average rank for ties).
    fn rank(v: &[f64]) -> Vec<f64> {
        let n = v.len();
        let mut indexed: Vec<(usize, f64)> = v.iter().enumerate().map(|(i, &x)| (i, x)).collect();
        indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut ranks = vec![0.0_f64; n];
        let mut i = 0;
        while i < n {
            let mut j = i + 1;
            while j < n && indexed[j].1 == indexed[i].1 {
                j += 1;
            }
            // Average rank for ties (1-based)
            let avg = ((i + 1) as f64 + j as f64) / 2.0;
            for k in i..j {
                ranks[indexed[k].0] = avg;
            }
            i = j;
        }
        ranks
    }

    // ─────────────────────────────────────────────────────────────────────
    // Phase 5i: validate the public finite-difference Jacobian API
    // ─────────────────────────────────────────────────────────────────────

    /// Inline reference implementation of the central-difference Jacobian
    /// of the per-bond cumulative discarded weight w.r.t. the per-site
    /// frequencies. Used to cross-check `InputJacobian::compute`.
    fn reference_jacobian_discarded(
        n: usize,
        chi_max: usize,
        n_steps: usize,
        delta: f64,
    ) -> Vec<Vec<f64>> {
        let half = n / 2;
        let ps = small_primes(n - half);
        let mut base_freqs: Vec<f64> = (1..=half).map(|i| i as f64).collect();
        base_freqs.extend(ps.iter().map(|&p| f64::from(p).sqrt()));

        let mut j = vec![vec![0.0_f64; n]; n.saturating_sub(1)];

        for i in 0..n {
            let mut freqs_plus = base_freqs.clone();
            freqs_plus[i] += delta;
            let mut mps_plus = Mps::new(n);
            let chi_uniform = vec![chi_max; n.saturating_sub(1)];
            let _ = run_qkr(&mut mps_plus, n, n_steps, &chi_uniform, &freqs_plus);
            let d_plus = mps_plus.discarded_weight_per_bond.clone();

            let mut freqs_minus = base_freqs.clone();
            freqs_minus[i] -= delta;
            let mut mps_minus = Mps::new(n);
            let _ = run_qkr(&mut mps_minus, n, n_steps, &chi_uniform, &freqs_minus);
            let d_minus = mps_minus.discarded_weight_per_bond.clone();

            for k in 0..n.saturating_sub(1) {
                j[k][i] = (d_plus[k] - d_minus[k]) / (2.0 * delta);
            }
        }

        j
    }

    /// The public Jacobian API must agree with the inline reference
    /// implementation, and the participation-ratio profile derived from it
    /// must produce a non-trivial adaptive χ allocation.
    #[test]
    fn validate_jacobian_module() {
        use crate::finite_difference_jacobian::{
            InputJacobian, JacobianAllocation, JacobianConfig, chi_allocation_from_jacobian,
            participation_ratio_profile,
        };

        let n = 50;
        let chi_max = 8;
        let n_steps = 8;
        let delta = 0.01_f64;

        let half = n / 2;
        let ps = small_primes(n - half);
        let mut base_freqs: Vec<f64> = (1..=half).map(|i| i as f64).collect();
        base_freqs.extend(ps.iter().map(|&p| f64::from(p).sqrt()));

        let factory = |freqs: &[f64]| -> Mps {
            let mut mps = Mps::new(n);
            let chi_uniform = vec![chi_max; n.saturating_sub(1)];
            let _ = run_qkr(&mut mps, n, n_steps, &chi_uniform, freqs);
            mps
        };

        let observe = |mps: &Mps| -> Vec<f64> { mps.discarded_weight_per_bond.clone() };

        let cfg = JacobianConfig { delta, stride: 1 };

        let j = InputJacobian::compute(&base_freqs, factory, observe, &cfg);

        assert_eq!(j.n_inputs, n);
        assert_eq!(j.n_outputs, n - 1);
        assert_eq!(j.matrix.len(), n - 1);
        assert_eq!(j.matrix[0].len(), n);

        let j_ref = reference_jacobian_discarded(n, chi_max, n_steps, delta);
        for k in 0..n - 1 {
            for i in 0..n {
                let api_val = j.matrix[k][i];
                let ref_val = j_ref[k][i];
                if api_val.is_finite() && ref_val.is_finite() {
                    let diff = (api_val - ref_val).abs();
                    let scale = api_val.abs().max(ref_val.abs()).max(1e-9);
                    assert!(
                        diff / scale < 1e-6,
                        "k={k} i={i}: api={api_val} ref={ref_val} (rel diff {})",
                        diff / scale
                    );
                }
            }
        }

        // Sanity: pearson/rank helpers still callable
        let ones = vec![1.0_f64; n - 1];
        let twos = vec![2.0_f64; n - 1];
        let _ = pearson(&ones, &twos);
        let _ = rank(&ones);

        let pr = participation_ratio_profile(&j);
        assert_eq!(pr.len(), n - 1);
        for &p in &pr {
            assert!(p >= 0.0 && p.is_finite());
            assert!(p <= n as f64 + 1.0);
        }

        let chi = chi_allocation_from_jacobian(&j, 4, 32, JacobianAllocation::ParticipationRatio);
        assert_eq!(chi.len(), n - 1);
        let unique: std::collections::HashSet<usize> = chi.iter().copied().collect();
        assert!(
            unique.len() >= 2,
            "chi allocation should produce at least 2 distinct values, got {chi:?}"
        );

        println!(
            "  Jacobian module API works: J shape [{} × {}], PR ∈ [{:.2}, {:.2}], χ ∈ [{}, {}]",
            j.n_outputs,
            j.n_inputs,
            pr.iter().cloned().fold(f64::INFINITY, f64::min),
            pr.iter().cloned().fold(0.0_f64, f64::max),
            chi.iter().min().unwrap(),
            chi.iter().max().unwrap()
        );
    }
}
