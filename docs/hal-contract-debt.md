# HAL Contract Technical Debt Register

Deviations between the HAL Contract v2 specification (`crates/arvak-hal/src/backend.rs`)
and the current IBM and Scaleway/IQM backend implementations (Rust adapters + Python backends).

Audited: 2026-02-18 (updated with Scaleway/IQM findings)
Updated: 2026-02-19 — ALL 12 items resolved. Full clearance commit includes DEBT-01 through DEBT-18.
Updated: 2026-02-21 — 5 new items (DEBT-19–DEBT-23) from Quantinuum + AQT adapter audit.
Updated: 2026-02-21 — **DEBT-19/21/22 fixed (VQ-046); DEBT-20/23 addressed in spec (VQ-047, HAL v2.1). All 23 items resolved.**
Updated: 2026-02-26 — DEBT-Q1–Q3 fixed (HAL Contract v2.3 photonic extension, VQ-048). DEBT-Q4/Q5 remain Open. Quandela adapter added (VQ-047).

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

**Status: Open**
**Component:** `adapters/arvak-adapter-quandela/src/backend.rs`

`QuandelaBackend::submit()` returns `HalError::Backend("DEBT-Q4: photonic encoding pass not implemented")`.
`validate()` returns `RequiresTranspilation` to signal this to orchestrators.
Blocked on: Rust Perceval client or QASM3 → perceval-interop transpiler.

---

### DEBT-Q5: Quandela REST API submission endpoint not documented

**Status: Open**
**Component:** `adapters/arvak-adapter-quandela/src/api.rs`

`QuandelaClient::ping()` is a stub that checks for non-empty key only. Real availability check and circuit submission require the Quandela cloud API endpoint, which is not yet publicly documented. `availability()` returns `always_available()` when key is non-empty (optimistic stub).

---

### Summary — HAL Contract v2.3 photonic items

| ID | Severity | Component | Status |
|----|----------|-----------|--------|
| DEBT-Q1 | Medium | `DecoherenceMonitor` trait | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q2 | High | `CompressorSpec::rotary_valve` | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q3 | Medium | `TransferFunctionSample` | **FIXED 2026-02-26** (VQ-048) |
| DEBT-Q4 | High | Quandela encoding pass | **Open** — photonic encoding blocked on Perceval client |
| DEBT-Q5 | Medium | Quandela REST API | **Open** — endpoint TBD |

---

**Open items: 2** (DEBT-Q4, DEBT-Q5). All other 26 items resolved.

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
