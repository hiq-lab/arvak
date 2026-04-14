# The Bianchi-Projection Journey

> **TL;DR** — We tried to validate a published proposal for *gauge-restoring
> single-step truncation correction* in MPS via the Bianchi identity in Vidal
> form. After implementing seven distinct candidate methods and testing them
> against measured truncation error on QKR/Barabási-Albert circuits, the
> honest result is: **no analytical O(N) predictor competes with the real
> finite-difference Jacobian**. The Jacobian gives Spearman 0.65 against
> measured discarded weight; every analytical predictor we tried (sin(C/2),
> Lieb-Robinson causal-cone IPR, Calabrese-Cardy/KPZ scaling) gives ≤ 0.4.
>
> **What we kept**: a corrected Bianchi-violation diagnostic in our balanced
> canonical form, a per-bond discarded-weight accumulator, a discarded-weight
> SVD truncation mode, and the finite-difference Jacobian as a public API.
>
> **What we removed**: the Bianchi projection step itself (physically cannot
> reduce gauge violation in a single-bond operation), all the failed
> analytical predictors, and the unused infrastructure that came with them.

## The original idea

Spec proposal: *In Vidal canonical form, the consistency condition*

```text
T_i Λ_i² = Λ_{i-1}² T_i
```

*holds for properly canonical MPS. After SVD truncation it is violated.
Define the violation*

```text
B_i = ‖ T_i Λ_i² − Λ_{i-1}² T_i ‖_F
```

*and apply a single gradient step Λ_i ← Λ_i − η_i · ∇ Σ B_j² with adaptive
step η_i = η₀ · sin²(C_i/2). The expected outcome was a fidelity gain that
grows with N at fixed χ-budget.*

This sounded good. The construction had four moving pieces:

1. A **gauge-consistency diagnostic** B_i computable from stored data
2. A **gradient step** that reduces B_i via local Λ updates
3. An **adaptive step size** from sin(C/2)
4. A **scaling claim** that the gain grows with system size

We set out to validate (4) via N ∈ {50, 100, 200, 500, 1000} on Barabási-
Albert / QKR-style circuits, with Tindall et al. (PRX Quantum 2024) as the
intended publication-grade benchmark.

## Phase 1 — Persist Λᵢ in SiteTensor

**What**: Added `lambda: Option<Vec<f64>>` to `SiteTensor`. Updated
`svd_truncate` to return the singular values it computed. Updated
`apply_two_qubit` to write them into the SiteTensor on the right-bond side.

**Why**: Without storing Λ on each bond, the diagnostic is not even
computable. This is foundational and stays.

**Cost**: 1h, ~50 lines, backward-compatible (existing constructors set
`lambda = None`).

## Phase 1.5 — Discarded-weight truncation mode

**What**: Added `TruncationMode::DiscardedWeight { eps }` as an alternative
to the legacy `Absolute` mode. Instead of dropping SVs below
`s_max · 1e-14`, the new mode keeps SVs until cumulative `Σ σ²` exceeds
`(1 - eps)` of the total. The 2-norm truncation error is then exactly
`√eps`, which is theoretically clean.

**Why**: The original `Absolute` mode bounds SV magnitudes but doesn't bound
the actual error. The discarded-weight mode bounds the error directly,
which composes cleanly with any error-aware analysis.

**Status**: **Kept**. Genuine improvement over the legacy criterion.

## Phase 2 — Bianchi violation diagnostic

**What**: Implemented `bianchi_violation` and `bianchi_profile` to compute
B_i per bond.

**Bug found and fixed (twice)**:

- *First implementation*: The diagonal-only formula
  `implied[α] = Σ_β |A[α,σ,β]|² · λ_right²[β]` compared against `λ_left²[α]`.
  This worked for left-canonical form but **not** for arvak-proj's balanced
  canonical form (where SVD distributes √S onto both adjacent sites).

  Bell state and GHZ-4 produced "violation" ~0.2 even though they are
  perfectly canonical. The diagnostic was measuring a non-existent gauge
  break introduced by the formula's wrong assumption about the storage
  convention.

- *After balance-form rederivation*: For the balanced form where
  `A^σ[α,β] = U[α·d+σ, β] · √λ[β]`, the SVD identity `M = U·diag(λ)·V†`
  via Parseval on V gives:

  ```text
  Σ_{σ,β} |A^σ[α,β]|² · λ_right[β] = λ_left[α]
  ```

  Note the **single power** of λ on both sides (NOT λ²). The previous
  implementation had quadratic powers everywhere. After the fix:

  - Bell state: B_i ≈ 1e-16 (machine epsilon) ✓
  - GHZ-4: B_i ≈ 5e-16 ✓
  - Truncated states: real, finite B_i proportional to discarded weight ✓

**Status**: **Kept** with corrected math. The B_i diagnostic is a genuine
metric — it measures how much the stored bond environment deviates from
what the actual site tensors imply. Useful as a sanity-check after long
simulations.

**Cost of bug**: We spent several phases chasing artifacts of the wrong
gauge formula. The lesson is brutal: always validate analytic formulas
against known canonical states (Bell, GHZ) before trusting them.

## Phase 3 — Bianchi projection step

**What**: Implemented `project_bond` (Λ-only gradient step) and
`project_bond_rotation` (Schmidt-basis rotation). Adaptive step size
η_i = η₀ · sin²(C_i/2) from the channel map.

**Result**: **The projection cannot reduce the violation**. Two reasons:

1. *Λ-only update*: Can only correct the diagonal (population) part of
   the violation. The diagonal is already optimal after SVD truncation;
   the SVD picks the truncated state minimizing 2-norm error locally.
   Adjusting Λ alone moves you OFF the truncation-optimal manifold.

2. *Bond-local rotation*: A unitary rotation at the bond changes the
   basis but does not reduce the Frobenius norm of the residual matrix
   `R - Λ²·I` — both `R` and `I` transform covariantly under the
   rotation, so their difference has the same norm.

**The honest theorem**: No single-bond operation can reduce a real Bianchi
violation. The violation reflects information loss from truncation; the
information is gone, not hidden behind a gauge. Restoring it requires a
re-canonicalization sweep (as in DMRG between iterations), which is
exactly the O(N · χ³) operation the projection was supposed to avoid.

**Status**: **Removed**. The diagnostic stays, the projection goes.

## Phase 4 — Pipeline integration

**What**: Wired `project_bond` into `apply_two_qubit` behind a `bianchi`
Cargo feature, with the sin(C/2) weights set on the MPS via
`set_sin_c_half`.

**Status**: **Removed** along with the projection. The `bianchi` feature
flag is gone. The `sin_c_half_per_bond` field on `Mps` is gone.
`apply_two_qubit` no longer has the projection step.

## Phase 5 — Barabási-Albert validation

**What**: Three-mode comparison (uniform-χ baseline, sin(C/2) adaptive-χ,
adaptive-χ + Bianchi projection) on QKR circuits with N ∈ {50, ..., 1000},
χ = 8, n_steps = 16.

**Result**: Bianchi viol ratio between baseline and projection:
1.00x to 1.06x — essentially zero effect. This confirmed Phase 3's
theoretical conclusion empirically.

**Side observation that became important**: Adaptive χ from sin(C/2)
gave only ~10% lower discarded weight than uniform — sometimes higher,
sometimes lower depending on N. The expected scaling didn't materialize.

**Status**: **Removed** as a benchmark, **lessons retained** in this doc.

## Phase 5e — Discarded-weight accumulator + sin(C/2) validation

**What**: Tracked the cumulative discarded weight per bond as
`Mps::discarded_weight_per_bond`, accumulated by `apply_two_qubit` after
each SVD. This is the **actual** 2-norm² truncation error, summed over
the lifetime of the MPS.

Then computed Spearman correlation between sin(C/2) (the supposed a-priori
predictor) and discarded weight (the a-posteriori measurement).

**Result**:

| N    | sin(C/2) ↔ disc Spearman |
|------|--------------------------|
| 50   | -0.13                    |
| 100  | +0.14                    |
| 200  | +0.18                    |
| 500  | -0.06                    |
| 1000 | **-0.20**                |

The correlation is **statistically null** and at large N becomes
**negative**. The sin(C/2) predictor on QKR/Barabási-Albert is empirically
unfounded.

**Status**: **Discarded-weight accumulator kept** (it's a genuine novel
metric — none of the published MPS tools track per-bond cumulative
truncation error). **The sin(C/2) hypothesis is officially negative on
this benchmark family.**

## Phase 5f — Sobol-style sensitivity (finite-difference Jacobian)

**What**: Computed the finite-difference Jacobian
`J[k][i] = ∂(disc_k)/∂(ω_i)` for QKR circuits. Took the per-bond
participation ratio of `J[k][:]` as a "sensitivity spread" predictor.

**Result**:

| N   | PR ↔ disc Spearman |
|-----|--------------------|
| 20  | **+0.71**          |
| 50  | **+0.65**          |
| 100 | **+0.69**          |

**This is the gold standard.** The Jacobian directly probes the actual
linear-response dependency between inputs and outputs. No theory, no
heuristic — just the slope of the input-output map measured numerically.

**Status**: **Kept and generalised** to the public
`finite_difference_jacobian` module in Phase 5i.

## Phase 5g — Causal-cone IPR with Lieb-Robinson weights

**What**: A closed-form, O(N), purely topological alternative to the
Jacobian. For each bond k, compute the past causal cone of all gates that
could affect it via Lieb-Robinson, weight each gate by its entangling
strength `sin²(θ/2)` and exponentially suppress outside the cone, then
take the participation ratio of the resulting per-gate weights.

**Idea source**: After deep web research (Lieb-Robinson bounds, OTOCs,
Krylov complexity, Wang-Hazzard 2020, Nahum-Vijay-Haah 2018, Kuwahara-
Saito 2024). The literature was clear: causal-cone + LR weight + IPR is
the "right" closed-form analogue of forward-mode AD sensitivity.

**Result**:

| N   | LR-IPR ↔ disc Spearman |
|-----|------------------------|
| 50  | +0.39                  |
| 100 | +0.26                  |
| 200 | +0.21                  |
| 500 | +0.15                  |
| 1000| +0.17                  |

Better than sin(C/2), but **far worse than the Jacobian** (0.65). The
gap is exactly the information that lives in the *phases and signed
contributions* inside the cone — interference between paths that LR
treats as uniform.

**Status**: **Removed.** Topology alone is insufficient when the circuit
has structured (non-random) gate angles like our 2^k QKR sequence. The
Lieb-Robinson cone is a worst-case envelope; it ignores constructive vs.
destructive path interference.

## Phase 5h — truncmap (Calabrese-Cardy + KPZ scaling)

**What**: Closed-form scaling formula inspired by Alex Wellerstein's
Nukemap radius computations:

```text
S_k(t) = c · v · t / 6           for t < k_eff/v   (linear growth)
        = c · k_eff / 3            for t ≥ k_eff/v   (saturation)
disc_k(T) ≈ A · max(0, T - t*_k)^(2/3) · v · c
```

with `t*_k = max(6 · log(χ_max) / (c·v), k_eff/v)`, `k_eff` = distance to
nearest boundary, `v` = global Lieb-Robinson velocity from mean
entangling power, c = 1 (free fermion central charge).

**Idea source**: Calabrese & Cardy quench entropy growth, Nahum-Vijay-Haah
KPZ roughness front — both well-established universal scaling laws for
random quantum circuits.

**Result**:

| N    | truncmap Spearman |
|------|-------------------|
| 50   | +0.50             |
| 100  | +0.26             |
| 200  | +0.15             |
| 500  | +0.10             |
| 1000 | +0.05             |

**Better than LR-IPR at small N, worse at large N.** Calabrese-Cardy is a
**mean-field theory** — it tells you the average entropy growth, not the
per-bond profile. For structured (non-random) circuits the per-bond
variation comes entirely from interference effects that the mean-field
formula averages away.

**Status**: **Removed.** Mean-field scaling is too coarse for per-bond
predictions on structured circuits.

## Phase 5i — Finite-difference Jacobian as public API

**What**: Refactored Phase 5f's inline FD Jacobian into a clean public
module `finite_difference_jacobian.rs`:

```rust
let j = InputJacobian::compute(&base_inputs, factory, observe, &cfg);
let pr = participation_ratio_profile(&j);
let chi = chi_allocation_from_jacobian(
    &j, chi_min, chi_max,
    JacobianAllocation::ParticipationRatio,
);
```

**Status**: **Kept.** This is the gold-standard predictor for adaptive
χ allocation. Cost: O(N · primal). For Tindall (N=127, T=20), the
Jacobian completes in ~120 seconds, which is acceptable.

**Why finite differences and not real forward-mode AD?**

For our use case the two are operationally equivalent:
- Same numerical values (up to O(δ²) discretization, negligible)
- Same asymptotic cost (O(N · primal))
- Same downstream behaviour (PR, chi allocation, correlation)

A real forward-mode AD with sparse tangents would be ~3-4 days of engineering
(SVD differentiation, dual MPS data structures, etc.) and would give the
same numerical result. We chose to keep the implementation honest — the
filename is `finite_difference_jacobian.rs`, not `forward_mode_ad.rs`.

## What we kept vs. what we removed

### Kept

| File / construct | What it does |
|------------------|--------------|
| `mps.rs::SiteTensor::lambda` | Per-bond singular values, populated by SVD |
| `mps.rs::TruncationMode::DiscardedWeight` | Calibrated 2-norm error truncation |
| `mps.rs::Mps::discarded_weight_per_bond` | Cumulative truncation error tracker |
| `mps.rs::Mps::reset_discarded_weight` | Reset the tracker |
| `mps.rs::Mps::discarded_weight` / `total_discarded_weight` | Read accessors |
| `mps.rs::Mps::get_cost` | Σ χ³ proxy for compute footprint |
| `bianchi.rs::transfer_matrix` | Hermitian per-site transfer matrix (single power) |
| `bianchi.rs::bianchi_violation` | Per-site B_i in balanced canonical form (corrected math) |
| `bianchi.rs::bianchi_profile` | Vector of B_i per bond |
| `bianchi.rs::total_bianchi_violation` | Scalar L2 of the profile |
| `finite_difference_jacobian.rs::InputJacobian` | The gold-standard predictor |
| `finite_difference_jacobian.rs::participation_ratio_profile` | Per-bond PR |
| `finite_difference_jacobian.rs::total_sensitivity_profile` | Per-bond ‖J_k‖₁ |
| `finite_difference_jacobian.rs::chi_allocation_from_jacobian` | χ from PR or sensitivity |

### Removed

| What | Why |
|------|-----|
| `bianchi.rs::project_bond` (Λ-only) | Cannot reduce gauge violation in single bond |
| `bianchi.rs::project_bond_rotation` | Unitary rotation doesn't change Frobenius norm |
| `bianchi.rs::project_all` / `project_all_rotation` | Drivers for the failed projections |
| `bianchi.rs::BianchiConfig` / `ProjectionStats` | Configuration for projection |
| `bianchi.rs::causal_cone_ipr` | LR topology only, ~0.2-0.4 Spearman |
| `bianchi.rs::qkr_gate_history` / `entangling_power_per_bond` | Used only by failed predictors |
| `bianchi.rs::truncmap` | Mean-field, ~0.05-0.5 Spearman |
| `bianchi.rs::GateRecord` | Data structure for failed methods |
| `Cargo.toml::[features].bianchi` | Feature flag for projection that doesn't work |
| `mps.rs::Mps::sin_c_half_per_bond` | Was used to feed the projection step |
| `mps.rs::Mps::set_sin_c_half` | Setter for above |
| `apply_two_qubit::cfg(feature="bianchi") block` | Wire-up of removed projection |
| All Phase 5 / 5e / 5f / 5g / 5h validation tests | Replaced by Phase 5i validation |

## Lessons

1. **Validate analytical formulas against known canonical states.** The
   Phase 2 bug (wrong λ powers in the balanced canonical form) cost us
   several phases of confused investigation. A 30-second test on a Bell
   state would have caught it immediately. We added that test only after
   the math was already wrong.

2. **Topology alone is not enough for structured circuits.** Lieb-
   Robinson bounds, Calabrese-Cardy scaling, and similar universal
   results are *averaged* statements about random or generic systems.
   For structured circuits like QKR (with 2^k angle sequences), the
   per-bond variation comes from interference effects that average out
   in any mean-field description.

3. **Mean-field predictors saturate.** truncmap was excellent at N=50
   (0.50 Spearman) but degraded fast. The ratio of the prediction range
   to the actual variation collapsed as N grew. This is a general
   feature of any "predictor that fits constants from one regime."

4. **Single-bond operations cannot reduce global gauge violations.**
   This is a theorem, not an implementation detail. If you measure
   inconsistency between bonds, only an operation that touches both
   bonds (a sweep) can fix it.

5. **The Jacobian is the linear-response answer and is finite-cost.**
   For N inputs and primal cost C, the Jacobian costs `2NC`. For our
   target benchmarks (N=127 Tindall, N=1000 BA) this is ≤ a few minutes
   on a workstation. There was no need for a faster heuristic — the
   ground truth was always tractable.

6. **Negative results have value.** sin(C/2) does not predict per-bond
   discarded weight on QKR/BA circuits. Lieb-Robinson topology does not
   either. Calabrese-Cardy scaling does not either. Each of these is a
   *real* observation that constrains future work — they aren't failures
   of effort, they're boundaries of known theory.

## What we did NOT validate

- **sin(C/2) on the original BA-topology benchmarks** (the published 135×
  speedup result). Those used a different benchmark family — adaptive
  χ allocation as part of bond-budget partitioning, not per-bond
  truncation prediction. This document does NOT contradict that result;
  it only says that the *naive* "sin(C/2) predicts per-bond discarded
  weight" hypothesis fails on QKR with 2^k angle sequences.

- **The Tindall et al. 127-qubit kicked Ising benchmark.** This was the
  original validation target (Phase 6). It is still the right next step,
  now using `finite_difference_jacobian` as the predictor instead of
  sin(C/2).

## Pointer to the working code

Everything that survived is in three files:

- `crates/arvak-proj/src/mps.rs` — Λ persistence, discarded weight tracker,
  truncation modes
- `crates/arvak-proj/src/bianchi.rs` — diagnostic only (no projection)
- `crates/arvak-proj/src/finite_difference_jacobian.rs` — the predictor

Plus this document for context.
