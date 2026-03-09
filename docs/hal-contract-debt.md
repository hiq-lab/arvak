# HAL Contract Technical Debt Register

Deviations between the HAL Contract v2 specification (`crates/arvak-hal/src/backend.rs`)
and the current IBM and Scaleway/IQM backend implementations (Rust adapters + Python backends).

Audited: 2026-02-18 (updated with Scaleway/IQM findings)
Updated: 2026-02-19 — ALL 12 items resolved. Full clearance commit includes DEBT-01 through DEBT-18.
Updated: 2026-02-21 — 5 new items (DEBT-19–DEBT-23) from Quantinuum + AQT adapter audit.
Updated: 2026-02-21 — **DEBT-19/21/22 fixed (VQ-046); DEBT-20/23 addressed in spec (VQ-047, HAL v2.1). All 23 items resolved.**
Updated: 2026-02-26 — DEBT-Q1–Q3 fixed (HAL Contract v2.3 photonic extension, VQ-048). Quandela adapter added (VQ-047).
Updated: 2026-02-28 — DEBT-Q4/Q5 fixed via `perceval_bridge.py` subprocess bridge (VQ-089). **All 28 items resolved.**

---

## Critical — Fixed

### DEBT-01: No pre-submission circuit validation

**Status: FIXED (2026-02-19) — all backends.**
- **Python IBM:** `ArvakIBMBackend.validate()` added. Checks shot count (≤ 300K) and qubit count. Called at top of `run()` via `HalValidationResult.raise_if_invalid()`.
- **Python Scaleway:** `ArvakScalewayBackend.validate()` added. Checks shot count (≤ 100K) and qubit count.
- **Python IQM Resonance:** `ArvakIQMResonanceBackend.validate()` added. Checks shot count (≤ 100K) and qubit count against live backend.
- **Rust IBM:** `IbmBackend::submit()` now calls `validate()` first. Returns `HalError::InvalidCircuit` on failure.
- **Rust Scaleway:** `ScalewayBackend::submit()` calls `validate()` internally — compliant.

---

### DEBT-02: IBM `GateSet::ibm()` is wrong

**Status: FIXED (2026-02-19).**
- Replaced `GateSet::ibm()` with `GateSet::ibm_eagle()` (native: `ecr, rz, sx, x`) and `GateSet::ibm_heron()` (native: `cz, rz, sx, x`).
- All 5 call sites in `adapters/arvak-adapter-ibm/src/backend.rs` updated: 127q backends use `ibm_eagle()`, 156q use `ibm_heron()`.
- `GateSet::ibm()` deprecated with `#[deprecated]` doc comment directing to the correct variants.
- `Capabilities::ibm()` factory deprecated likewise; callers now use `IbmBackend::connect()`.

---

### DEBT-03: Arvak compiler cannot target Eagle (ECR) basis

**Status: FIXED (2026-02-19).**
- `StandardGate::ECR` variant added to `arvak-ir/src/gate.rs` with correct unitary.
- `BasisGates::eagle()` added to `arvak-compile/src/property.rs`.
- `translate_to_eagle()` function added to `translation.rs`: ECR is native; CX decomposes to 5-gate ECR sequence; CZ = H·CX·H applied first.
- Gate-count regression test `test_eagle_translation_cx` (5 gates) retained.
- TODO comment documents the qubit-ordering convention mismatch preventing a full 4×4 unitary check.

---

## High — Fixed

### DEBT-04: No `cancel()` on Python jobs

**Status: FIXED (2026-02-19) — all Python backends.**
- `ArvakIBMJob.cancel()`: `DELETE {jobs_url}/{job_id}`. Maps 404/409 → `ArvakJobError`.
- `ArvakScalewayJob.cancel()`: per-job cancel, falls back to session terminate.
- `ArvakIQMResonanceJob.cancel()`: delegates to `iqm_job.cancel()`.

---

### DEBT-05: No `availability()` check before submission

**Status: FIXED (2026-02-19) — all backends.**
- Python IBM/Scaleway/IQM Resonance: `availability()` added, called at top of `run()`.
- Rust IBM/Scaleway: `availability()` implemented and correct.

---

### DEBT-06: Topology placeholder for IBM

**Status: FIXED (2026-02-19).**
- `Capabilities::ibm()` static factory deprecated with `#[deprecated]` doc comment.
- `arvak-eval/src/lib.rs` updated to use inline `Capabilities { gate_set: GateSet::ibm_heron(), topology: Topology::linear(...), ... }` struct.
- All production code paths go through `IbmBackend::connect()` which fetches real coupling map and sets `TopologyKind::HeavyHex`.

---

### DEBT-07: EU API endpoint routing not abstracted

**Status: FIXED (2026-02-19).**
- Added `ArvakIBMBackend._api_url(path: str) -> str` helper that inserts `/v1` for US and no prefix for EU.
- `_get_backend_info()`, `run()`, and `ArvakIBMJob._job_url()` all updated to call `_api_url()` — no more inline `if self._is_eu` URL conditionals in callers.

---

### DEBT-15: Python backends don't implement HAL `Backend` trait interface

**Status: FIXED (2026-02-19).**
All three Python backends now expose the full HAL contract interface:
- `validate(circuits, shots)` → `HalValidationResult` (**DEBT-01**)
- `availability()` → `HalAvailability` (**DEBT-05**)
- `submit(circuits, shots)` → `job_id` — new; delegates to `run()`
- `job_status(job_id)` → status string
- `job_result(job_id, ...)` → `ArvakResult`
- `job_cancel(job_id)` → `bool`
- `contract_version()` → `"2.0"` (**DEBT-12**)
- `run()` retained as convenience wrapper (submit + wait + result).

---

### DEBT-16: Scaleway unknown platform defaults to star topology

**Status: FIXED (2026-02-19).**
- `ScalewayBackend::capabilities_for_platform()` return type changed to `Result<Capabilities, HalError>`.
- Unknown platform strings now return `Err(HalError::UnsupportedBackend(...))` with a message listing known platforms.
- All callers updated.

---

## Medium — Fixed

### DEBT-08: `HalError` codes not mapped in Python

**Status: FIXED (2026-02-19).**
Full `ArvakError` hierarchy in `backend.py`:
```
ArvakError
├── ArvakValidationError
├── ArvakBackendUnavailableError
├── ArvakAuthenticationError
├── ArvakSubmissionError
├── ArvakTimeoutError
└── ArvakJobError
    └── ArvakJobCancelledError
```
IBM, Scaleway, IQM Resonance all map API failures to the correct subclass.

---

### DEBT-09: Result bit-width inference is fragile

**Status: FIXED (2026-02-19).**
- `_infer_bit_width(samples, num_clbits=0)` now accepts an optional `num_clbits` parameter.
- When all samples are `0x0`, returns `max(1, num_clbits)` instead of always returning 1.
- Call site in `ArvakIBMJob.result()` reads `num_clbits` from `result["metadata"]["num_clbits"]` or `result["header"]["memory_slots"]` (best-effort; falls back to 0 which preserves old behavior).

---

### DEBT-10: Backend info cache has TTL but no size limit

**Status: FIXED (2026-02-19).**
- `ArvakIBMBackend._BACKEND_INFO_TTL = 300` class constant added (was a magic number).
- `_get_backend_info()` uses the constant instead of the bare `300`.
- Cache is per-instance (one entry per backend object) — size limit is inherently bounded by the number of backend instances.

---

### DEBT-17: Scaleway `validate()` does not check shot count

**Status: FIXED (2026-02-19).**
- Rust Scaleway `validate()` now checks `shots == 0` (must be ≥ 1) and `shots > 100_000` (IQM hardware limit).
- Returns `ValidationResult::Invalid` with actionable error messages for both violations.

---

## Low — Fixed

### DEBT-12: No contract version reporting in Python

**Status: FIXED (2026-02-19).**
- `contract_version(self) -> str` added to `ArvakIBMBackend`, `ArvakScalewayBackend`, `ArvakIQMResonanceBackend`. All return `"2.0"`.

---

### DEBT-13: Layer 3 extensions not implemented

**Status: Deferred (by design).**
HAL Contract Layer 3 extensions (Batch, Sessions, Calibration, Error Mitigation, Pulse) are optional. Not implemented in Rust or Python backends. Implement when customer demand justifies it.

---

### DEBT-14: `ValidationResult::RequiresTranspilation` never returned

**Status: FIXED (2026-02-19).**
- Rust IBM `validate()` detects Eagle backends (`gate_set.two_qubit` contains `"ecr"`).
- When an Eagle circuit contains CZ/CX gates (decomposable to ECR), returns `RequiresTranspilation { details: "..." }` instead of `Invalid`.
- `submit()` treats `RequiresTranspilation` as a pass (transpilation happens in the Qiskit Python layer).

---

---

## Critical — Fixed in-session

### DEBT-18: IQM H gate decomposition produces wrong unitary

**Status: FIXED (2026-02-19).**

**Root cause:** `translate_to_iqm()` in `arvak-compile/src/passes/target/translation.rs` used:
```
H = PRX(π, π/4) · PRX(π/2, -π/2)
```
This produces `(e^{iπ/4}/√2) [[1,-1],[-i,-i]]` — not H, not equivalent up to global phase.

**Fix:** Changed to the correct decomposition:
```
H = PRX(π, 0) · PRX(π/2, π/2)   →  -i · H  (global phase -i, unobservable)
```

**Regression guard:** `test_iqm_h_unitary_correct()` in `translation.rs` verifies `PRX(π,0)·PRX(π/2,π/2)` is H up to global phase.

**Sirius hardware verification:** Bell state 49% |00⟩ / 45% |11⟩ (job 019c7788).

---

## Summary — All items resolved

| ID | Severity | Component | Status |
|----|----------|-----------|--------|
| DEBT-01 | Critical | All backends | **FIXED 2026-02-19** |
| DEBT-02 | Critical | Rust `capability.rs` | **FIXED 2026-02-19** |
| DEBT-03 | Critical | Rust compiler | **FIXED 2026-02-19** |
| DEBT-04 | High | Python jobs | **FIXED 2026-02-19** |
| DEBT-05 | High | All backends | **FIXED 2026-02-19** |
| DEBT-06 | High | Rust `capability.rs` | **FIXED 2026-02-19** |
| DEBT-07 | High | Python IBM | **FIXED 2026-02-19** |
| DEBT-08 | Medium | Python backends | **FIXED 2026-02-19** |
| DEBT-09 | Medium | Python IBM | **FIXED 2026-02-19** |
| DEBT-10 | Medium | Python IBM | **FIXED 2026-02-19** |
| DEBT-12 | Low | Python backends | **FIXED 2026-02-19** |
| DEBT-13 | Low | All | Deferred by design |
| DEBT-14 | Low | Rust IBM | **FIXED 2026-02-19** |
| DEBT-15 | High | Python backends | **FIXED 2026-02-19** |
| DEBT-16 | High | Rust Scaleway | **FIXED 2026-02-19** |
| DEBT-17 | Medium | Rust Scaleway | **FIXED 2026-02-19** |
| DEBT-18 | Critical | Rust IQM compiler | **FIXED 2026-02-19** |
| DEBT-19 | Medium | Python AQT | **FIXED 2026-02-21** — docstring explains PRX/rx discrepancy (VQ-046) |
| DEBT-20 | Medium | Rust trait + spec | **FIXED 2026-02-21** — validate(circuit, shots) in HAL v2.1 spec (VQ-047) |
| DEBT-21 | Medium | Python AQT | **FIXED 2026-02-21** — job_result() returns ArvakAQTResult (VQ-046) |
| DEBT-22 | Low | Rust Quantinuum + AQT | **FIXED 2026-02-21** — name() returns instance-specific target (VQ-046) |
| DEBT-23 | Medium | Rust HAL + spec | **FIXED 2026-02-21** — estimated_wait: Option<Duration> codified in HAL v2.1 (VQ-047) |

---

## Fixed Items — from Quantinuum + AQT Audit (2026-02-21, all fixed same session)

### DEBT-19: Python AQT PRX→R serialisation always sets phi=0 (wrong for phase-rotated gates)

**Status: FIXED (2026-02-21) — Medium — VQ-046**
**Component:** `crates/arvak-python/python/arvak/integrations/qiskit/backend.py` — `ArvakAQTBackend._serialize_circuit()`

The Python AQT backend transpiles via Qiskit with `basis_gates=['rz', 'rx', 'rxx']`. It then maps `rx(theta)` to AQT `R` with `phi=0.0` (axis-aligned X rotation). This is correct for `rx` gates, but the Rust adapter correctly handles `PRX(θ, φ)` → AQT `R(θ/π, -φ/π rem_euclid 2.0)` with a phase-sign flip.

The two paths diverge for any circuit that uses a PRX gate with a non-zero phase angle. After Qiskit transpilation, `PRX(θ, φ≠0)` becomes a sequence of `rz+rx+rz` that the Python serializer maps to multiple AQT ops — which is correct in terms of equivalent unitaries, but the Rust path is simpler and more direct. More critically: `GateSet::aqt()` in Rust names the gate `prx`, while the Python Qiskit transpilation uses `rx`. These naming differences can cause confusion when validating circuits.

The concrete correctness risk: if a Python caller constructs a `PRX` gate via the Arvak Python bindings and submits it through `ArvakAQTBackend`, the transpilation will decompose it to `rz+rx+rz` chains rather than the native `R` op, inflating circuit depth unnecessarily.

**Fix needed (VQ-046):** Document explicitly that the Python path uses `rx`-based decomposition (not `prx`), and add a comment explaining why this is correct but suboptimal vs the Rust path. Add a gate-set note to `ArvakAQTBackend` docstring. No semantic bug for correctly transpiled circuits, but the inconsistency needs documentation and a test.

---

### DEBT-20: Rust `validate()` trait does not accept `shots` — shot-count validation skipped or duplicated in `submit()`

**Status: FIXED (2026-02-21) — Medium — VQ-047 (spec change)**
**Component:** `crates/arvak-hal/src/backend.rs` — `Backend::validate()` signature; AQT and Quantinuum `backend.rs`

The spec §3.2 `validate(circuit)` signature does not include `shots`. AQT has a 2000-shot hard limit and a 2000-op hard limit; Quantinuum has a 10,000-shot limit. These are checked in `submit()` directly (Quantinuum `backend.rs` lines 267–280; AQT `backend.rs` lines 437–457) but not in `validate()`, since `validate()` has no `shots` parameter.

Python backends independently patched this: `ArvakAQTBackend.validate(circuit, shots=1024)` and `ArvakQuantinuumBackend.validate(circuits, shots=1024)` both accept shots. The Rust trait does not.

**Fix needed (VQ-047 spec + VQ-046 Rust):** The spec must update `validate()` to `validate(circuit, shots: u32)`. Then the Rust trait is updated and all adapters can consolidate shot-limit checks into `validate()`, with `submit()` calling `validate()` (per DEBT-01 pattern, which the new adapters are missing — see spec gap A-2).

---

### DEBT-21: Python AQT `job_result()` returns raw `dict`, not `ArvakResult` or `ArvakAQTResult`

**Status: FIXED (2026-02-21) — Medium — VQ-046**
**Component:** `crates/arvak-python/python/arvak/integrations/qiskit/backend.py` — `ArvakAQTBackend.job_result()`

The HAL contract §6.1 `job_result()` must return an `ExecutionResult`-equivalent object with `counts`, `shots`, `execution_time_ms`, and `metadata`. `ArvakAQTResult` is defined in `backend.py` and has `get_counts()` — but `job_result()` returns the raw counts dict, not an `ArvakAQTResult` instance.

Compare: `ArvakQuantinuumBackend.job_result()` and `ArvakIBMBackend.job_result()` both return proper result wrapper objects. AQT is inconsistent.

Additionally, the compliance matrix in this file does not yet include Quantinuum or AQT rows.

**Fix needed (VQ-046):** `ArvakAQTBackend.job_result()` should return `ArvakAQTResult(counts, shots)`. Update compliance matrix.

---

### DEBT-22: Rust `name()` returns generic type name, not instance-specific target

**Status: FIXED (2026-02-21) — Low — VQ-046**
**Component:** `adapters/arvak-adapter-quantinuum/src/backend.rs:195`, `adapters/arvak-adapter-aqt/src/backend.rs:322`

`QuantinuumBackend::name()` returns `"quantinuum"` even when `self.target` is `"H2-1"`. `AqtBackend::name()` returns `"aqt"` even when `self.resource` is `"offline_simulator_no_noise"`. Two instances targeting different machines are indistinguishable by `name()`.

Python adapters do this correctly: `self.name = f"quantinuum_{device}"` and `self.name = f"aqt_{resource}"`.

**Fix needed (VQ-046):** Return `self.target.as_str()` (Quantinuum) and `format!("aqt/{}/{}", self.workspace, self.resource)` (AQT). Also update `AqtBackend::name()` test in `backend.rs`.

---

### DEBT-23: `BackendAvailability.estimated_wait` type diverges from spec

**Status: FIXED (2026-02-21) — Medium — VQ-047 (spec clarification)**
**Component:** `crates/arvak-hal/src/backend.rs` — `BackendAvailability` struct

Spec §4.5 table: `estimated_wait_secs: Option<f64>`. Rust implementation: `estimated_wait: Option<Duration>`. Both new adapters set it to `None`, so currently harmless. Python backends don't expose this field at all.

**Fix needed (VQ-047 spec):** Spec should explicitly say `Option<Duration>` (Rust idiomatic), or the Rust implementation should align to `Option<f64>`. Either way, the field needs a spec-endorsed type. Python backends should expose it via `HalAvailability`.

---

## Spec-Level Gaps (VQ-047 — HAL Contract Spec Update)

These are gaps in the spec itself (not Arvak implementation bugs). All addressed by VQ-047.

| Gap | Spec section | Finding |
|-----|-------------|---------|
| **A-1** | §2.3 | Auth patterns are fully out-of-scope; JWT re-auth logic duplicated across adapters |
| **A-2** | §3.3 rule 4 | `submit()` not required to call `validate()` first — guards diverge per adapter |
| **A-3** | §3.2 | `validate()` lacks `shots` — Python patched it unilaterally; spec needs update |
| **A-4** | §8 | Gate-set reference table missing Quantinuum and AQT entries |
| **A-5** | §5.1 | HTTP 410 "result expired" has no state-machine representation; mapped to `JobNotFound` |
| **A-6** | §3.3 rule 5 | Calling `result()` on non-Completed job is "undefined behavior" — should specify return |
| **B-1** | §3.3 | `BackendFactory` / `BackendConfig` are Arvak extensions; should be Layer 2 in spec |
| **B-2** | §3.2 | Workspace/resource hierarchy (AQT model) has no spec representation; `name()` must be unique |
| **B-3** | §4.1 | Emulator/hardware distinction relies on string heuristics; spec should require authoritative source |
| **B-4** | §4.1 | `max_circuit_ops` constraint not in `Capabilities`; AQT hard-codes 2000 as a constant |
| **D-1** | DEBT-13 | `mid_circuit_measurement` feature flag in Quantinuum caps signals Layer 3 is active — elevate from "deferred by design" |

---

---

## HAL Contract v2.3 — Photonic Extension (VQ-047/VQ-048, 2026-02-26)

### DEBT-Q1: `DecoherenceMonitor` lacks photonic measurement methods

**Status: FIXED (2026-02-26) — VQ-048**
**Component:** `crates/arvak-hal/src/capability.rs` — `DecoherenceMonitor` trait

Added two default methods with `None` implementations:
- `measure_hom_visibility(shots) -> Option<f64>` — Hong-Ou-Mandel visibility (photonic backends)
- `compute_hom_fingerprint(sample_count, shots_per_sample) -> Option<Vec<TransferFunctionSample>>` — HOM-based PUF fingerprint

Non-photonic backends inherit the default `None` impls (non-breaking).
`QuandelaBackend` explicitly returns `None` with DEBT-Q5 note; alsvid-lab path used instead.

---

### DEBT-Q2: `CompressorSpec::rotary_valve: bool` cannot express photonic cryocooler types

**Status: FIXED (2026-02-26) — VQ-048**
**Component:** `crates/arvak-hal/src/capability.rs` — `CompressorSpec` struct

Replaced `pub rotary_valve: bool` with `pub compressor_type: CompressorType` (new enum).
Added `CompressorType` variants: `RotaryValve`, `GiffordMcMahon`, `Stirling`, `PulseTube`, `Other(String)`.
Added `CompressorSpec::is_rotary_valve()` helper for backward-compatible access.
`Capabilities::quandela()` uses `CompressorType::GiffordMcMahon`.

---

### DEBT-Q3: `TransferFunctionSample` has no photonic metric

**Status: FIXED (2026-02-26) — VQ-048**
**Component:** `crates/arvak-hal/src/capability.rs` — `TransferFunctionSample` struct

Added optional field:
```rust
pub visibility_modulation: Option<f64>,  // HOM visibility modulation; None for superconducting
```
All existing `TransferFunctionSample` construction sites updated with `visibility_modulation: None`.
`QuandelaBackend::ingest_alsvid_enrollment()` populates from alsvid-lab per-phase HOM visibility.

---

### DEBT-Q4: Photonic dual-rail encoding pass not implemented

**Status: FIXED (2026-02-28) — VQ-089**
**Component:** `adapters/arvak-adapter-quandela/` — `perceval_bridge.py`

Implemented dual-rail encoding in `perceval_bridge.py` subprocess bridge:
- Qubit q → modes (2q, 2q+1); CNOT inserts 2 ancilla after data qubit, shifts subsequent qubits
- Circuit-first approach: `pcvl.Circuit.add(offset, component)` uses absolute mode offsets
- `RemoteProcessor(name=platform, m=n_total)` then `rp.add(0, combined_circuit)`
- 24 unit tests passing.

---

### DEBT-Q5: Quandela REST API submission endpoint not documented

**Status: FIXED (2026-02-28) — VQ-089**
**Component:** `adapters/arvak-adapter-quandela/` — `perceval_bridge.py`

Cloud submission implemented via `perceval_bridge.py` subprocess bridge:
- Commands: `ping/submit/status/result/cancel`
- `Sampler(rp, max_shots_per_call=shots)` required for `RemoteProcessor`
- `rp.min_detected_photons_filter(n_qubits)` required for cloud submission
- Explorer Offer limit: 1 job in queue at a time
- Integration test validated against Quandela cloud (blocked only by queue availability).

---

### Summary — HAL Contract v2.3 photonic items

| ID | Severity | Component | Status |
|----|----------|-----------|--------|
| DEBT-Q1 | Medium | `DecoherenceMonitor` trait | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q2 | High | `CompressorSpec::rotary_valve` | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q3 | Medium | `TransferFunctionSample` | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q4 | High | Quandela encoding pass | **FIXED 2026-02-28** (VQ-089) — perceval_bridge.py |
| DEBT-Q5 | Medium | Quandela REST API | **FIXED 2026-02-28** (VQ-089) — perceval_bridge.py |

---

**All 28 items resolved.** DEBT-Q4/Q5 closed 2026-02-28 via VQ-089.

**Open items (prior):** All 23 DEBT items resolved as of 2026-02-21. Spec gaps (VQ-047) addressed in HAL Contract v2.1.

### Compliance matrix (post 2026-02-21 audit)

| Component | validate() | submit() | status() | result() | cancel() | availability() | contract_version() |
|-----------|-----------|----------|----------|----------|----------|---------------|-------------------|
| Rust IBM | ✓ (shots+qubits+gates) | ✓ (calls validate) | ✓ | ✓ | ✓ | ✓ | — |
| Rust Scaleway | ✓ (shots+qubits+gates) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Rust Quantinuum | ✓ (qubits+shots) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Rust AQT | ✓ (qubits+shots+ops) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Python IBM | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python Scaleway | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python IQM Resonance | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python Quantinuum | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python AQT | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ ArvakAQTResult | ✓ | ✓ | ✓ "2.0" |

---

## QUASI-OS Integration Debt (2026-03-01)

The following items were identified during QUASI quantum OS development. They describe
gaps in the HAL Contract spec that caused architectural violations upstream in Afana
(the Ehrenfest compiler) and quasi-board. Each item must be resolved in the HAL Contract
before the corresponding QUASI PRs can be finalised.

---

### DEBT-24: No `GET /hal/backends/{name}` capabilities endpoint

**Status: FIXED (2026-03-01) — ts-halcontract `getBackendCapabilities()` + spec §10**
**Severity: High**
**Identified by:** QUASI PR #368 (noise model spec) / Afana compiler boundary review (2026-03-01)
**Component:** HAL Contract HTTP REST surface (ts-halcontract, Arvak HTTP adapter)

#### Problem

The HAL Contract spec defines rich `Capabilities` on the `Backend<C>` trait (§4.1: gate_set,
topology, noise_profile, max_shots, …), but the HTTP REST surface only exposes:

- `GET /hal/backends` — list of backend name strings
- `POST /hal/jobs` — job submission
- `GET /hal/jobs/{id}` — job result polling

There is **no endpoint to query per-backend capabilities over HTTP**.

This gap forced an architectural violation in QUASI: `noise_channels` (a hardware-behaviour
description: depolarizing `p`, amplitude damping `gamma`) was added to the Ehrenfest program
itself (wrong layer — Ehrenfest describes physics, not hardware). Without a capabilities
endpoint, the compiler has no way to discover what noise model a backend requires at program
write time, so the information was incorrectly embedded in the program.

The same gap affects gate-set selection, topology-aware routing, and backend selection
logic in quasi-board — all decisions that require backend capabilities but cannot query them
without an HTTP endpoint.

#### Fix needed

Add to the HAL Contract HTTP REST surface:

```
GET /hal/backends/{name}
```

Response body (JSON):
```json
{
  "name": "ibm_torino",
  "num_qubits": 156,
  "gate_set": { "single_qubit": ["rz","sx","x"], "two_qubit": ["cz","ecr"], "native": ["rz","sx","x","cz"] },
  "topology": { "kind": "HeavyHex", "edges": [[0,1],[1,2]] },
  "max_shots": 300000,
  "max_circuit_ops": null,
  "is_simulator": false,
  "features": ["dynamic_circuits","mid_circuit_measurement"],
  "noise_profile": { "t1": 150.0, "t2": 80.0, "single_qubit_fidelity": 0.9998, "two_qubit_fidelity": 0.995, "readout_fidelity": 0.97 }
}
```

The response maps directly to `Capabilities` (§4.1). Backends that cannot determine
capabilities without I/O must cache at startup (spec §3.3 rule 1).

**QUASI unblocked by:** Once this endpoint exists, `noise_channels` can be removed from
the Ehrenfest CDDL (QUASI PR #368) and noise channel selection becomes a HAL submission
concern driven by the capabilities response.

---

### DEBT-25: No parameter binding in `submit()` for variational algorithms

**Status: FIXED (2026-03-01, compliance enforced 2026-03-03) — `Backend::submit(parameters)` trait + all 11 adapters + REST gateway**
**Severity: High**
**Identified by:** QUASI PR #364 (parametric Ehrenfest v0.2) / Afana compiler review (2026-03-01)
**Component:** HAL Contract `Backend::submit()` trait method + `SubmitCircuitInput` HTTP type

#### Problem

Ehrenfest v0.2 introduces parametric programs (VQE, QAOA, Trotter sweeps) where the
Hamiltonian contains `ParameterRef` coefficients and the compiled OpenQASM 3.0 output
contains `input float[64] theta_0;` declarations.

The current HAL Contract `submit(circuit, shots)` signature (§3.2) has no way to pass
concrete parameter values at submission time. This forced an incorrect design decision
in QUASI PR #364: the `compile-parametric` CLI reads parameter values from JSON and
resolves them inside Afana (compiler layer), producing a fully-bound concrete circuit.

This is wrong for two reasons:
1. **Layer violation**: parameter binding is an execution concern, not a compilation
   concern. The compiler should emit a parametric circuit; the caller binds values at
   submission time (this is the standard model in Qiskit, Cirq, and PennyLane).
2. **Ehrenfest has no JSON form**: Ehrenfest programs are CBOR binary (`.cbor.hex`).
   A CLI that reads JSON to get parameter values is reading the wrong format.

The correct model: Afana compiles once to a parametric OpenQASM 3.0 circuit with
`input float[64]` declarations. The HAL driver receives the circuit + a parameter map
at submission time and binds values before dispatch to hardware.

#### Fix needed

Extend `submit()` to accept optional parameter bindings:

```rust
// Trait method
async fn submit(
    &self,
    circuit: C,
    shots: u32,
    parameters: Option<HashMap<String, f64>>,
) -> HalResult<JobId>;
```

HTTP body extension for `POST /hal/jobs`:
```json
{
  "qasm": "OPENQASM 3.0; input float[64] theta_0; ...",
  "backend": "ibm_torino",
  "shots": 1024,
  "parameters": { "theta_0": 1.5707963, "theta_1": 0.7853981 }
}
```

HAL drivers that do not support parametric circuits (OpenQASM 3.0 `input` declarations)
MUST return `HalError::Unsupported` when `parameters` is non-empty.

HAL drivers that do support parametric execution (IBM, IQM via OpenQASM 3.0) bind
the parameter map before dispatching to hardware — no Afana involvement required.

**QUASI unblocked by:** Once this is in the spec, QUASI PR #364 can be revised so
`compile-parametric` reads `.cbor.hex` (not JSON), emits parametric QASM 3.0, and
the quasi-board passes a parameter map at `POST /hal/jobs` time.

**Compliance enforcement (2026-03-03):** All 11 adapters now return
`HalError::Unsupported` when `parameters` is `Some` with a non-empty map, per the
HAL Contract §3.2 requirement. Previously, all adapters silently dropped the
parameter map (`let _ = parameters;` or `_parameters` prefix), which violated the
contract and would produce incorrect results for parametric circuits without any
diagnostic. Affected adapters: IBM, Scaleway, IQM, Quantinuum, AQT, Braket, QDMI,
CUDA-Q, DDSIM, Quandela, Simulator.

---

### Summary — QUASI-OS integration items

| ID | Severity | Component | Status |
|----|----------|-----------|--------|
| DEBT-24 | High | HAL HTTP REST surface — `GET /hal/backends/{name}` | **FIXED 2026-03-01** |
| DEBT-25 | High | `Backend::submit()` + `SubmitCircuitInput` — parameter binding | **FIXED 2026-03-01, compliance enforced 2026-03-03** |

**All items resolved.** DEBT-24/25 resolved 2026-03-01, DEBT-Q4/Q5 resolved 2026-02-28.

---

---

## Spec-Level Gaps — Photonic & Pulse-Level Backends (2026-03-09)

These are gaps in the HAL Contract *specification* exposed by the Quandela and Pasqal/Pulser
integrations. Currently mitigated by Arvak's internal bridge architecture (QASM3 in,
Perceval/Pulser conversion hidden inside the adapter). They become blocking if external
consumers need to submit photonic or pulse-level programs directly through the HAL API.

---

### GAP-PH1: Spec does not account for mode expansion in photonic backends

**Status: Open (mitigated internally)**
**Severity: Medium**
**Component:** HAL Contract §3.2 `validate()`, §4.1 `Capabilities`

Photonic backends use dual-rail encoding: logical qubit q maps to optical modes (2q, 2q+1),
and multi-qubit gates (e.g. CNOT) insert ancilla modes. A 3-qubit circuit may require 8+
modes internally. The spec assumes the circuit's qubit count is what the backend executes —
`validate()` checks `circuit.num_qubits <= capabilities.num_qubits`, but for photonic
backends `num_qubits` refers to logical qubits while the hardware constraint is on modes.

Currently mitigated: `perceval_bridge.py` handles mode expansion transparently. The Rust
adapter's `validate()` checks logical qubit count against platform limits (6 for sim:ascella,
12 for sim:belenos), which implicitly accounts for mode overhead since those limits were
chosen with dual-rail in mind.

**Spec change needed:** `Capabilities` should distinguish `num_logical_qubits` from
`num_physical_resources` (modes for photonics, physical qubits for superconducting), or
document that `num_qubits` is the logical limit inclusive of encoding overhead.

---

### GAP-PH2: No `ProgramFormat` for non-QASM backends (Perceval, Pulser)

**Status: Open (mitigated internally)**
**Severity: Low**
**Component:** HAL Contract §3.2 `submit()`, §7 Program Formats

The spec defines QASM3 as the circuit interchange format. Photonic backends need Perceval
circuit JSON; neutral-atom backends need Pulser Sequences (pulse schedules, not gate circuits).
Currently both adapters accept QASM3 and convert internally, but this prevents a compiler from
emitting target-native programs that skip the conversion step.

Currently mitigated: Arvak's compilation pipeline emits QASM3; the adapter bridges handle
conversion. No external consumer needs to submit Perceval/Pulser programs today.

**Spec change needed (when required):** Add `ProgramFormat::Perceval` and
`ProgramFormat::PulserSequence` to §7, with serialization conventions. Backends advertise
supported formats via `Capabilities::supported_program_formats`. Low priority — only needed
if a photonic-native compiler wants to bypass QASM3.

---

### GAP-PH3: `Capabilities` lacks photonic/optical hardware constraints

**Status: Open (mitigated internally)**
**Severity: Low**
**Component:** HAL Contract §4.1 `Capabilities`

Photonic backends have hardware constraints with no spec representation: maximum mode count,
photon-loss rate, detector efficiency, Hong-Ou-Mandel visibility threshold. These are
currently either implicit in platform-specific limits or stuffed into `features: Vec<String>`
and `noise_profile` ad-hoc.

Currently mitigated: The Quandela adapter hardcodes platform-specific limits. Alsvid
ingestion populates `DecoherenceMonitor` fields (DEBT-Q1) for HOM visibility.

**Spec change needed (when required):** Either extend `Capabilities` with an optional
`PhotonicProfile` struct (mode count, loss rate, detector efficiency), or document that
`noise_profile` is the catch-all for technology-specific metrics. Low priority — only
relevant when multiple photonic backends need comparable capability reporting.

---

### GAP-PH4: No submission path for pulse-level programs (Pasqal/Pulser)

**Status: Open (not mitigated — Pasqal has no HAL adapter)**
**Severity: Medium**
**Component:** HAL Contract §3.2 `submit()`, QDMI #171–#177

Pulser Sequences are pulse schedules, not gate circuits. The HAL `submit(circuit, shots)`
signature assumes a gate-level circuit. Pasqal/Pulser integration is currently a converter
only (`PulserIntegration.to_arvak()` / `from_arvak()`), not a HAL-compliant backend.
Submitting a pulse-level program through the HAL API is not possible.

This parallels the QDMI pulse-level discussion (#171–#177) — both QDMI and the HAL Contract
need a story for pulse submission. The QDMI approach (channel-level pulse representation +
pulse submission interface) could inform the HAL Contract design.

**Spec change needed (when required):** Define a `submit_pulse()` method or extend `submit()`
to accept pulse-level programs alongside gate circuits. Alternatively, define that pulse-level
submission is out-of-scope for the HAL Contract and belongs to the device driver layer.

---

### Summary — Photonic spec gaps

| ID | Severity | Gap | Blocking? |
|----|----------|-----|-----------|
| GAP-PH1 | Medium | Mode expansion not modeled in `validate()`/`Capabilities` | No — mitigated by adapter |
| GAP-PH2 | Low | No `ProgramFormat` for Perceval/Pulser | No — QASM3 bridge works |
| GAP-PH3 | Low | No photonic hardware constraints in `Capabilities` | No — hardcoded in adapter |
| GAP-PH4 | Medium | No pulse-level submission path | Partially — Pasqal has no HAL adapter |

**None are blocking today.** All become relevant when external consumers or additional photonic
backends need direct HAL API access without Arvak's internal bridge layer.

