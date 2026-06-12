# Architecture Review — Stability, Mathematical Accuracy, Efficiency, Dead Code

> **Status update (same branch):** the following findings are FIXED in the
> commits that follow this review, each guarded by the new
> `crates/arvak-compile/tests/unitary_equivalence.rs` suite and unit tests:
> **C1** (Optimize1qGates order + convergence, incl. the ZSX identity),
> **C2/C3** (Rx/Ry translations for IBM/Eagle/Heron),
> **C4** (IQM Rz order), **C5** (CX→ECR decomposition; DEBT-03 resolved —
> the gate.rs ECR matrix is Qiskit's ECR correctly converted to big-endian,
> there was no convention clash),
> **C6** (BasisTranslation now errors on entangling custom gates;
> ConsolidateBlocks removed from the default O3 pipeline until KAK local
> extraction lands), **C7** (projective reset), **H2** (simulate-once +
> cumulative sampling, optional seed), **H3** (verification simulator now
> implements all StandardGates and errors instead of skipping),
> **M1** (parser recursion limit), **M3** (acos clamp),
> **M4** (IQM S/Sdg/T/Tdg/SX/SXdg; Heron S/Sdg/T/Tdg), and the silent
> gate-truncation branch in Optimize1qGates. Remaining open: H1 (bitstring
> convention), M2, M5–M7, D1–D5.

**Date:** 2026-06-12
**Scope:** Full workspace (74k LOC Rust): deep review of `arvak-ir`, `arvak-compile`,
`arvak-adapter-sim`, `arvak-qasm3` (parser entry points); targeted review of
`arvak-sched`, `arvak-grpc`, `arvak-hal`, and all 12 adapters; workspace-level
dead-code and dependency sweep. All mathematical claims below were verified
numerically (NumPy cross-checks against standard gate definitions) and, for the
most severe finding, empirically against the compiled pass.

---

## Executive summary

The orchestration layer (HAL, scheduler, gRPC, adapters) is in good shape:
prior audit rules are visibly applied (HTTP timeouts everywhere, token
redaction, cache eviction, tokio locks, canonical `hal-contract` types).

The compiler's mathematics is not. **Five independent, verified correctness
bugs exist in the optimization/translation pipeline**, several on the default
path (optimization level 1). The root cause is systemic: gate identities were
hand-derived, encoded in three separate hand-rolled matrix tables, and tested
only by gate *count* or with palindromic/commuting circuits that cannot detect
ordering or sign errors. Any circuit containing non-commuting single-qubit
runs, or `rx`/`ry` gates targeting IBM-family or IQM bases, currently compiles
to a circuit that computes the wrong state.

---

## Critical — mathematical correctness

### C1. `Optimize1qGates` merges runs in reverse order and never converges
`crates/arvak-compile/src/passes/agnostic/optimization/optimize_1q.rs:332`

```rust
combined = combined * u;   // builds U_first · U_second · … (wrong)
```

The codebase's convention is column-vector (applying g1 then g2 yields
`U2·U1`; established by `from_gate_sequence_1q2q` and the translation tests).
The merge loop builds the product in the *opposite* order, so any
non-commuting run is replaced by a circuit for the reversed product.

**Empirically confirmed** (temporary test, since removed): the circuit
`[H, S]` (correct unitary `S·H`) is optimized to `[Ry(π/2), Rz(−π/2)]`,
which implements `RZ(−π/2)·RY(π/2)` — equivalent to neither `S·H` nor `H·S`.

The reason it is *neither*: the pass re-discovers the freshly emitted 2-gate
run on the next iteration, re-merges it (reversed again), emits the flipped
form, and **ping-pongs between two forms until `MAX_ITERATIONS = 200`** is
exhausted (optimize_1q.rs:312–392). Consequences:

- Wrong circuit semantics for every non-commuting 1q run at **opt level ≥ 1
  (the default)**.
- 200 full-DAG rescans (`find_1q_runs` does a complete toposort each time)
  per affected run — a large compile-time waste even when the math is benign.
- Final output depends on iteration-cap parity, with only a `tracing::warn`.

**Fix:** `combined = u * combined;` plus a convergence criterion (stop when
the emitted run equals the input run, or compare canonical ZYZ angles).

### C2. `Rx` translation wrong for IBM, Eagle, and Heron bases
`crates/arvak-compile/src/passes/target/translation.rs:372–382, 485–491, 602–608`

Emitted: `Rz(−π/2)·SX·Rz(θ)·SX·Rz(−π/2)`. At θ=0 this equals **X**, not I.
Numerically verified ≠ `RX(θ)` (and ≠ `RX(−θ)`) for generic θ.
Correct identity (Qiskit U3 form): `RZ(π/2)·SX·RZ(θ+π)·SX·RZ(π/2)` (global
phase aside) — note the **θ+π** middle angle and **+π/2** outer angles.

### C3. `Ry` translation wrong for Eagle and Heron bases
`translation.rs:492–497, 609–616`

Emitted: `SX·Rz(θ)·SX·Rz(−π)` (≠ `RY(±θ)`, verified). Correct:
`RZ(π)·SX·RZ(θ+π)·SX`. The plain-IBM branch (`SX·Rz(θ)·SXdg`,
translation.rs:385–389) **is correct** — but Eagle/Heron deliberately avoid
`SXdg` and substituted a wrong identity. The same wrong identity is
duplicated in `Optimize1qGates::zyz_to_zsx`
(optimize_1q.rs:171–205), which is the **default 1q re-synthesis for any
target with `rz`+`sx`** (manager.rs:167–174), so even pure-CX/H circuits that
get re-synthesized through ZSX with β∉{0} acquire the error.

### C4. IQM `Rz` translation emits the inverse rotation
`translation.rs:269–275`

The comment derives `Rz(θ) = PRX(π, θ/2) · PRX(π, 0)` in matrix notation
(right factor applied first), but the instruction vector emits
`[PRX(π, θ/2), PRX(π, 0)]` — application order reversed. Verified: the
emitted sequence implements `RZ(−θ)`. (The `Z` and `H` cases on the same page
are correct; only `Rz` has the swap.) Every IQM-targeted circuit with a
generic Z-rotation rotates the wrong way. **Fix:** swap the two instructions.

### C5. Eagle `CX → ECR` decomposition is wrong, and the ECR convention is self-contradictory
`translation.rs:509–522`, `crates/arvak-ir/src/gate.rs:91–96`,
`verify_compilation.rs:484–507`, `translation.rs:856–862 (DEBT-03 TODO)`

The emitted sequence `RZ(π/2)⊗RZ(π/2) → ECR → X(q0)·SX(q1)` does not equal
CX under the ECR matrix documented in `gate.rs` (and re-implemented in
`verify_compilation.rs`) in *either* qubit-ordering convention — verified by
solving `Post = CX·Pre†·ECR†` (the residual is not even a tensor product, so
no sign-tweak rescues it). Under the project's own convention (first qubit =
high bit of the 4×4 block), the correct relation has **identity pre-rotations**
and all correction after the ECR:
`CX = (X·RZ(π/2) on q0) ⊗ (Z·SX·Z on q1) · ECR`.
The `DEBT-03` TODO already admits the gate.rs ECR matrix and the 2-qubit
algebra disagree on endianness. Pin the convention first, then derive and
**unitary-test** the decomposition.

### C6. KAK synthesis emits circuits with identity local factors
`crates/arvak-compile/src/unitary.rs:711–717`, `translation.rs:160–200`

`kak_decompose()` deliberately sets `a0/a1/b0/b1 = I` for entangling
unitaries ("synthesis happens in a later pass" — it doesn't), yet
`decompose_custom_2q` calls `kak.to_circuit()` on exactly those
decompositions. The result is correct only when the input happens to be a
bare canonical interaction. Additionally, the 1-CNOT branch of `to_circuit`
(`CX` followed by `Rz(0, −2tx)`, unitary.rs:1232–1246) is not a valid
synthesis of `exp(i·tx·XX)` even with correct locals.

**Blast radius:** `ConsolidateBlocks` (opt level 3, manager.rs:154–156)
replaces 2-qubit blocks with `CustomGate` matrices; `BasisTranslation` then
synthesizes them through this path → **silently wrong circuits at O3**.
Until local-factor extraction is implemented, this path should return
`CompileError` rather than a wrong circuit (project rule: never silently
produce incorrect results).

### C7. Simulator `Reset` is unphysical and can annihilate the state
`adapters/arvak-adapter-sim/src/statevector.rs:453–479`

`reset()` *coherently adds* the |1⟩ amplitude into |0⟩
(`amplitudes[j] += val`). Resetting a qubit in |−⟩ = (|0⟩−|1⟩)/√2 sums to
zero, the norm guard skips renormalization, and the entire statevector
becomes zero — subsequent sampling returns the fallback index. A reset is a
non-unitary *projection*: zero the |1⟩ amplitudes and renormalize (or sample
the measurement outcome and conditionally apply X).

---

## High

### H1. Bitstring endianness is inconsistent across backends
- Sim adapter: reverses, q0 = **leftmost** char (`statevector.rs:500–505`).
- IBM adapter: `hex_to_binary` keeps Qiskit order, q0 = **rightmost**
  (`adapters/arvak-adapter-ibm/src/backend.rs:402–419`).
- IQM adapter: emits the API's array order as-is (`backend.rs:165–180`).

The same Bell-state outcome is keyed `"10"` on one backend and `"01"` on
another. For an orchestration layer whose value proposition is backend
portability, `Counts` needs one documented convention with per-adapter
normalization (and a conformance test shared by all adapters).

### H2. Simulator re-runs the full circuit once per shot
`adapters/arvak-adapter-sim/src/simulator.rs:100–117`

`Measure` doesn't collapse and `Reset` is deterministic, so all shots evolve
the identical statevector; the loop costs `shots × O(G·2^n)` for zero
behavioral benefit (1000× slowdown at 1000 shots). Simulate once, then sample
the final distribution `shots` times. While there: `sample()` is an O(2^n)
linear scan per shot (`statevector.rs:482–497`) — precompute a cumulative
distribution; `thread_rng()` per call also defeats reproducibility (no seed
support).

### H3. `VerifyCompilation` skips gates silently and is absent from the default pipeline
`verify_compilation.rs:508–516`, `manager.rs:124–186`

Unsupported gates (RXX/RYY/RZZ, CRx/CRy, CCX, …) are warn-and-skipped, so
"verified equivalent" can be vacuous — this contradicts the project's own
"never silently skip unsupported operations" rule, in the one pass whose
entire job is catching mistakes. It also never runs in
`PassManagerBuilder` pipelines, only in tests.

### H4. Test-coverage gap that hides C1–C6
Every end-to-end semantic test (`tests/measurement_safety.rs`,
translation tests) uses only `H`, `CX`, `RZ` — gates whose translations are
correct, palindromic, or commuting. Translation tests assert gate *counts*
(`assert_eq!(dag.num_ops(), 5)`), not unitaries. Required: a property test
that, for every (gate, basis) pair and random angles, simulates the
translated circuit against the original (the `VerifyCompilation` simulator
can do this today for n ≤ 2).

---

## Medium

- **M1. Unbounded recursion in the QASM3 expression parser**
  (`crates/arvak-qasm3/src/parser/expression.rs:10–137`): recursive descent
  with no depth limit; deeply nested `((((…))))` in user-submitted QASM can
  overflow the stack and abort the gRPC/dashboard process. Add a depth
  counter (e.g. 256) returning a parse error.
- **M2. Blocking SQLite I/O on async executor threads**
  (`crates/arvak-sched/src/persistence/sqlite_store.rs`): async trait methods
  take a `std::sync::Mutex<rusqlite::Connection>` and run queries inline.
  Not held across `.await` (good), but blocks the runtime thread under load —
  wrap in `tokio::task::spawn_blocking` (project rule).
- **M3. `zyz_decomposition` NaN path** (`unitary.rs:260`):
  `a.norm().acos()` is NaN when |a| = 1+ε from rounding; the `.clamp(0,π)`
  is applied to the *result* (NaN-propagating), and
  `normalize_angle` later maps NaN→0 silently. Clamp the acos *argument*:
  `a.norm().min(1.0).acos()`.
- **M4. IQM basis missing S/Sdg/T/Tdg/SX translations**
  (`translation.rs:326–330`): common Clifford+T gates error with
  `GateNotInBasis` on IQM targets even though they are trivially expressible
  (S = Rz(π/2), etc. — once C4 is fixed).
- **M5. `SimulatorBackend::submit` is synchronous in disguise**
  (`simulator.rs:208–232`): awaits the whole simulation before returning the
  job id (status is always `Completed`), and constructs a throwaway
  `SimulatorBackend` inside `spawn_blocking` to call `run_simulation`, which
  doesn't need `self`. Make `run_simulation` a free function and either
  return immediately (true async job) or document the sync contract.
- **M6. Simulator ignores the classical-bit mapping**
  (`statevector.rs:54–60`): counts keys cover *all* qubits regardless of
  which were measured into which clbits, and mid-circuit measurement does not
  collapse the state — diverging from hardware-backend semantics.
- **M7. `Statevector::new` panics (assert ≤ 26 qubits)** while
  `with_max_qubits` accepts any value (`statevector.rs:19–23`,
  `simulator.rs:57`): a config of `max_qubits = 30` converts into a task
  panic instead of a validation error.

---

## Dead code & redundancy

- **D1. Orphaned crates:** `arvak-auto` and `arvak-bench` have no internal
  dependents, no `[[bin]]`, and are not re-exported by `arvak-python`
  (which uses only `arvak-sim`/`arvak-proj`). Either wire them into the CLI,
  publish them, or move them out of the default workspace build.
  (`arvak-qdmi-device` is a `cdylib` plugin and `arvak-dashboard` a binary —
  legitimately standalone.)
- **D2. Three hand-rolled copies of the gate-matrix table:**
  `arvak-compile/src/unitary.rs`, `verify_compilation.rs` (own statevector +
  matrices), `arvak-adapter-sim/src/statevector.rs` (kernel per gate). C2–C5
  exist *because* each site re-derives identities independently. Extract a
  single `arvak-ir::matrix` (or `arvak-types`) module: `StandardGate →
  matrix`, one documented endianness, one apply-kernel — then every consumer
  and every translation rule can be property-tested against it.
- **D3. Duplicate `EPSILON` constants** — already acknowledged at
  `unitary.rs:11–13` vs `optimization/mod.rs:15`; consolidate.
- **D4. `SimJob.circuit` is `#[allow(dead_code)]`** (`simulator.rs:24–25`)
  — stored, cloned per job, never read. 21 `#[allow(dead_code)]` sites
  workspace-wide deserve a pass; `cargo check --workspace` is otherwise
  warning-clean.
- **D5. `CircuitDag::apply` clones every instruction**
  (`dag.rs:241`) although it owns the argument — restructure to move it into
  the node (wire lists can be captured first). Minor but on the hottest IR
  path. Likewise `topological_ops()` collects a full `Vec` per call and is
  invoked repeatedly by passes (200× by C1's loop).

---

## What is in good shape

- `CircuitDag`: O(1) `wire_front` appends, careful swap-remove index fix-ups
  in `remove_op`, arity/duplicate-qubit validation, integrity checker.
- `BasisTranslation` rebuilds the DAG rather than using `substitute_node`,
  with a regression test for the historical ordering bug.
- Orchestration hygiene: timeouts on all 8 reqwest clients, manual `Debug`
  with `[REDACTED]` tokens (`arvak-hal/src/auth.rs`), bounded job caches with
  terminal-state eviction (`grpc/storage/memory.rs`, sim adapter), tokio
  locks in the scheduler, `JobStatus` sourced from the canonical
  `hal-contract` crate (no enum drift), adapters share `arvak_qasm3::emit`
  instead of hand-rolling QASM.
- `arvak-proj` MPS: truncation-error accounting is documented and validated
  against the Kim et al. benchmark suite (not deep-reviewed here).

## Recommended order of attack

1. Fix C1 (one-line order fix + convergence check) and C4 (swap two lines) —
   small, surgical, default-path.
2. Build the shared gate-matrix module (D2) + property test
   "translate/optimize then simulate ≡ original" for all bases over random
   angles (closes H4, would have caught C1–C6).
3. Re-derive C2/C3/C5 against that test; resolve the ECR endianness debt
   (DEBT-03) first.
4. Make `decompose_custom_2q` error on entangling customs until KAK local
   extraction lands (C6), or gate `ConsolidateBlocks` behind it.
5. Fix C7 + H2 in the simulator (projection reset; simulate-once,
   sample-many).
6. Define the counts bitstring convention (H1) and add adapter conformance
   tests.
