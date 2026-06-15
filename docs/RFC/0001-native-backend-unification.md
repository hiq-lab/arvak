# RFC 0001 — Native Backend Unification

| | |
|---|---|
| **Status** | Accepted (2026-05-23) |
| **Author** | Daniel Hinderink |
| **Created** | 2026-05-23 |
| **Branch target** | unify-native-backends |
| **Affects** | `arvak-python`, `arvak-hal`, all `arvak-adapter-*` crates |

## Decisions on open questions (settled 2026-05-23)

1. **Single class for all backends.** `ArvakBackend` only — capabilities come from `capabilities()` at the Rust side, not hardcoded per vendor.
2. **No job persistence across processes for v1.** Helper API can be added if anyone asks.
3. **All 12 adapters compiled into the default wheel.** `pip install arvak` works without extras for every supported backend.

## Pending decisions (BLOCKING Phase 2)

Surfaced during the Phase-1 implementation audit on 2026-05-27. See
**Phase 1.5** below for full rationale and concrete options.

- **A1.** How does `PyJobHandle.result()` handle jobs longer than the
  current HAL `wait()` 5-minute timeout? *Recommendation: PyO3-level
  polling loop with optional `timeout=None` argument.*
- **A2.** Does `ArvakBackend.run()` block until results are in (current
  Phase-1 behaviour, fine for sim, breaks for cloud), or return a
  deferred handle (Qiskit `JobV1` semantics, mandatory for cloud)?
  *Recommendation: deferred, with `ArvakJob` wrapping a list of
  `PyJobHandle`s for multi-circuit batches.*

Both fixes land inside Phase 2 as the first commits, before the IQM
adapter is wired in.

## Summary

Replace the six vendor-specific Python backend classes in
`arvak.integrations.qiskit.backend` (~3,000 lines of code, six vendor
SDK dependencies) with a single `ArvakBackend` Python class that
delegates to the native Rust `Backend` trait implementations via PyO3.
This makes the architectural claim "Arvak is the Rust quantum OS, Python
is a thin façade" actually true, instead of partially true.

## Motivation

### Current state

Arvak has 12 native Rust backend adapters (`adapters/arvak-adapter-*`)
implementing `arvak_hal::Backend`. The CLI and gRPC paths use them
directly. **The Python `ArvakProvider` does not.** Instead, six parallel
Python classes re-implement vendor submission, polling, and result
parsing on top of vendor SDKs:

| Python class | File:line | Lines | Uses (Python SDK) | Native Rust adapter exists? |
|---|---|---|---|---|
| `ArvakSimulatorBackend` | `backend.py:388` | ~80 | (none — runs `arvak.run_sim` via PyO3) | `arvak-adapter-sim` |
| `ArvakIBMBackend` | `backend.py:465` | ~370 | `requests` against IBM Cloud REST | `arvak-adapter-ibm` |
| `ArvakIQMResonanceBackend` | `backend.py:1273` | ~230 | `iqm-client[qiskit]==33.0.3` | `arvak-adapter-iqm` |
| `ArvakScalewayBackend` | `backend.py:989` | ~280 | Scaleway QaaS HTTP | `arvak-adapter-scaleway` |
| `ArvakQuantinuumBackend` | `backend.py:1733` | ~270 | Quantinuum REST | `arvak-adapter-quantinuum` |
| `ArvakAQTBackend` | `backend.py:2422` | ~300 | AQT Arnica HTTP | `arvak-adapter-aqt` |
| `ArvakIonQBackend` | `backend.py:2814` | ~370 | IonQ Cloud REST | `arvak-adapter-ionq` |

The only Python class that *does* use the native Rust path is
`ArvakSimulatorBackend`, and even it goes through `arvak.run_sim` (a
`#[pyfunction]`) rather than through `arvak-adapter-sim` as a `Backend`
trait impl.

Concretely: today, calling `provider.get_backend('iqm_garnet').run(qc)`
imports `iqm-client[qiskit]==33.0.3`, transpiles via `qiskit-on-iqm`,
and submits through the IQM SDK — even though
`adapters/arvak-adapter-iqm/src/api.rs` already implements the IQM
Resonance REST API in 435 lines of `reqwest` against the same endpoint.

### Why this is wrong

1. **Two sources of truth per vendor.** Bug fixes, retry logic, error
   taxonomy, validation rules, HAL contract debt (DEBT-01, DEBT-05,
   DEBT-15) live in both places and drift.
2. **The Python path bypasses Arvak's value proposition.** Users who
   import `arvak` for "compile + route + submit through the Rust quantum
   OS" get, for IQM, *Qiskit + iqm-client + arvak-compiler-as-helper*
   instead.
3. **Dependency surface.** `pyproject.toml` extras pull
   `iqm-client[qiskit]`, `qiskit-ibm-runtime`, `pytket-quantinuum`,
   `aqt-arnica`, etc. — each pinned to a specific version range that
   constrains user environments.
4. **Adding a new vendor doubles the work.** Today every vendor needs a
   Rust adapter *and* a Python class. The architecture invites this.

### Why now

- Qiskit 2.4 migration just hit one of these vendor SDKs (`mqt-bench`
  needed PR #895 for Qiskit 2.4 compat — same pattern will hit every
  vendor SDK we bind to).
- The HAL contract v2 cleanup (DEBT-01 through DEBT-25) just landed.
  The Rust `Backend` trait is now stable enough to be the canonical
  surface.
- 12 native adapters exist and pass their tests. The Rust side is ready.

## Goals

1. One implementation per vendor, in Rust, exposed to Python through
   PyO3.
2. Single Python class — `arvak.ArvakBackend` — works for any backend
   the Rust side knows about.
3. Provider discovery driven by a Rust `BackendRegistry`, not by
   hard-coded sets in Python.
4. Remove vendor SDK dependencies from `arvak-python/pyproject.toml`
   extras.
5. Backward-compatible during migration: existing
   `provider.get_backend('iqm_garnet').run(qc)` keeps working while
   the implementation underneath swaps.

## Non-goals

- **Killing the `ArvakProvider` Qiskit-compat surface.** Users who write
  Qiskit-style `provider.get_backend(...).run(circuit)` should keep
  working forever. Only the *implementation* changes.
- **Adding new vendor support.** This RFC is consolidation, not
  expansion.
- **Touching the compiler pipeline.** `arvak.compile`,
  `BasisGates.iqm()`, `CouplingMap.*` are unchanged.
- **Replacing the QASM3 ingress format.** Python passes QASM3 strings to
  Rust; the Rust adapter takes `arvak_ir::Circuit`. Conversion happens
  once at the boundary.

## Proposed architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Python                                                          │
│  arvak.integrations.qiskit                                       │
│    ArvakProvider                                                 │
│      .get_backend(name) → ArvakBackend                           │
│                                                                  │
│    ArvakBackend  (Qiskit-compat duck-typed class)                │
│      .name, .num_qubits, .basis_gates, .coupling_map             │
│      .run(qc, shots) → ArvakJob                                  │
│      .availability(), .validate(), .submit(), .status(), ...     │
│                                                                  │
│    ArvakJob                                                      │
│      .job_id(), .status(), .result()                             │
└────────────────────────────┬─────────────────────────────────────┘
                             │ PyO3
┌────────────────────────────▼─────────────────────────────────────┐
│  arvak-python (Rust, PyO3)                                       │
│    #[pyclass] PyBackend     ← wraps Box<dyn arvak_hal::Backend>  │
│    #[pyclass] PyJobHandle   ← wraps (backend_ref, JobId)         │
│    #[pyclass] PyCapabilities, PyJobStatus, PyExecutionResult     │
│    #[pyfunction] backend_for(name: &str) -> PyBackend            │
│    #[pyfunction] list_backends() -> Vec<String>                  │
│                                                                  │
│    BackendRegistry — single-process registry, lazy construction  │
└────────────────────────────┬─────────────────────────────────────┘
                             │ Rust trait dispatch
┌────────────────────────────▼─────────────────────────────────────┐
│  arvak-hal::Backend                                              │
│    name, capabilities, availability, validate,                   │
│    submit, status, result, cancel, wait                          │
└────────────────────────────┬─────────────────────────────────────┘
                             │ implements
       ┌─────────────────────┼─────────────────────┐
       ▼                     ▼                     ▼
  adapter-ibm          adapter-iqm            adapter-quantinuum  ... (12 total)
```

### PyO3 surface

A single `PyBackend` wraps `Box<dyn arvak_hal::Backend + Send + Sync>`.
Methods mirror the trait, with these translations:

| HAL trait method | Python method | Returns |
|---|---|---|
| `name()` | `.name` (property) | `str` |
| `capabilities()` | `.capabilities()` | `PyCapabilities` |
| `availability().await` | `.availability()` | `PyAvailability` |
| `validate(c).await` | `.validate(qasm)` | `PyValidationResult` |
| `submit(c, shots, params).await` | `.submit(qasm, shots, params)` | `PyJobHandle` |
| `status(id).await` | `.status(job_id)` | `PyJobStatus` (enum) |
| `result(id).await` | `.result(job_id)` | `PyExecutionResult` |
| `cancel(id).await` | `.cancel(job_id)` | `None` |
| `wait(id).await` | `.wait(job_id, timeout=300)` | `PyExecutionResult` |

`PyJobHandle` holds a strong reference to its `PyBackend` so the
Python-side job survives the backend going out of scope between
`submit()` and `result()`.

### Async strategy

The HAL `Backend` trait is `async_trait`. Python callers expect
synchronous blocking semantics from `backend.run(qc).result()`
(matching Qiskit's `JobV1`). Two options:

1. **Sync-over-async** (recommended): each `#[pymethod]` blocks on the
   tokio runtime via `pyo3_async_runtimes::tokio::get_runtime().block_on(...)`.
   The runtime is created once at module init. Python users see
   ordinary blocking methods.
2. **Native async coroutines**: methods return Python `awaitable`s via
   `pyo3-async-runtimes`. Lets advanced users do `await
   backend.submit(...)`. Strictly more flexible but breaks the Qiskit
   `JobV1` compat contract.

**Recommendation: Option 1 for v1**, with the door open to add an
`asyncio_*` variant later if anyone asks. Rationale: every existing
caller in our codebase (and every Qiskit user) is sync. We can add
async without breaking sync; we can't easily go the other way.

The tokio runtime is shared with every other PyO3 entry point — we
already have one for `arvak.run_sim`. The `pyo3-async-runtimes` crate
handles GIL release during `block_on` correctly.

### Backend registry

Each adapter crate has a Cargo feature in `arvak-python`:

```toml
# crates/arvak-python/Cargo.toml
[features]
default       = ["adapter-sim"]
adapter-sim   = ["arvak-adapter-sim"]
adapter-ibm   = ["arvak-adapter-ibm"]
adapter-iqm   = ["arvak-adapter-iqm"]
adapter-aqt   = ["arvak-adapter-aqt"]
adapter-ionq  = ["arvak-adapter-ionq"]
adapter-quantinuum = ["arvak-adapter-quantinuum"]
adapter-scaleway   = ["arvak-adapter-scaleway"]
adapter-braket     = ["arvak-adapter-braket"]
adapter-quandela   = ["arvak-adapter-quandela"]
adapter-ddsim      = ["arvak-adapter-ddsim"]
adapter-cudaq      = ["arvak-adapter-cudaq"]
all-adapters       = ["adapter-sim", "adapter-ibm", "adapter-iqm", ...]
```

`BackendRegistry::get(name)` matches on the prefix:

```rust
match name {
    "sim" => Ok(Box::new(SimBackend::new()?)),
    n if n.starts_with("ibm_")        => Ok(Box::new(IbmBackend::from_env(n)?)),
    n if n.starts_with("iqm_")        => Ok(Box::new(IqmBackend::from_env(n)?)),
    n if n.starts_with("quantinuum_") => Ok(Box::new(QuantinuumBackend::from_env(n)?)),
    n if n.starts_with("aqt_")        => Ok(Box::new(AqtBackend::from_env(n)?)),
    n if n.starts_with("ionq_")       => Ok(Box::new(IonQBackend::from_env(n)?)),
    n if n.starts_with("scaleway_")   => Ok(Box::new(ScalewayBackend::from_env(n)?)),
    _ => Err(HalError::UnknownBackend(name.into())),
}
```

Each branch is `#[cfg(feature = "adapter-X")]`. Default wheel ships
with `adapter-sim` only; `pip install arvak[hardware]` pulls in all
adapters via the `all-adapters` cargo feature.

### Circuit ingress

The HAL `submit()` takes `&arvak_ir::Circuit`. PyO3 receives a QASM3
string (the existing pattern from `qiskit_to_arvak`) and converts:

```rust
#[pymethods]
impl PyBackend {
    fn submit(&self, qasm: &str, shots: u32, parameters: Option<HashMap<String, f64>>) -> PyResult<PyJobHandle> {
        let circuit = arvak_ir::from_qasm(qasm)?;
        let rt = pyo3_async_runtimes::tokio::get_runtime();
        let job_id = rt.block_on(self.inner.submit(&circuit, shots, parameters.as_ref()))?;
        Ok(PyJobHandle { backend: self.clone_arc(), job_id })
    }
}
```

This preserves the QASM3-as-interchange invariant we already document
in `docs/hal-contract.md`.

### Error mapping

Map `arvak_hal::HalError` variants to the existing Python exception
hierarchy:

| HalError variant | Python exception |
|---|---|
| `Validation(_)` | `ArvakValidationError` |
| `BackendUnavailable(_)` | `ArvakBackendUnavailableError` |
| `Authentication(_)` | `ArvakAuthenticationError` |
| `Submission(_)` | `ArvakSubmissionError` |
| `JobFailed(_)` | `ArvakJobError` |
| `JobCancelled` | `ArvakJobCancelledError` |
| `Timeout(_)` | `ArvakTimeoutError` |
| `Unsupported(_)` | `NotImplementedError` |
| everything else | `ArvakError` |

The exception classes already exist (`backend.py:24–53`) and stay.

## Migration plan

### Phase 0 (this RFC)

Decision document. No code. Output: this file, reviewed and `Status:
Accepted`.

### Phase 1 — Pilot: native simulator (1–2 days)

1. Add `PyBackend`, `PyJobHandle`, `PyCapabilities`,
   `PyExecutionResult`, `PyValidationResult`, `PyJobStatus`,
   `PyAvailability` in `crates/arvak-python/src/backend.rs`.
2. Wire `arvak-adapter-sim` as a default-feature dep.
3. Add `backend_for(name)` and `list_backends()` `#[pyfunction]`s.
4. Add `ArvakBackend` Python class in
   `crates/arvak-python/python/arvak/integrations/qiskit/backend.py`
   delegating to the new PyO3 surface.
5. Make `provider.get_backend('sim')` return the new class instead of
   `ArvakSimulatorBackend`. Mark the old class deprecated but keep it
   importable.
6. Test parity: existing test suite for `ArvakSimulatorBackend` must
   pass against the new implementation.

**Validates:** PyO3 async runtime, type bindings, error mapping. No
external service. Reversible by reverting one PR.

**Status: Done (2026-05-23).** 15/15 Phase-1 smoke tests pass plus the
five follow-up tests (B1/C1/E1/E2). Implementation audit on
2026-05-27 surfaced two cloud-vendor design questions (A1, A2 below)
that MUST be decided before Phase 2 begins.

### Phase 1.5 — Cloud-vendor design questions (BLOCKING for Phase 2)

The Phase 1 sim pilot validated the basic PyO3 bridge end-to-end, but
sim returns results instantly. Two assumptions that hold for sim
**break the moment a cloud backend with a real queue is wired in**.
Both need an explicit decision before `arvak-adapter-iqm` is wired in
Phase 2; otherwise we encode a wrong default that's expensive to undo
once every vendor copies the same pattern.

#### A1 — `wait()` 5-minute timeout

**Current state:** `PyJobHandle.result()` delegates to
`arvak_hal::Backend::wait()`, which has a default implementation that
polls every 500 ms for a maximum of 5 minutes
(`crates/arvak-hal/src/backend.rs:186–207`). After 5 minutes it
returns `HalError::Timeout`, which the PyO3 layer surfaces as
`PyTimeoutError`.

**Problem:** Real IBM/IQM/Quantinuum/IonQ jobs routinely sit in the
queue longer than 5 minutes. A Qiskit user who calls
`job.result()` and expects "block until done" gets an exception
instead — and not even a useful one, because the job is still running
fine on the backend.

**Options:**

1. **`PyJobHandle.result(timeout=None)` with its own polling loop.**
   When `timeout is None`, block forever (or until a sentinel exit
   condition). When `timeout` is set, honour it. Polling lives in the
   PyO3 wrapper, calling `backend.status()` + `backend.result()` rather
   than `backend.wait()`. Backwards-compat: keep the 5-min HAL
   `wait()` default for direct Rust callers.
2. **Per-backend `wait()` override in each adapter.** Each cloud
   adapter implements `wait()` with backend-appropriate polling
   (e.g. IQM job statuses already expose `estimated_completion_time`).
   No timeout by default; users can break out with `Ctrl-C` or close
   the Python process. Less code in the PyO3 layer, more code in
   N adapters.
3. **Both.** PyO3 layer accepts `timeout`, adapters provide
   intelligent polling cadence. Strictly more capable, more surface.

**Recommendation: Option 1 for Phase 2**, with the door open to add
Option 2's per-adapter polling cadence in Phase 3+. Rationale: one
change-site, immediately fixes the timeout problem for every vendor,
doesn't block Phase 2 on cleaning up HAL `wait()`. The PyO3-level
polling loop is ~20 lines.

**Sketch:**

```rust
#[pymethods]
impl PyJobHandle {
    #[pyo3(signature = (timeout=None, poll_interval_ms=500))]
    fn result(
        &self,
        timeout: Option<f64>,
        poll_interval_ms: u64,
        py: Python<'_>,
    ) -> PyResult<PyExecutionResult> {
        let backend = self.backend.clone();
        let job_id = self.job_id.clone();
        let deadline = timeout.map(|s| std::time::Instant::now() + Duration::from_secs_f64(s));
        let poll = Duration::from_millis(poll_interval_ms);
        py.detach(move || runtime().block_on(async move {
            loop {
                match backend.status(&job_id).await? {
                    JobStatus::Completed => return backend.result(&job_id).await,
                    JobStatus::Failed(m) => return Err(HalError::JobFailed(m)),
                    JobStatus::Cancelled => return Err(HalError::JobCancelled),
                    JobStatus::Queued | JobStatus::Running => {
                        if let Some(d) = deadline {
                            if std::time::Instant::now() >= d {
                                return Err(HalError::Timeout(job_id.0.clone()));
                            }
                        }
                        tokio::time::sleep(poll).await;
                    }
                }
            }
        })).map_err(hal_to_py_err).map(|r| PyExecutionResult { inner: Arc::new(r) })
    }
}
```

#### A2 — `ArvakBackend.run()` is eager-blocking, not deferred

**Current state:** `ArvakBackend.run(circuits, shots)` calls
`self._native.run(qasm, shots, parameters)` for each circuit, which
internally chains `submit() -> wait() -> result()`. By the time
`backend.run()` returns, the work is done. The returned `ArvakJob`
just wraps already-computed counts; `.result()` is instant,
`.status()` always says `"DONE"`.

**Problem:** Qiskit `JobV1` semantics — and every existing Qiskit
caller — assume `backend.run(qc)` returns **immediately** with a
handle, and `job.result()` is what blocks. With cloud backends this
matters operationally:

- Users want to fire multiple `backend.run()` calls in parallel and
  collect results later.
- Notebooks want `job = backend.run(qc); do_other_work(); counts = job.result()`.
- Cancel semantics only make sense if `run()` returns before the job
  finishes.

For sim this distinction is invisible (results are instant either
way). For IQM/IBM/Quantinuum the eager pattern means
`backend.run(qc)` blocks the calling thread for the full queue +
execution time.

**Options:**

1. **Restructure `ArvakBackend.run()` to be deferred.** Returns
   immediately with a thin `ArvakJob` that holds a `PyJobHandle` list
   (one per circuit). `.result()` calls `handle.result()` for each
   and aggregates. Native multi-circuit batch APIs (IBM Sampler,
   Quantinuum batch) become a separate concern.
2. **Add a `defer=True` flag to `run()`.** Eager by default, deferred
   on opt-in. Worst of both — users have to know about the flag,
   batch behaviour is still ad-hoc.
3. **Native per-circuit `submit()` + a `BatchJob` wrapper.** Submit
   all circuits as separate HAL jobs, return a wrapper that waits on
   all of them. Closest to Qiskit `JobV1` for many-circuit batches,
   no special batch API needed yet.

**Recommendation: Option 1 for Phase 2**, with Option 3's
multi-handle aggregation built in from the start because it's the
natural shape. Concretely:

```python
class ArvakBackend:
    def run(self, circuits, shots: int = 1024, **options) -> 'ArvakJob':
        if not isinstance(circuits, list):
            circuits = [circuits]
        parameters = options.get('parameters')
        # Submit all circuits; collect handles. Does NOT block.
        handles = [
            self._native.submit(_qiskit_to_qasm3(qc), shots, parameters)
            for qc in circuits
        ]
        return ArvakJob(backend=self, handles=handles, shots=shots)

class ArvakJob:
    """Real deferred job — wraps one or more PyJobHandle instances."""
    def __init__(self, backend, handles, shots):
        self._backend = backend
        self._handles = handles
        self._shots = shots
        self._cached: list[dict] | None = None

    def result(self, timeout: float | None = None) -> 'ArvakResult':
        if self._cached is None:
            self._cached = [
                dict(h.result(timeout=timeout).counts) for h in self._handles
            ]
        return ArvakResult(self._backend.name, self._cached, self._shots)

    def status(self) -> str:
        states = [h.status().state for h in self._handles]
        if all(s == "completed" for s in states): return "DONE"
        if any(s == "failed" for s in states):    return "ERROR"
        if any(s == "cancelled" for s in states): return "CANCELLED"
        if any(s == "running" for s in states):   return "RUNNING"
        return "QUEUED"

    def cancel(self) -> None:
        for h in self._handles: h.cancel()
```

**Consequence for Phase 1:** the current sim path keeps working
unchanged (sim returns instantly, so eager vs deferred is invisible).
The restructure ships as part of Phase 2.

#### A1 + A2 — what they cost

| Item | Files touched | LOC | Risk |
|---|---|---|---|
| A1 (timeout on `result`) | `backend.rs` (one method) | ~25 | low — pure addition |
| A2 (deferred `run`) | `backend.py` (`ArvakBackend.run`, `ArvakJob`) | ~40 | low — sim behaviour identical |

Together: roughly half a day. They land **inside Phase 2** as the
first commits before the IQM adapter is wired in, so the IQM
integration immediately benefits from the right semantics.

#### Decision needed before Phase 2 begins

- [ ] A1: confirm Option 1 (PyO3-level polling with optional timeout)
  or pick another.
- [ ] A2: confirm Option 1+3 hybrid (deferred multi-handle) or pick
  another.

### Phase 2 — IQM Resonance (1–2 days)

The vendor we just discussed. Validates the cloud-vendor pattern.

1. Wire `arvak-adapter-iqm` as an optional feature.
2. Add IQM branch to `BackendRegistry::get`.
3. Switch `provider.get_backend('iqm_*')` to return `ArvakBackend`.
4. Remove `iqm-client[qiskit]==33.0.3` from `pyproject.toml` extras.
5. Mark `ArvakIQMResonanceBackend` deprecated.

**Validates:** cloud auth via env (`IQM_TOKEN`), OIDC path for LUMI/LRZ
still works (existing native adapter supports it), submission/polling
through PyO3.

### Phases 3–6 — Remaining vendors (1–2 days each)

In order of likely difficulty / blast radius:
3. Scaleway (similar shape to IQM)
4. IBM (most users — extra care, more parametrized circuits)
5. Quantinuum (batch API quirks)
6. AQT, IonQ (smaller surface)

Each phase: native adapter wired + Python class switched + vendor SDK
dependency removed + old class deprecated.

### Phase 7 — Cleanup (0.5 day)

1. Delete deprecated `ArvakSimulatorBackend`,
   `ArvakIBMBackend`, `ArvakIQMResonanceBackend`, etc.
2. Delete now-orphan `ArvakIBMJob`, `ArvakIQMResonanceJob`, etc.
3. Drop vendor SDK extras from `pyproject.toml`.
4. CHANGELOG entry, major version bump (this is a breaking change for
   anyone who imported the per-vendor classes directly).

## Backwards compatibility

- `provider.get_backend('iqm_garnet').run(qc, shots).result().get_counts()`
  keeps working throughout — that's the surface 99% of users touch.
- Direct imports of `ArvakIBMBackend` etc. emit a `DeprecationWarning`
  from Phase 1 onward and break at Phase 7. Documented in CHANGELOG.
- Job IDs change format (HAL uses opaque UUIDs; some vendor classes
  exposed vendor-native IDs). If anyone has persisted job IDs across
  sessions, they break. This affects no known caller — verify before
  Phase 7.
- The PyO3 module gets larger by the size of compiled adapters (small —
  most adapter code is `reqwest` calls). Wheel grows ~2–3 MB.

## Risks & open questions

### Risks

1. **Vendor API drift during migration.** If IBM changes its REST
   contract while we're porting Quantinuum, the IBM native adapter
   needs an out-of-band fix. Mitigation: the native adapters already
   exist and are tested — Phase N only switches the *call site*, not
   the implementation.
2. **Hidden Python-side features in the old classes.** Some Python
   classes do small things the Rust adapter doesn't (e.g.,
   `ArvakIBMBackend` has a 27-qubit topology hardcoded as fallback).
   Audit each Python class for behaviour the Rust adapter is missing
   *before* deletion, not after.
3. **Async runtime sharing.** If `arvak-python` ever needs a different
   tokio runtime config than what `pyo3-async-runtimes` defaults to
   (e.g., specific worker thread counts), we need to set that up at
   module init. Not a blocker, just a note.
4. **gRPC service compatibility.** `arvak-grpc` already uses these
   native adapters — no change needed. But if Phase N changes the HAL
   `Backend` trait shape (it shouldn't), gRPC needs to follow.

### Open questions

1. **Should `ArvakBackend` be the same class for all backends, or a
   thin Qiskit-compat subclass per vendor?** Going with single class +
   prefix-dispatch in this RFC. If users complain about `.basis_gates`
   needing to look different per vendor, revisit — but a single class
   that asks Rust for `capabilities().native_gates()` is cleaner.
2. **Job persistence across processes.** Current vendor classes store
   nothing between sessions. Native adapters return `JobId(String)`
   which IS persistable across processes via `backend.status(JobId)`.
   Worth exposing a `provider.attach_job(backend_name, job_id)` helper?
   Out of scope for v1.
3. **Adapter feature defaults.** Ship `arvak` with all adapters
   compiled in by default (bigger wheel, no-friction), or with `sim`
   only (small wheel, `pip install arvak[hardware]` for the rest)?
   Recommendation: ship all by default — the size delta is modest and
   `pip install arvak` should "just work" for new users.
4. **What about `arvak.integrations.cirq`, `pennylane`, `qrisp`?** Same
   pattern applies — their backend objects also currently exist as
   Python classes. This RFC scopes to the Qiskit integration. Once
   Qiskit is done, the pattern ports trivially. Out of scope here.

## Acceptance criteria for marking this RFC accepted

- [ ] Daniel reads it, marks `Status: Accepted` (or returns with
  changes).
- [ ] Phase 1 sub-tasks have rough size estimates.
- [ ] Open question 3 (default features) decided.

## Acceptance criteria for closing each phase

- [ ] All existing tests for that vendor pass against the new
  implementation.
- [ ] At least one happy-path E2E test against the real backend (or a
  high-fidelity stub for vendors we can't burn credentials on).
- [ ] Vendor SDK dependency removed from `pyproject.toml`.
- [ ] CHANGELOG entry under `### Changed`.
- [ ] Old Python class emits `DeprecationWarning` (until Phase 7).
