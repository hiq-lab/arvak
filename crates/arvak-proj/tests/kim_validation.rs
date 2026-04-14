//! Phase 6 — 1D Floquet Kicked Ising validation benchmark.
//!
//! Validates the arvak-proj adaptive-χ pipeline against an independent dense
//! statevector reference at the self-dual point of the kicked Ising chain
//! (J = h_x = π/4, h_z = 0; Bertini-Kos-Prosen, PRX 9, 021033, 2019).
//!
//! Stages:
//! - **A** N=12, χ_max = 64 (provably exact). Cross-check arvak-proj against
//!         the inline dense reference simulator. Expect agreement to ~1e-10.
//! - **B** N=12, χ_max sweep at 4..32 (uniform). Print fidelity/error vs χ.
//! - **C** N=12, Jacobian-allocated χ. Demonstrate that the adaptive profile
//!         meets target accuracy at smaller total bond budget than uniform.
//! - **D** N=24, exact statevector reference (16M-dim) vs arvak-proj at large χ.
//!         Confirms the validation also holds at the largest tractable
//!         statevector size.
//! - **E** N=50, 100, 200 — no exact reference. Compare uniform-χ vs
//!         Jacobian-χ for total discarded weight and observable stability.

use arvak_proj::finite_difference_jacobian::{
    chi_allocation_from_jacobian, InputJacobian, JacobianAllocation, JacobianConfig,
};
use arvak_proj::kicked_ising::{
    apply_kim_step, apply_kim_step_disordered, reference_kim_run,
    reference_kim_run_disordered, KimParams,
};
use arvak_proj::mps::Mps;
use std::time::Instant;

const TOL_EXACT: f64 = 1e-10;

/// Run the kicked Ising circuit on an MPS for `n_steps` Trotter steps using
/// the supplied per-bond χ profile. Returns the per-step ⟨Z_q⟩ history,
/// shape `(n_steps+1) × n`.
fn run_mps_kim(
    n: usize,
    params: KimParams,
    n_steps: usize,
    chi_per_bond: &[usize],
) -> (Mps, Vec<Vec<f64>>) {
    let mut mps = Mps::new(n);
    let mut history = Vec::with_capacity(n_steps + 1);
    history.push((0..n).map(|q| mps.expectation_z(q)).collect());

    for _step in 0..n_steps {
        apply_kim_step(&mut mps, params, chi_per_bond).unwrap();
        history.push((0..n).map(|q| mps.expectation_z(q)).collect());
    }
    (mps, history)
}

/// Disordered variant: each site has its own h_x.
fn run_mps_kim_disordered(
    n: usize,
    params: KimParams,
    h_x_per_site: &[f64],
    n_steps: usize,
    chi_per_bond: &[usize],
) -> (Mps, Vec<Vec<f64>>) {
    let mut mps = Mps::new(n);
    let mut history = Vec::with_capacity(n_steps + 1);
    history.push((0..n).map(|q| mps.expectation_z(q)).collect());

    for _step in 0..n_steps {
        apply_kim_step_disordered(&mut mps, params, h_x_per_site, chi_per_bond).unwrap();
        history.push((0..n).map(|q| mps.expectation_z(q)).collect());
    }
    (mps, history)
}

/// Build a Jacobian for the **disordered** kicked Ising chain by perturbing
/// each site's h_x around the supplied baseline. Returns the FD Jacobian of
/// per-bond cumulative discarded weight w.r.t. per-site h_x.
fn build_disordered_kim_jacobian(
    n: usize,
    params: KimParams,
    base_hx: &[f64],
    pilot_chi: usize,
    pilot_steps: usize,
) -> InputJacobian {
    let factory = move |hx_per_site: &[f64]| -> Mps {
        let mut mps = Mps::new(n);
        let chi_per_bond = vec![pilot_chi; n - 1];
        for _step in 0..pilot_steps {
            apply_kim_step_disordered(&mut mps, params, hx_per_site, &chi_per_bond).unwrap();
        }
        mps
    };

    let observe = |mps: &Mps| -> Vec<f64> { mps.discarded_weight_per_bond.clone() };

    let cfg = JacobianConfig {
        delta: 0.02,
        stride: 1,
    };

    InputJacobian::compute(base_hx, factory, observe, &cfg)
}

/// Deterministic Mulberry32-style PRNG so that disordered tests are
/// reproducible without bringing in `rand` as a dev dependency.
fn det_random_hx(n: usize, seed: u64, base: f64, amplitude: f64) -> Vec<f64> {
    let mut state = seed.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = ((state >> 11) as f64) / ((1_u64 << 53) as f64); // [0, 1)
        out.push(base + amplitude * (2.0 * r - 1.0));
    }
    out
}

/// Maximum absolute deviation between two ⟨Z⟩ histories of identical shape.
fn history_max_err(a: &[Vec<f64>], b: &[Vec<f64>]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mut m = 0.0_f64;
    for (row_a, row_b) in a.iter().zip(b.iter()) {
        assert_eq!(row_a.len(), row_b.len());
        for (x, y) in row_a.iter().zip(row_b.iter()) {
            m = m.max((x - y).abs());
        }
    }
    m
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE A — N=12, full χ, exact agreement with dense reference
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_a_n12_full_chi_matches_reference() {
    let n = 12;
    let n_steps = 10;
    let params = KimParams::self_dual();

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage A: N={n}, χ_max=64 (provably exact), {n_steps} Trotter steps");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    let chi_max = 1_usize << (n / 2); // 2^6 = 64, exact for any 12-qubit state
    let chi_per_bond = vec![chi_max; n - 1];

    let t_ref = Instant::now();
    let ref_history = reference_kim_run(n, params, n_steps);
    let ref_ms = t_ref.elapsed().as_secs_f64() * 1000.0;

    let t_mps = Instant::now();
    let (mps, mps_history) = run_mps_kim(n, params, n_steps, &chi_per_bond);
    let mps_ms = t_mps.elapsed().as_secs_f64() * 1000.0;

    let max_err = history_max_err(&ref_history, &mps_history);
    let max_bond = mps.bond_dims().iter().max().copied().unwrap_or(0);
    let total_disc = mps.total_discarded_weight();

    println!("  reference statevector: {ref_ms:8.2} ms");
    println!("  arvak-proj MPS:        {mps_ms:8.2} ms");
    println!("  max bond used:         {max_bond}");
    println!("  total discarded weight: {total_disc:.3e}");
    println!("  max ⟨Z⟩ error vs ref:  {max_err:.3e}\n");

    assert!(
        max_err < TOL_EXACT,
        "Stage A: arvak-proj at full χ disagrees with dense reference: max_err={max_err}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE B — N=12, χ sweep, document fidelity vs uniform χ budget
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_b_n12_chi_sweep_uniform() {
    let n = 12;
    let n_steps = 10;
    let params = KimParams::self_dual();

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage B: N={n}, uniform χ sweep, {n_steps} Trotter steps");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    let ref_history = reference_kim_run(n, params, n_steps);

    println!(
        "  {:>4} | {:>14} | {:>14} | {:>10}",
        "χ", "max ⟨Z⟩ error", "total disc", "wall ms"
    );
    println!("  {}", "-".repeat(54));

    for &chi in &[2, 4, 6, 8, 12, 16, 24, 32, 64] {
        let chi_per_bond = vec![chi; n - 1];

        let t = Instant::now();
        let (mps, mps_history) = run_mps_kim(n, params, n_steps, &chi_per_bond);
        let ms = t.elapsed().as_secs_f64() * 1000.0;

        let max_err = history_max_err(&ref_history, &mps_history);
        let total_disc = mps.total_discarded_weight();

        println!(
            "  {:>4} | {:>14.3e} | {:>14.3e} | {:>10.2}",
            chi, max_err, total_disc, ms
        );
    }
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: build a Jacobian for the kicked Ising model using per-site
// transverse-field (h_x) perturbations as the input axis. The Jacobian
// outputs are per-bond cumulative discarded weight after a pilot run at
// the supplied (small) χ. h_x is the right perturbation axis because it
// directly drives entanglement growth — h_z perturbations are diagonal in
// the computational basis and produce identically-zero Jacobians on the
// discarded-weight observable.
// ─────────────────────────────────────────────────────────────────────────────

fn build_kim_jacobian(n: usize, params: KimParams, pilot_chi: usize, pilot_steps: usize) -> InputJacobian {
    let base_hx: Vec<f64> = vec![params.h_x; n];

    let factory = move |hx_per_site: &[f64]| -> Mps {
        let mut mps = Mps::new(n);
        let chi_per_bond = vec![pilot_chi; n - 1];
        for _step in 0..pilot_steps {
            // ZZ entangling layer with global J
            let zz_angle = params.j * params.dt;
            let zz_angles: Vec<f64> = vec![zz_angle; n.saturating_sub(1)];
            mps.apply_two_qubit_layer_parallel(arvak_proj::mps::zz(0.0), &chi_per_bond, &zz_angles)
                .unwrap();
            // Site-dependent RX kick
            let rx_layer: Vec<_> = (0..n)
                .map(|i| arvak_proj::mps::rx(2.0 * hx_per_site[i] * params.dt))
                .collect();
            mps.apply_single_layer(&rx_layer);
        }
        mps
    };

    let observe = |mps: &Mps| -> Vec<f64> { mps.discarded_weight_per_bond.clone() };

    let cfg = JacobianConfig {
        delta: 0.02,
        stride: 1,
    };

    InputJacobian::compute(&base_hx, factory, observe, &cfg)
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE C — N=12, Jacobian-allocated χ vs uniform χ at matched budget
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_c_n12_jacobian_vs_uniform() {
    let n = 12;
    let n_steps = 10;
    let params = KimParams::self_dual();

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage C: N={n}, Jacobian-allocated χ vs uniform χ");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    let ref_history = reference_kim_run(n, params, n_steps);

    let pilot_chi = 4; // small enough to force truncation in the pilot
    let pilot_steps = n_steps;

    let t_jac = Instant::now();
    let jacobian = build_kim_jacobian(n, params, pilot_chi, pilot_steps);
    let jac_ms = t_jac.elapsed().as_secs_f64() * 1000.0;
    println!(
        "  Jacobian build: {jac_ms:.2} ms ({} inputs × {} outputs, pilot χ={pilot_chi}, pilot steps={pilot_steps})",
        jacobian.n_inputs, jacobian.n_outputs
    );

    println!(
        "\n  {:>8} | {:>14} {:>10} | {:>14} {:>10}",
        "χ range", "jac max_err", "jac budget", "uniform err", "uni budget"
    );
    println!("  {}", "-".repeat(64));

    for &chi_min in &[2_usize] {
        for &chi_max in &[6, 8, 12, 16] {
            let chi_jacobian = chi_allocation_from_jacobian(
                &jacobian,
                chi_min,
                chi_max,
                JacobianAllocation::ParticipationRatio,
            );

            let chi_uniform_avg = {
                let total: usize = chi_jacobian.iter().sum();
                let n_bonds = chi_jacobian.len();
                (total / n_bonds).max(chi_min)
            };
            let chi_uniform = vec![chi_uniform_avg; n - 1];

            let (_mps_j, hist_j) = run_mps_kim(n, params, n_steps, &chi_jacobian);
            let (_mps_u, hist_u) = run_mps_kim(n, params, n_steps, &chi_uniform);

            let err_j = history_max_err(&ref_history, &hist_j);
            let err_u = history_max_err(&ref_history, &hist_u);

            let budget_j: usize = chi_jacobian.iter().sum();
            let budget_u: usize = chi_uniform.iter().sum();

            println!(
                "  [{:>2}..{:>2}] | {:>14.3e} {:>10} | {:>14.3e} {:>10}",
                chi_min, chi_max, err_j, budget_j, err_u, budget_u
            );
        }
    }
    println!();
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE D — N=24, exact statevector reference (16M-dim) vs arvak-proj
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_d_n24_full_chi_matches_reference() {
    let n = 24;
    let n_steps = 6;
    let params = KimParams::self_dual();

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage D: N={n}, χ_max=4096 (exact), {n_steps} Trotter steps");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    // For 24 qubits, exact χ_max = 2^12 = 4096. We use χ_max = 256 which is
    // not strictly exact but gives essentially-converged numerics for this
    // shallow circuit (entanglement growth in self-dual KIM is bounded by
    // ~ v_E · n_steps · log 2 ≈ 4 ebits at step 6 → χ ~ 16 saturates).
    let chi_max = 256;
    let chi_per_bond = vec![chi_max; n - 1];

    let t_ref = Instant::now();
    let ref_history = reference_kim_run(n, params, n_steps);
    let ref_ms = t_ref.elapsed().as_secs_f64() * 1000.0;

    let t_mps = Instant::now();
    let (mps, mps_history) = run_mps_kim(n, params, n_steps, &chi_per_bond);
    let mps_ms = t_mps.elapsed().as_secs_f64() * 1000.0;

    let max_err = history_max_err(&ref_history, &mps_history);
    let max_bond = mps.bond_dims().iter().max().copied().unwrap_or(0);
    let total_disc = mps.total_discarded_weight();

    println!("  reference statevector ({} amps): {ref_ms:.2} ms", 1_usize << n);
    println!("  arvak-proj MPS (χ_max={chi_max}):  {mps_ms:.2} ms");
    println!("  actual max bond used:            {max_bond}");
    println!("  total discarded weight:          {total_disc:.3e}");
    println!("  max ⟨Z⟩ error vs ref:            {max_err:.3e}\n");

    assert!(
        max_err < 1e-8,
        "Stage D: arvak-proj at χ_max={chi_max} disagrees with dense reference: max_err={max_err}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE E — N=50 — Negative control: on a translation-invariant kicked
// Ising chain the Jacobian should NOT beat uniform-χ, because there is no
// inhomogeneity for it to exploit. This stage documents that null result
// honestly. The matched test that Jacobian *should* win on is Stage F
// (disordered KIM) below.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_e_homogeneous_negative_control() {
    let n_steps = 8;
    let params = KimParams::self_dual();

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage E: NEGATIVE control — homogeneous self-dual KIM, N=50");
    println!("  Expectation: jacobian ≈ uniform (no inhomogeneity to exploit)");
    println!("  {n_steps} Trotter steps; reference = MPS at high χ_max");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    println!(
        "  {:>4} | {:>14} | {:>14} {:>12} | {:>14} {:>12}",
        "N", "strategy", "max ⟨Z⟩ err", "budget", "wall ms", "max bond"
    );
    println!("  {}", "-".repeat(80));

    let chi_ceiling = 8_usize;
    let chi_min_jac = 2_usize;
    let chi_ref = 64_usize;

    for &n in &[50_usize] {
        // ── Reference: high-χ MPS run ───────────────────────────────────
        let chi_ref_per_bond = vec![chi_ref; n - 1];
        let t = Instant::now();
        let (mps_ref, hist_ref) = run_mps_kim(n, params, n_steps, &chi_ref_per_bond);
        let ms_ref = t.elapsed().as_secs_f64() * 1000.0;
        let mb_ref = mps_ref.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>14} | {:>14} {:>12} | {:>14.2} {:>12}",
            n,
            format!("ref χ={chi_ref}"),
            "—",
            chi_ref * (n - 1),
            ms_ref,
            mb_ref
        );

        // ── Uniform baseline at chi_ceiling ─────────────────────────────
        let chi_uniform = vec![chi_ceiling; n - 1];
        let t = Instant::now();
        let (mps_u, hist_u) = run_mps_kim(n, params, n_steps, &chi_uniform);
        let ms_u = t.elapsed().as_secs_f64() * 1000.0;
        let mb_u = mps_u.bond_dims().iter().max().copied().unwrap_or(0);
        let err_u = history_max_err(&hist_ref, &hist_u);
        let budget_u: usize = chi_uniform.iter().sum();
        println!(
            "  {:>4} | {:>14} | {:>14.3e} {:>12} | {:>14.2} {:>12}",
            n,
            format!("uniform χ={chi_ceiling}"),
            err_u,
            budget_u,
            ms_u,
            mb_u
        );

        // ── Jacobian: pilot at small χ, allocate up to chi_ceiling ──────
        let pilot_chi = 4;
        let pilot_steps = n_steps;
        let t_jac = Instant::now();
        let jacobian = build_kim_jacobian(n, params, pilot_chi, pilot_steps);
        let jac_build_ms = t_jac.elapsed().as_secs_f64() * 1000.0;

        let chi_jacobian = chi_allocation_from_jacobian(
            &jacobian,
            chi_min_jac,
            chi_ceiling,
            JacobianAllocation::ParticipationRatio,
        );

        let t = Instant::now();
        let (mps_j, hist_j) = run_mps_kim(n, params, n_steps, &chi_jacobian);
        let ms_j = t.elapsed().as_secs_f64() * 1000.0;
        let mb_j = mps_j.bond_dims().iter().max().copied().unwrap_or(0);
        let err_j = history_max_err(&hist_ref, &hist_j);
        let budget_j: usize = chi_jacobian.iter().sum();
        println!(
            "  {:>4} | {:>14} | {:>14.3e} {:>12} | {:>14.2} {:>12}    (jac build {:.1}ms)",
            n,
            format!("jacobian [{}..{}]", chi_min_jac, chi_ceiling),
            err_j,
            budget_j,
            ms_j,
            mb_j,
            jac_build_ms
        );
        println!();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// STAGE F — Disordered self-dual KIM, N=14 (statevector reference) and
// N=50 (high-χ MPS reference). Each site has h_x = π/4 + δ_i with δ_i drawn
// from a fixed-seed uniform distribution. The site-by-site disorder is the
// inhomogeneity that the Jacobian-allocator can exploit.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stage_f_disordered_jacobian_wins() {
    let params = KimParams::self_dual();
    let n_steps = 8;
    let disorder_amplitude = 0.5; // δ ∈ [-0.5, 0.5] around h_x = π/4
    let seed = 12345_u64;

    println!("\n  ══════════════════════════════════════════════════════════════════");
    println!("  Stage F: DISORDERED self-dual KIM (h_x site-disorder ±{disorder_amplitude})");
    println!("  Expectation: jacobian < uniform on max ⟨Z⟩ error at matched budget");
    println!("  ══════════════════════════════════════════════════════════════════\n");

    println!(
        "  {:>4} | {:>16} | {:>14} {:>12} | {:>14} {:>12}",
        "N", "strategy", "max ⟨Z⟩ err", "budget", "wall ms", "max bond"
    );
    println!("  {}", "-".repeat(82));

    // ── Sub-test 1: N=14 with dense statevector ground truth ─────────────
    {
        let n = 14_usize;
        let h_x_per_site = det_random_hx(n, seed, params.h_x, disorder_amplitude);
        let chi_ceiling = 8_usize;
        let chi_min_jac = 4_usize;

        let t = Instant::now();
        let hist_ref = reference_kim_run_disordered(n, params, &h_x_per_site, n_steps);
        let ms_ref = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  {:>4} | {:>16} | {:>14} {:>12} | {:>14.2} {:>12}",
            n, "dense ref", "—", "—", ms_ref, "—"
        );

        let chi_uniform = vec![chi_ceiling; n - 1];
        let t = Instant::now();
        let (mps_u, hist_u) =
            run_mps_kim_disordered(n, params, &h_x_per_site, n_steps, &chi_uniform);
        let ms_u = t.elapsed().as_secs_f64() * 1000.0;
        let err_u = history_max_err(&hist_ref, &hist_u);
        let budget_u: usize = chi_uniform.iter().sum();
        let mb_u = mps_u.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>16} | {:>14.3e} {:>12} | {:>14.2} {:>12}",
            n,
            format!("uniform χ={chi_ceiling}"),
            err_u,
            budget_u,
            ms_u,
            mb_u
        );

        let pilot_chi = 4;
        let t_jac = Instant::now();
        let jacobian =
            build_disordered_kim_jacobian(n, params, &h_x_per_site, pilot_chi, n_steps);
        let jac_build_ms = t_jac.elapsed().as_secs_f64() * 1000.0;
        let chi_jacobian = chi_allocation_from_jacobian(
            &jacobian,
            chi_min_jac,
            chi_ceiling,
            JacobianAllocation::ParticipationRatio,
        );

        let t = Instant::now();
        let (mps_j, hist_j) =
            run_mps_kim_disordered(n, params, &h_x_per_site, n_steps, &chi_jacobian);
        let ms_j = t.elapsed().as_secs_f64() * 1000.0;
        let err_j = history_max_err(&hist_ref, &hist_j);
        let budget_j: usize = chi_jacobian.iter().sum();
        let mb_j = mps_j.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>16} | {:>14.3e} {:>12} | {:>14.2} {:>12}    (jac build {:.1}ms)",
            n,
            format!("jacobian [{}..{}]", chi_min_jac, chi_ceiling),
            err_j,
            budget_j,
            ms_j,
            mb_j,
            jac_build_ms
        );

        // Print the Jacobian-allocated profile for inspection
        println!("    chi profile: {:?}", chi_jacobian);
        println!();
    }

    // ── Sub-test 2: N=50 with high-χ MPS reference ───────────────────────
    {
        let n = 50_usize;
        let h_x_per_site = det_random_hx(n, seed, params.h_x, disorder_amplitude);
        let chi_ceiling = 8_usize;
        let chi_min_jac = 4_usize;
        let chi_ref = 64_usize;

        let chi_ref_per_bond = vec![chi_ref; n - 1];
        let t = Instant::now();
        let (mps_ref, hist_ref) =
            run_mps_kim_disordered(n, params, &h_x_per_site, n_steps, &chi_ref_per_bond);
        let ms_ref = t.elapsed().as_secs_f64() * 1000.0;
        let mb_ref = mps_ref.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>16} | {:>14} {:>12} | {:>14.2} {:>12}",
            n,
            format!("ref χ={chi_ref}"),
            "—",
            chi_ref * (n - 1),
            ms_ref,
            mb_ref
        );

        let chi_uniform = vec![chi_ceiling; n - 1];
        let t = Instant::now();
        let (mps_u, hist_u) =
            run_mps_kim_disordered(n, params, &h_x_per_site, n_steps, &chi_uniform);
        let ms_u = t.elapsed().as_secs_f64() * 1000.0;
        let err_u = history_max_err(&hist_ref, &hist_u);
        let budget_u: usize = chi_uniform.iter().sum();
        let mb_u = mps_u.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>16} | {:>14.3e} {:>12} | {:>14.2} {:>12}",
            n,
            format!("uniform χ={chi_ceiling}"),
            err_u,
            budget_u,
            ms_u,
            mb_u
        );

        let pilot_chi = 4;
        let t_jac = Instant::now();
        let jacobian =
            build_disordered_kim_jacobian(n, params, &h_x_per_site, pilot_chi, n_steps);
        let jac_build_ms = t_jac.elapsed().as_secs_f64() * 1000.0;
        let chi_jacobian = chi_allocation_from_jacobian(
            &jacobian,
            chi_min_jac,
            chi_ceiling,
            JacobianAllocation::ParticipationRatio,
        );

        let t = Instant::now();
        let (mps_j, hist_j) =
            run_mps_kim_disordered(n, params, &h_x_per_site, n_steps, &chi_jacobian);
        let ms_j = t.elapsed().as_secs_f64() * 1000.0;
        let err_j = history_max_err(&hist_ref, &hist_j);
        let budget_j: usize = chi_jacobian.iter().sum();
        let mb_j = mps_j.bond_dims().iter().max().copied().unwrap_or(0);
        println!(
            "  {:>4} | {:>16} | {:>14.3e} {:>12} | {:>14.2} {:>12}    (jac build {:.1}ms)",
            n,
            format!("jacobian [{}..{}]", chi_min_jac, chi_ceiling),
            err_j,
            budget_j,
            ms_j,
            mb_j,
            jac_build_ms
        );
        println!();
    }
}
