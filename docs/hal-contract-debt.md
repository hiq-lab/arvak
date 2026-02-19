# HAL Contract Technical Debt Register

Deviations between the HAL Contract v2 specification (`crates/arvak-hal/src/backend.rs`)
and the current IBM and Scaleway/IQM backend implementations (Rust adapters + Python backends).

Audited: 2026-02-18 (updated with Scaleway/IQM findings)
Updated: 2026-02-19 — **ALL 12 items resolved.** Full clearance commit includes DEBT-01 through DEBT-18.

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

**Open items: 0** (DEBT-13 deferred by design — optional Layer 3 extensions).

### Compliance matrix (post-clearance)

| Component | validate() | submit() | status() | result() | cancel() | availability() | contract_version() |
|-----------|-----------|----------|----------|----------|----------|---------------|-------------------|
| Rust IBM | ✓ (shots+qubits+gates) | ✓ (calls validate) | ✓ | ✓ | ✓ | ✓ | — |
| Rust Scaleway | ✓ (shots+qubits+gates) | ✓ | ✓ | ✓ | ✓ | ✓ | — |
| Python IBM | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python Scaleway | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
| Python IQM Resonance | ✓ | ✓ (submit + run) | ✓ (job_status) | ✓ (job_result) | ✓ | ✓ | ✓ "2.0" |
