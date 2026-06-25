//! Unified PyO3 surface for Arvak HAL backends.
//!
//! This module is the single bridge between Python callers and the native
//! Rust adapter implementations of [`arvak_hal::Backend`]. It exposes:
//!
//! - [`PyBackend`] — wraps any `Arc<dyn Backend + Send + Sync>` and proxies
//!   the HAL trait methods to Python via sync-over-async.
//! - [`PyJobHandle`] — opaque handle returned from `submit()`, supports
//!   `status()`, `result()`, `wait()`, `cancel()`.
//! - [`PyCapabilities`], [`PyJobStatus`], [`PyExecutionResult`],
//!   [`PyValidationResult`], [`PyAvailability`] — read-only Python views of
//!   the corresponding HAL types.
//! - Module-level functions `backend_for(name)` and `list_backends()`.
//!
//! # Design
//!
//! The HAL `Backend` trait is `#[async_trait]`. Python users expect
//! synchronous blocking semantics (matching Qiskit `JobV1`). We satisfy
//! that by running every async method on a process-wide tokio runtime and
//! releasing the GIL during the `block_on` so Python threads can make
//! progress.
//!
//! See `docs/RFC/0001-native-backend-unification.md` for the design rationale.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use pyo3::exceptions::{
    PyConnectionError, PyPermissionError, PyRuntimeError, PyTimeoutError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use arvak_hal::{
    Backend, BackendAvailability, Capabilities, ExecutionResult, HalError, JobId, JobStatus,
    ValidationResult,
};

// ---------------------------------------------------------------------------
// Shared tokio runtime (sync-over-async)
// ---------------------------------------------------------------------------

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("arvak-python-bg")
            .build()
            .expect("failed to build tokio runtime for arvak-python")
    })
}

// ---------------------------------------------------------------------------
// Error mapping: HalError → Python exception
// ---------------------------------------------------------------------------

fn hal_to_py_err(e: HalError) -> PyErr {
    let msg = e.to_string();
    match e {
        HalError::BackendUnavailable(_) => PyConnectionError::new_err(msg),
        HalError::AuthenticationFailed(_) | HalError::Auth(_) => PyPermissionError::new_err(msg),
        HalError::Timeout(_) => PyTimeoutError::new_err(msg),
        HalError::InvalidCircuit(_) | HalError::CircuitTooLarge(_) | HalError::InvalidShots(_) => {
            PyValueError::new_err(msg)
        }
        HalError::Unsupported(_) => pyo3::exceptions::PyNotImplementedError::new_err(msg),
        _ => PyRuntimeError::new_err(msg),
    }
}

// ---------------------------------------------------------------------------
// PyCapabilities — read-only view of arvak_hal::Capabilities
// ---------------------------------------------------------------------------

/// Backend capabilities (read-only view).
#[pyclass(name = "Capabilities", module = "arvak", frozen)]
#[derive(Clone)]
pub struct PyCapabilities {
    inner: Arc<Capabilities>,
}

#[pymethods]
impl PyCapabilities {
    /// Backend name (e.g. `"simulator"`, `"iqm_garnet"`).
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Number of qubits.
    #[getter]
    fn num_qubits(&self) -> u32 {
        self.inner.num_qubits
    }

    /// Native gate set (sorted, OpenQASM 3 naming).
    #[getter]
    fn basis_gates(&self) -> Vec<String> {
        let g = &self.inner.gate_set;
        let mut out: Vec<String> = g
            .single_qubit
            .iter()
            .chain(g.two_qubit.iter())
            .chain(g.three_qubit.iter())
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// `true` if this is a simulator/emulator.
    #[getter]
    fn is_simulator(&self) -> bool {
        self.inner.is_simulator
    }

    /// Maximum number of shots per job.
    #[getter]
    fn max_shots(&self) -> u32 {
        self.inner.max_shots
    }

    /// Maximum gate operations per circuit, or `None` if uncapped.
    #[getter]
    fn max_circuit_ops(&self) -> Option<u32> {
        self.inner.max_circuit_ops
    }

    /// Coupling map as a list of `[a, b]` pairs, or `None` for all-to-all.
    ///
    /// Returns `None` when the topology is fully connected — matches the
    /// Qiskit `BackendV2.coupling_map = None` convention. For all other
    /// topologies the explicit edge list is returned.
    #[getter]
    fn coupling_map(&self) -> Option<Vec<(u32, u32)>> {
        use arvak_hal::TopologyKind;
        if matches!(self.inner.topology.kind, TopologyKind::FullyConnected) {
            return None;
        }
        let edges: Vec<(u32, u32)> = self.inner.topology.edges.clone();
        if edges.is_empty() { None } else { Some(edges) }
    }

    /// Capability feature flags (e.g. `"statevector"`, `"dynamic_circuits"`).
    #[getter]
    fn features(&self) -> Vec<String> {
        self.inner.features.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Capabilities(name='{}', qubits={}, simulator={})>",
            self.inner.name, self.inner.num_qubits, self.inner.is_simulator
        )
    }
}

// ---------------------------------------------------------------------------
// PyAvailability
// ---------------------------------------------------------------------------

/// Backend availability snapshot.
#[pyclass(name = "Availability", module = "arvak", frozen)]
#[derive(Clone)]
pub struct PyAvailability {
    #[pyo3(get)]
    is_available: bool,
    #[pyo3(get)]
    queue_depth: Option<u32>,
    #[pyo3(get)]
    estimated_wait_s: Option<u64>,
    #[pyo3(get)]
    status_message: Option<String>,
}

impl From<BackendAvailability> for PyAvailability {
    fn from(a: BackendAvailability) -> Self {
        Self {
            is_available: a.is_available,
            queue_depth: a.queue_depth,
            estimated_wait_s: a.estimated_wait.map(|d| d.as_secs()),
            status_message: a.status_message,
        }
    }
}

#[pymethods]
impl PyAvailability {
    fn __bool__(&self) -> bool {
        self.is_available
    }

    fn __repr__(&self) -> String {
        format!(
            "<Availability(available={}, queue={:?}, msg={:?})>",
            self.is_available, self.queue_depth, self.status_message
        )
    }
}

// ---------------------------------------------------------------------------
// PyValidationResult
// ---------------------------------------------------------------------------

/// Result of circuit validation.
///
/// One of three states:
/// - `Valid` — circuit can be submitted as-is.
/// - `Invalid` — circuit violates backend constraints; `reasons` is non-empty.
/// - `RequiresTranspilation` — circuit needs compilation first; `details`
///   describes what.
#[pyclass(name = "ValidationResult", module = "arvak", frozen)]
#[derive(Clone)]
pub struct PyValidationResult {
    #[pyo3(get)]
    valid: bool,
    #[pyo3(get)]
    requires_transpilation: bool,
    #[pyo3(get)]
    reasons: Vec<String>,
    #[pyo3(get)]
    details: Option<String>,
}

impl From<ValidationResult> for PyValidationResult {
    fn from(v: ValidationResult) -> Self {
        match v {
            ValidationResult::Valid => Self {
                valid: true,
                requires_transpilation: false,
                reasons: vec![],
                details: None,
            },
            ValidationResult::Invalid { reasons } => Self {
                valid: false,
                requires_transpilation: false,
                reasons,
                details: None,
            },
            ValidationResult::RequiresTranspilation { details } => Self {
                valid: false,
                requires_transpilation: true,
                reasons: vec![],
                details: Some(details),
            },
        }
    }
}

#[pymethods]
impl PyValidationResult {
    fn __bool__(&self) -> bool {
        self.valid
    }

    fn __repr__(&self) -> String {
        if self.valid {
            "<ValidationResult(valid)>".into()
        } else if self.requires_transpilation {
            format!(
                "<ValidationResult(requires_transpilation, details={:?})>",
                self.details
            )
        } else {
            format!("<ValidationResult(invalid, reasons={:?})>", self.reasons)
        }
    }
}

// ---------------------------------------------------------------------------
// PyJobStatus
// ---------------------------------------------------------------------------

/// Job status. String values: `"queued"`, `"running"`, `"completed"`,
/// `"failed"`, `"cancelled"`.
#[pyclass(name = "JobStatus", module = "arvak", frozen)]
#[derive(Clone)]
pub struct PyJobStatus {
    #[pyo3(get)]
    state: String,
    #[pyo3(get)]
    message: Option<String>,
}

impl From<JobStatus> for PyJobStatus {
    fn from(s: JobStatus) -> Self {
        match s {
            JobStatus::Queued => Self {
                state: "queued".into(),
                message: None,
            },
            JobStatus::Running => Self {
                state: "running".into(),
                message: None,
            },
            JobStatus::Completed => Self {
                state: "completed".into(),
                message: None,
            },
            JobStatus::Failed(msg) => Self {
                state: "failed".into(),
                message: Some(msg),
            },
            JobStatus::Cancelled => Self {
                state: "cancelled".into(),
                message: None,
            },
        }
    }
}

#[pymethods]
impl PyJobStatus {
    fn is_terminal(&self) -> bool {
        matches!(self.state.as_str(), "completed" | "failed" | "cancelled")
    }

    fn is_done(&self) -> bool {
        self.state == "completed"
    }

    fn __repr__(&self) -> String {
        match &self.message {
            Some(m) => format!("<JobStatus({}: {})>", self.state, m),
            None => format!("<JobStatus({})>", self.state),
        }
    }

    fn __str__(&self) -> String {
        self.state.clone()
    }
}

// ---------------------------------------------------------------------------
// PyExecutionResult
// ---------------------------------------------------------------------------

/// Result of circuit execution. Counts are accessed via `.counts` (a dict).
#[pyclass(name = "ExecutionResult", module = "arvak", frozen)]
#[derive(Clone)]
pub struct PyExecutionResult {
    inner: Arc<ExecutionResult>,
}

#[pymethods]
impl PyExecutionResult {
    /// Measurement counts as `{bitstring: count}`.
    #[getter]
    fn counts(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let d = PyDict::new(py);
        for (bitstring, count) in self.inner.counts.iter() {
            d.set_item(bitstring, count)?;
        }
        Ok(d.into())
    }

    /// Number of shots executed.
    #[getter]
    fn shots(&self) -> u32 {
        self.inner.shots
    }

    /// Execution time in milliseconds, if reported by the backend.
    #[getter]
    fn execution_time_ms(&self) -> Option<u64> {
        self.inner.execution_time_ms
    }

    fn __repr__(&self) -> String {
        format!(
            "<ExecutionResult(shots={}, bitstrings={})>",
            self.inner.shots,
            self.inner.counts.len()
        )
    }
}

// ---------------------------------------------------------------------------
// PyJobHandle
// ---------------------------------------------------------------------------

/// Handle to a submitted job.
///
/// Returned by [`PyBackend::submit`]. Holds a strong reference to the
/// originating backend so the job survives the Python `backend` going out
/// of scope between submission and result retrieval.
#[pyclass(name = "JobHandle", module = "arvak")]
pub struct PyJobHandle {
    backend: Arc<dyn Backend + Send + Sync>,
    job_id: JobId,
    shots: u32,
}

#[pymethods]
impl PyJobHandle {
    /// Opaque job identifier.
    #[getter]
    fn job_id(&self) -> String {
        self.job_id.0.clone()
    }

    /// Number of shots originally requested.
    #[getter]
    fn shots(&self) -> u32 {
        self.shots
    }

    /// Query current status (single round-trip, not blocking until done).
    fn status(&self, py: Python<'_>) -> PyResult<PyJobStatus> {
        let backend = self.backend.clone();
        let job_id = self.job_id.clone();
        let s = py
            .detach(move || runtime().block_on(async move { backend.status(&job_id).await }))
            .map_err(hal_to_py_err)?;
        Ok(s.into())
    }

    /// Cancel the job.
    fn cancel(&self, py: Python<'_>) -> PyResult<()> {
        let backend = self.backend.clone();
        let job_id = self.job_id.clone();
        py.detach(move || runtime().block_on(async move { backend.cancel(&job_id).await }))
            .map_err(hal_to_py_err)
    }

    /// Block until the job completes, then return the result.
    ///
    /// Polls `status()` at `poll_interval_ms` cadence until terminal,
    /// then fetches `result()`. Unlike the HAL `wait()` default (which
    /// has a 5-minute hard cap), this method has no built-in timeout —
    /// passing `timeout=None` (the default) waits indefinitely, matching
    /// Qiskit `JobV1.result()` semantics for cloud-vendor jobs that can
    /// sit in queue for hours.
    ///
    /// Args:
    ///   timeout: maximum seconds to wait. `None` means wait forever.
    ///   poll_interval_ms: cadence of `status()` polls (default 500 ms).
    ///
    /// Raises:
    ///   `TimeoutError` if `timeout` elapses before the job reaches a
    ///   terminal state.
    #[pyo3(signature = (timeout=None, poll_interval_ms=500))]
    fn result(
        &self,
        timeout: Option<f64>,
        poll_interval_ms: u64,
        py: Python<'_>,
    ) -> PyResult<PyExecutionResult> {
        let backend = self.backend.clone();
        let job_id = self.job_id.clone();
        let poll = Duration::from_millis(poll_interval_ms.max(1));
        let deadline = timeout.map(|s| Instant::now() + Duration::from_secs_f64(s.max(0.0)));

        let res = py
            .detach(move || {
                runtime().block_on(async move {
                    loop {
                        match backend.status(&job_id).await? {
                            JobStatus::Completed => return backend.result(&job_id).await,
                            JobStatus::Failed(m) => return Err(HalError::JobFailed(m)),
                            JobStatus::Cancelled => return Err(HalError::JobCancelled),
                            JobStatus::Queued | JobStatus::Running => {
                                if let Some(d) = deadline
                                    && Instant::now() >= d
                                {
                                    return Err(HalError::Timeout(job_id.0.clone()));
                                }
                                tokio::time::sleep(poll).await;
                            }
                        }
                    }
                })
            })
            .map_err(hal_to_py_err)?;
        Ok(PyExecutionResult {
            inner: Arc::new(res),
        })
    }

    /// Alias for [`Self::result`] (Qiskit `JobV1` compat).
    #[pyo3(signature = (timeout=None, poll_interval_ms=500))]
    fn wait(
        &self,
        timeout: Option<f64>,
        poll_interval_ms: u64,
        py: Python<'_>,
    ) -> PyResult<PyExecutionResult> {
        self.result(timeout, poll_interval_ms, py)
    }

    fn __repr__(&self) -> String {
        format!("<JobHandle(id={}, shots={})>", self.job_id.0, self.shots)
    }
}

// ---------------------------------------------------------------------------
// PyBackend
// ---------------------------------------------------------------------------

/// Native Arvak backend handle.
///
/// Returned by [`backend_for`]. Wraps an `Arc<dyn arvak_hal::Backend>` and
/// proxies HAL methods to Python via sync-over-async. One Python class
/// handles every supported vendor — vendor-specific behaviour comes from
/// `capabilities()` at the Rust side.
#[pyclass(name = "Backend", module = "arvak")]
pub struct PyBackend {
    inner: Arc<dyn Backend + Send + Sync>,
}

#[pymethods]
impl PyBackend {
    /// Backend name (e.g. `"simulator"`, `"iqm_garnet"`).
    #[getter]
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// Backend capabilities (cached at construction by the adapter).
    fn capabilities(&self) -> PyCapabilities {
        PyCapabilities {
            inner: Arc::new(self.inner.capabilities().clone()),
        }
    }

    /// Number of qubits (Qiskit-compat shortcut for `.capabilities().num_qubits`).
    #[getter]
    fn num_qubits(&self) -> u32 {
        self.inner.capabilities().num_qubits
    }

    /// Native gate set (Qiskit-compat).
    #[getter]
    fn basis_gates(&self) -> Vec<String> {
        self.capabilities().basis_gates()
    }

    /// Coupling map, or `None` for all-to-all.
    #[getter]
    fn coupling_map(&self) -> Option<Vec<(u32, u32)>> {
        self.capabilities().coupling_map()
    }

    /// Query backend availability (single round-trip).
    fn availability(&self, py: Python<'_>) -> PyResult<PyAvailability> {
        let backend = self.inner.clone();
        let a = py
            .detach(move || runtime().block_on(async move { backend.availability().await }))
            .map_err(hal_to_py_err)?;
        Ok(a.into())
    }

    /// Validate a circuit (parsed from QASM3) against backend constraints.
    fn validate(&self, qasm: &str, py: Python<'_>) -> PyResult<PyValidationResult> {
        let circuit = arvak_qasm3::parse(qasm).map_err(crate::error::parse_to_py_err)?;
        let backend = self.inner.clone();
        let v = py
            .detach(move || runtime().block_on(async move { backend.validate(&circuit).await }))
            .map_err(hal_to_py_err)?;
        Ok(v.into())
    }

    /// Submit a circuit (QASM3 string) for execution. Returns a job handle.
    ///
    /// `parameters` maps `input float[64]` parameter names to concrete
    /// values; pass `None` (or omit) for non-parametric circuits.
    #[pyo3(signature = (qasm, shots=1024, parameters=None))]
    fn submit(
        &self,
        qasm: &str,
        shots: u32,
        parameters: Option<HashMap<String, f64>>,
        py: Python<'_>,
    ) -> PyResult<PyJobHandle> {
        if shots == 0 {
            return Err(PyValueError::new_err("shots must be > 0"));
        }
        let circuit = arvak_qasm3::parse(qasm).map_err(crate::error::parse_to_py_err)?;
        let backend = self.inner.clone();
        let job_id = py
            .detach(move || {
                runtime().block_on(async move {
                    backend.submit(&circuit, shots, parameters.as_ref()).await
                })
            })
            .map_err(hal_to_py_err)?;
        Ok(PyJobHandle {
            backend: self.inner.clone(),
            job_id,
            shots,
        })
    }

    /// Submit + wait + fetch — convenience wrapper.
    ///
    /// Equivalent to `backend.submit(qasm, shots, parameters).result(timeout)`.
    /// For deferred submission (return immediately, fetch later), call
    /// `.submit()` directly.
    #[pyo3(signature = (qasm, shots=1024, parameters=None, timeout=None, poll_interval_ms=500))]
    fn run(
        &self,
        qasm: &str,
        shots: u32,
        parameters: Option<HashMap<String, f64>>,
        timeout: Option<f64>,
        poll_interval_ms: u64,
        py: Python<'_>,
    ) -> PyResult<PyExecutionResult> {
        let handle = self.submit(qasm, shots, parameters, py)?;
        handle.result(timeout, poll_interval_ms, py)
    }

    fn __repr__(&self) -> String {
        let cap = self.inner.capabilities();
        format!(
            "<Backend(name='{}', qubits={}, simulator={})>",
            cap.name, cap.num_qubits, cap.is_simulator
        )
    }
}

// ---------------------------------------------------------------------------
// Backend registry — builds backends by name
// ---------------------------------------------------------------------------

/// Construct a backend by name.
///
/// Supported as of Phase 2a:
///   - `"sim"` / `"simulator"` / `"arvak_simulator"` — local statevector
///     simulator (feature `simulator`, on by default).
///   - `"iqm_garnet"` / `"iqm_sirius"` / `"iqm_emerald"` / `"iqm_crystal"`
///     — IQM Resonance cloud machines (feature `adapter-iqm`, on by
///     default). Auth via `IQM_TOKEN` env var.
///
/// LUMI Helmi / LRZ paths (OIDC-authenticated) are out of scope for
/// 2a — they need the missing `IqmBackend::with_oidc()` glue (RFC
/// Phase 2b).
fn make_backend(name: &str) -> Result<Arc<dyn Backend + Send + Sync>, HalError> {
    match name {
        "sim" | "arvak_simulator" | "simulator" => {
            #[cfg(feature = "simulator")]
            {
                Ok(Arc::new(arvak_adapter_sim::SimulatorBackend::new()))
            }
            #[cfg(not(feature = "simulator"))]
            {
                Err(HalError::Configuration(
                    "simulator adapter not compiled in (enable 'simulator' feature)".into(),
                ))
            }
        }
        // LUMI Helmi at CSC Finland — IQM hardware via CSC OIDC auth.
        // Endpoint comes from HELMI_CORTEX_URL (set by LUMI's helmi_qiskit
        // module) so we don't hardcode a private URL. Project ID comes
        // from LUMI_PROJECT_ID. Initial OIDC login is handled by `arvak
        // auth login` (out-of-band, browser-based); this call reads the
        // cached token and refreshes via refresh-token if needed.
        "iqm_helmi" => {
            #[cfg(feature = "adapter-iqm")]
            {
                let endpoint = std::env::var("HELMI_CORTEX_URL").map_err(|_| {
                    HalError::Configuration(
                        "HELMI_CORTEX_URL not set — LUMI's helmi_qiskit module \
                         sets this env var when loaded. For local use outside \
                         LUMI, set it manually to the Helmi Cortex API URL."
                            .into(),
                    )
                })?;
                let project_id = std::env::var("LUMI_PROJECT_ID").map_err(|_| {
                    HalError::Configuration(
                        "LUMI_PROJECT_ID not set — required for CSC OIDC auth. \
                         Use the project ID you applied for on LUMI."
                            .into(),
                    )
                })?;
                let oidc = arvak_hal::OidcConfig::lumi(&project_id);
                let backend = arvak_adapter_iqm::IqmBackend::with_oidc(oidc, "helmi", endpoint)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-iqm"))]
            {
                Err(HalError::Configuration(
                    "IQM adapter not compiled in (enable 'adapter-iqm' feature)".into(),
                ))
            }
        }
        // LRZ-hosted IQM machines via LRZ OIDC.
        // Endpoint from LRZ_IQM_URL, project from LRZ_PROJECT_ID.
        // Target name follows pattern iqm_lrz_<machine>.
        n if n.starts_with("iqm_lrz_") => {
            #[cfg(feature = "adapter-iqm")]
            {
                let target = &n["iqm_lrz_".len()..];
                if target.is_empty() {
                    return Err(HalError::Configuration(format!(
                        "invalid LRZ backend name: {n} (expected 'iqm_lrz_<machine>')"
                    )));
                }
                let endpoint = std::env::var("LRZ_IQM_URL").map_err(|_| {
                    HalError::Configuration(
                        "LRZ_IQM_URL not set — required for LRZ-hosted IQM access. \
                         See LRZ quantum-computing documentation for the URL."
                            .into(),
                    )
                })?;
                let project_id = std::env::var("LRZ_PROJECT_ID").map_err(|_| {
                    HalError::Configuration(
                        "LRZ_PROJECT_ID not set — required for LRZ OIDC auth.".into(),
                    )
                })?;
                let oidc = arvak_hal::OidcConfig::lrz(&project_id);
                let backend = arvak_adapter_iqm::IqmBackend::with_oidc(oidc, target, endpoint)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-iqm"))]
            {
                Err(HalError::Configuration(
                    "IQM adapter not compiled in (enable 'adapter-iqm' feature)".into(),
                ))
            }
        }
        // Quandela — photonic QPUs and simulators via Perceval Cloud.
        // Auth: PCVL_CLOUD_TOKEN env var (Perceval's convention; the
        // adapter passes this to a Perceval Python subprocess bridge).
        // Platform layout from arvak_adapter_quandela::backend:
        //   sim:ascella (6q free sim)  qpu:ascella (6q QPU)
        //   sim:belenos (12q sim)      qpu:belenos (12q QPU, 2025)
        //   quandela_altair (legacy Altair 4K cryocooled, 5q —
        //     also the InternalCode PUF target)
        n if n.starts_with("quandela_") => {
            #[cfg(feature = "adapter-quandela")]
            {
                let platform = match n {
                    "quandela_ascella_sim" => "sim:ascella",
                    "quandela_ascella" => "qpu:ascella",
                    "quandela_belenos_sim" => "sim:belenos",
                    "quandela_belenos" => "qpu:belenos",
                    // Perceval Cloud uses the `qpu:<name>` prefix for hardware
                    // platforms. Verified 2026-06-25 against the live Cloud
                    // API: `qpu:altair` returns 200, the literal string
                    // `quandela_altair` returns 404. (Free-tier tokens get
                    // 403 on this platform — Altair access is gated — but
                    // that's an upstream auth concern, not a routing bug.)
                    "quandela_altair" => "qpu:altair",
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown Quandela backend: {other} \
                             (known: quandela_ascella_sim, quandela_ascella, \
                             quandela_belenos_sim, quandela_belenos, quandela_altair)"
                        )));
                    }
                };
                // PCVL_CLOUD_TOKEN is required for Quandela Cloud platforms.
                // The `sim:*` simulators may run without it via local mock,
                // but the Perceval bridge consults the env var either way.
                // We don't gate on it here — let the bridge surface the
                // auth error with its own Perceval-specific message.
                let backend = arvak_adapter_quandela::QuandelaBackend::for_platform(platform)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-quandela"))]
            {
                Err(HalError::Configuration(format!(
                    "Quandela adapter not compiled in for backend {n} (enable 'adapter-quandela' feature)"
                )))
            }
        }
        // AQT (Alpine Quantum Technologies) — ion-trap simulators and
        // IBEX Q1 hardware via the Arnica cloud API. AQT_TOKEN is
        // required even for offline simulators (Arnica validates).
        n if n.starts_with("aqt_") => {
            #[cfg(feature = "adapter-aqt")]
            {
                // Registry name → (workspace, resource) on Arnica.
                let (workspace, resource) = match n {
                    "aqt_offline_sim" => ("default", "offline_simulator_no_noise"),
                    "aqt_noise_sim" => ("default", "offline_simulator_noise"),
                    "aqt_cloud_sim" => ("aqt_simulators", "simulator_noise"),
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown AQT backend: {other} \
                             (known: aqt_offline_sim, aqt_noise_sim, aqt_cloud_sim)"
                        )));
                    }
                };
                // The adapter itself tolerates an empty AQT_TOKEN (offline
                // sim path) but Arnica still validates tokens server-side.
                // We surface a clearer error if the user hasn't set one.
                if std::env::var("AQT_TOKEN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .is_none()
                {
                    return Err(HalError::Configuration(
                        "AQT_TOKEN not set — required by Arnica cloud API even for \
                         offline simulators (confirmed 2026-02-21). Get a token from \
                         arnica.aqt.eu."
                            .into(),
                    ));
                }
                let backend = arvak_adapter_aqt::AqtBackend::with_resource(workspace, resource)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-aqt"))]
            {
                Err(HalError::Configuration(format!(
                    "AQT adapter not compiled in for backend {n} (enable 'adapter-aqt' feature)"
                )))
            }
        }
        // IonQ — cloud simulator (29q free tier) + Aria-1/2 (25q) +
        // Forte-1 (36q) trapped-ion QPUs via REST. Auth: IONQ_API_KEY.
        n if n.starts_with("ionq_") => {
            #[cfg(feature = "adapter-ionq")]
            {
                let device = match n {
                    "ionq_simulator" => "simulator",
                    "ionq_aria_1" => "qpu.aria-1",
                    "ionq_aria_2" => "qpu.aria-2",
                    "ionq_forte_1" => "qpu.forte-1",
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown IonQ backend: {other} \
                             (known: ionq_simulator, ionq_aria_1, ionq_aria_2, ionq_forte_1)"
                        )));
                    }
                };
                std::env::var("IONQ_API_KEY").map_err(|_| {
                    HalError::Configuration(
                        "IONQ_API_KEY not set — required for IonQ Cloud API. \
                         Get a key from cloud.ionq.com."
                            .into(),
                    )
                })?;
                let backend = arvak_adapter_ionq::IonQBackend::with_backend(device)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-ionq"))]
            {
                Err(HalError::Configuration(format!(
                    "IonQ adapter not compiled in for backend {n} (enable 'adapter-ionq' feature)"
                )))
            }
        }
        // AWS Braket — managed quantum service. Cloud simulators (SV1/
        // TN1/DM1) plus QPU access to Rigetti Ankaa-3, IonQ Aria-1/2/
        // Forte-1, and IQM Garnet. Auth via standard AWS chain (env,
        // SSO, config, IAM role). Requires ARVAK_BRAKET_S3_BUCKET for
        // task result storage. Device ARN constants live in
        // arvak_adapter_braket::device.
        n if n.starts_with("braket_") => {
            #[cfg(feature = "adapter-braket")]
            {
                use arvak_adapter_braket::device;
                let arn = match n {
                    "braket_sv1" => device::SV1,
                    "braket_tn1" => device::TN1,
                    "braket_dm1" => device::DM1,
                    "braket_rigetti_ankaa" => device::RIGETTI_ANKAA_3,
                    "braket_ionq_aria_1" => device::IONQ_ARIA,
                    "braket_ionq_aria_2" => device::IONQ_ARIA_2,
                    "braket_ionq_forte_1" => device::IONQ_FORTE,
                    "braket_iqm_garnet" => device::IQM_GARNET,
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown Braket backend: {other} \
                             (known: braket_sv1, braket_tn1, braket_dm1, \
                             braket_rigetti_ankaa, braket_ionq_aria_1, \
                             braket_ionq_aria_2, braket_ionq_forte_1, \
                             braket_iqm_garnet)"
                        )));
                    }
                };
                // ARVAK_BRAKET_S3_BUCKET is required by the adapter for
                // result storage. Surface it before going async so the
                // user gets a clear message rather than an opaque
                // BraketError::MissingS3Bucket from inside connect().
                std::env::var("ARVAK_BRAKET_S3_BUCKET").map_err(|_| {
                    HalError::Configuration(
                        "ARVAK_BRAKET_S3_BUCKET not set — required for Braket task results. \
                         Create an S3 bucket and grant Braket write access (see AWS docs)."
                            .into(),
                    )
                })?;
                // BraketBackend::connect() is async (initializes AWS
                // SDK client + fetches device info for unknown ARNs).
                let backend = runtime()
                    .block_on(arvak_adapter_braket::BraketBackend::connect(arn))
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-braket"))]
            {
                Err(HalError::Configuration(format!(
                    "Braket adapter not compiled in for backend {n} (enable 'adapter-braket' feature)"
                )))
            }
        }
        // NVIDIA CUDA-Q — GPU-accelerated simulators (and eventually
        // hardware backends) via REST. Targets:
        //   cudaq_mqpu          → nvidia-mqpu (40q multi-GPU statevector)
        //   cudaq_custatevec    → custatevec  (32q single-GPU statevector)
        //   cudaq_tensornet     → tensornet   (100q tensor-network)
        //   cudaq_density_matrix→ density-matrix (20q noise sim)
        // Auth via CUDAQ_API_TOKEN.
        n if n.starts_with("cudaq_") => {
            #[cfg(feature = "adapter-cudaq")]
            {
                let target = match n {
                    "cudaq_mqpu" => arvak_adapter_cudaq::targets::MQPU,
                    "cudaq_custatevec" => arvak_adapter_cudaq::targets::CUSTATEVEC,
                    "cudaq_tensornet" => arvak_adapter_cudaq::targets::TENSORNET,
                    "cudaq_density_matrix" => arvak_adapter_cudaq::targets::DM,
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown CUDA-Q backend: {other} \
                             (known: cudaq_mqpu, cudaq_custatevec, cudaq_tensornet, \
                             cudaq_density_matrix)"
                        )));
                    }
                };
                // CUDAQ_API_TOKEN is read by CudaqBackend::with_target() —
                // pre-check it here to provide a clearer error than
                // CudaqError::MissingToken.
                std::env::var("CUDAQ_API_TOKEN").map_err(|_| {
                    HalError::Configuration(
                        "CUDAQ_API_TOKEN not set — required for NVIDIA CUDA-Q Cloud API. \
                         Get a token from build.nvidia.com → API keys."
                            .into(),
                    )
                })?;
                let backend = arvak_adapter_cudaq::CudaqBackend::with_target(target)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-cudaq"))]
            {
                Err(HalError::Configuration(format!(
                    "CUDA-Q adapter not compiled in for backend {n} (enable 'adapter-cudaq' feature)"
                )))
            }
        }
        // MQT DDSIM — local decision-diagram simulator. Runs out-of-
        // process via a python3 subprocess (`pip install mqt.ddsim`).
        // No cloud auth. The bare name "ddsim" maps to default
        // settings (128q ceiling); future targets like ddsim_path,
        // ddsim_hybrid can be added if/when we wire alternative DDSIM
        // engines.
        "ddsim" => {
            #[cfg(feature = "adapter-ddsim")]
            {
                Ok(Arc::new(arvak_adapter_ddsim::DdsimBackend::new()))
            }
            #[cfg(not(feature = "adapter-ddsim"))]
            {
                Err(HalError::Configuration(
                    "DDSIM adapter not compiled in (enable 'adapter-ddsim' feature)".into(),
                ))
            }
        }
        // Quantinuum H1/H2 ion-trap systems (real hardware + emulators).
        // Auth via QUANTINUUM_EMAIL + QUANTINUUM_PASSWORD env vars.
        //
        // Registry name → Quantinuum machine identifier:
        //   quantinuum_h2           → H2-1   (real H2 hardware)
        //   quantinuum_h2_emulator  → H2-1E  (cloud emulator)
        //   quantinuum_h1_emulator  → H1-1E  (cloud emulator)
        //   (the local noiseless emulator H2-1LE is the adapter default)
        //
        // RFC §Phase 5 gate: evaluated MQT ionshuttler on 2026-06-25,
        // decided to skip. ionshuttler is a generic QCCD research tool —
        // Quantinuum's commercial pipeline handles shuttling internally
        // and doesn't expose shuttle-schedule input via REST. Not
        // useful in our thin-adapter shape.
        n if n.starts_with("quantinuum_") => {
            #[cfg(feature = "adapter-quantinuum")]
            {
                let machine = match n {
                    "quantinuum_h2" => "H2-1",
                    "quantinuum_h2_emulator" => "H2-1E",
                    "quantinuum_h1_emulator" => "H1-1E",
                    other => {
                        return Err(HalError::Configuration(format!(
                            "unknown Quantinuum backend: {other} \
                             (known: quantinuum_h2, quantinuum_h2_emulator, quantinuum_h1_emulator)"
                        )));
                    }
                };
                // Pre-validate env vars for clearer error messages than the
                // adapter's bare MissingEmail / MissingPassword variants.
                std::env::var("QUANTINUUM_EMAIL").map_err(|_| {
                    HalError::Configuration(
                        "QUANTINUUM_EMAIL not set — required for Quantinuum API auth. \
                         Sign up at quantinuum.com."
                            .into(),
                    )
                })?;
                std::env::var("QUANTINUUM_PASSWORD").map_err(|_| {
                    HalError::Configuration(
                        "QUANTINUUM_PASSWORD not set — required for Quantinuum API auth.".into(),
                    )
                })?;
                let backend = arvak_adapter_quantinuum::QuantinuumBackend::with_target(machine)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-quantinuum"))]
            {
                Err(HalError::Configuration(format!(
                    "Quantinuum adapter not compiled in for backend {n} (enable 'adapter-quantinuum' feature)"
                )))
            }
        }
        // IBM Quantum (Cloud API). Modern path uses IBM_API_KEY + a service
        // CRN. EU-hosted backends (brussels, strasbourg, aachen) check
        // IBM_SERVICE_CRN_EU first, with fallback to IBM_SERVICE_CRN — matches
        // the legacy ArvakIBMBackend behaviour. US-East backends use
        // IBM_SERVICE_CRN directly. The native adapter parses the CRN to
        // pick the correct region endpoint.
        n if n.starts_with("ibm_") => {
            #[cfg(feature = "adapter-ibm")]
            {
                const EU_BACKENDS: &[&str] = &["ibm_brussels", "ibm_strasbourg", "ibm_aachen"];
                let is_eu = EU_BACKENDS.contains(&n);

                // Sanity check: we want IBM_API_KEY to be set before going
                // further so the error message points at the missing key
                // rather than an IbmError::MissingServiceCrn surfaced from
                // deep inside connect().
                std::env::var("IBM_API_KEY").map_err(|_| {
                    HalError::Configuration(
                        "IBM_API_KEY not set — required for IBM Cloud Quantum API. \
                         Get it from cloud.ibm.com → API keys."
                            .into(),
                    )
                })?;

                // EU path: prefer IBM_SERVICE_CRN_EU, fall back to IBM_SERVICE_CRN.
                // Adapter reads IBM_SERVICE_CRN directly, so we shim the EU CRN
                // through it when present.
                if is_eu && let Ok(eu_crn) = std::env::var("IBM_SERVICE_CRN_EU") {
                    // SAFETY: setting env var to redirect the adapter's read.
                    // Single-threaded at this point in the registry path.
                    unsafe {
                        std::env::set_var("IBM_SERVICE_CRN", &eu_crn);
                    }
                }

                if std::env::var("IBM_SERVICE_CRN").is_err() {
                    let hint = if is_eu {
                        "IBM_SERVICE_CRN_EU or IBM_SERVICE_CRN not set — required for \
                         EU-hosted IBM backends (Frankfurt region)."
                    } else {
                        "IBM_SERVICE_CRN not set — required for IBM Cloud Quantum API. \
                         Format: crn:v1:bluemix:public:quantum-computing:<region>:..."
                    };
                    return Err(HalError::Configuration(hint.into()));
                }

                // IbmBackend::connect() is async (fetches real topology /
                // qubit count from the Cloud API). Block on it via the
                // shared runtime.
                let target = n.to_string();
                let backend = runtime()
                    .block_on(arvak_adapter_ibm::IbmBackend::connect(target))
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-ibm"))]
            {
                Err(HalError::Configuration(format!(
                    "IBM adapter not compiled in for backend {n} (enable 'adapter-ibm' feature)"
                )))
            }
        }
        // Scaleway QaaS — IQM hardware (Garnet/Sirius/Emerald) hosted by
        // Scaleway, accessed via their REST API. Requires three env vars:
        // SCALEWAY_SECRET_KEY, SCALEWAY_PROJECT_ID, SCALEWAY_SESSION_ID
        // (the session must be pre-created in the Scaleway console).
        n if n.starts_with("scaleway_") => {
            #[cfg(feature = "adapter-scaleway")]
            {
                let machine = &n["scaleway_".len()..];
                let platform = match machine {
                    "garnet" => "QPU-GARNET-20PQ",
                    "sirius" => "QPU-SIRIUS-24PQ",
                    "emerald" => "QPU-EMERALD-54PQ",
                    _ => {
                        return Err(HalError::Configuration(format!(
                            "unknown Scaleway machine: {n} \
                             (known: scaleway_garnet, scaleway_sirius, scaleway_emerald)"
                        )));
                    }
                };
                let secret_key = std::env::var("SCALEWAY_SECRET_KEY").map_err(|_| {
                    HalError::Configuration(
                        "SCALEWAY_SECRET_KEY not set — get it from \
                         console.scaleway.com → API keys"
                            .into(),
                    )
                })?;
                let project_id = std::env::var("SCALEWAY_PROJECT_ID")
                    .map_err(|_| HalError::Configuration("SCALEWAY_PROJECT_ID not set".into()))?;
                let session_id = std::env::var("SCALEWAY_SESSION_ID").map_err(|_| {
                    HalError::Configuration(
                        "SCALEWAY_SESSION_ID not set — create a session at \
                         console.scaleway.com → Quantum Computing → Sessions"
                            .into(),
                    )
                })?;
                let backend = arvak_adapter_scaleway::ScalewayBackend::with_credentials(
                    secret_key, project_id, session_id, platform,
                )
                .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-scaleway"))]
            {
                Err(HalError::Configuration(format!(
                    "Scaleway adapter not compiled in for backend {n} (enable 'adapter-scaleway' feature)"
                )))
            }
        }
        n if n.starts_with("iqm_") => {
            #[cfg(feature = "adapter-iqm")]
            {
                // Strip the "iqm_" prefix to get the target name the
                // native adapter expects ("garnet", "sirius", etc.).
                let target = &n["iqm_".len()..];
                if target.is_empty() {
                    return Err(HalError::Configuration(format!(
                        "invalid IQM backend name: {n} (expected 'iqm_<target>', e.g. 'iqm_garnet')"
                    )));
                }
                let backend = arvak_adapter_iqm::IqmBackend::with_target(target)
                    .map_err(|e| HalError::Backend(e.to_string()))?;
                Ok(Arc::new(backend))
            }
            #[cfg(not(feature = "adapter-iqm"))]
            {
                Err(HalError::Configuration(format!(
                    "IQM adapter not compiled in for backend {n} (enable 'adapter-iqm' feature)"
                )))
            }
        }
        other => Err(HalError::Configuration(format!(
            "unknown backend: {other} (known: {})",
            known_backends().join(", ")
        ))),
    }
}

/// List backend names known to this build.
///
/// Reports the *canonical* names a caller can pass to
/// [`make_backend`]. IQM backends use the `iqm_<target>` form.
fn known_backends() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    #[cfg(feature = "simulator")]
    {
        v.push("sim".into());
    }
    #[cfg(feature = "adapter-iqm")]
    {
        // IQM Resonance Cloud — token auth via IQM_TOKEN
        v.push("iqm_garnet".into());
        v.push("iqm_sirius".into());
        v.push("iqm_emerald".into());
        v.push("iqm_crystal".into());
        // LUMI Helmi at CSC — OIDC auth via cached token
        v.push("iqm_helmi".into());
        // LRZ-hosted IQM machines — OIDC auth; users append machine name
        // (e.g. iqm_lrz_<machine>). The bare "iqm_lrz_" form is invalid.
    }
    #[cfg(feature = "adapter-scaleway")]
    {
        v.push("scaleway_garnet".into());
        v.push("scaleway_sirius".into());
        v.push("scaleway_emerald".into());
    }
    #[cfg(feature = "adapter-quantinuum")]
    {
        v.push("quantinuum_h2".into());
        v.push("quantinuum_h2_emulator".into());
        v.push("quantinuum_h1_emulator".into());
    }
    #[cfg(feature = "adapter-aqt")]
    {
        v.push("aqt_offline_sim".into());
        v.push("aqt_noise_sim".into());
        v.push("aqt_cloud_sim".into());
    }
    #[cfg(feature = "adapter-ionq")]
    {
        v.push("ionq_simulator".into());
        v.push("ionq_aria_1".into());
        v.push("ionq_aria_2".into());
        v.push("ionq_forte_1".into());
    }
    #[cfg(feature = "adapter-quandela")]
    {
        v.push("quandela_ascella_sim".into());
        v.push("quandela_ascella".into());
        v.push("quandela_belenos_sim".into());
        v.push("quandela_belenos".into());
        v.push("quandela_altair".into());
    }
    #[cfg(feature = "adapter-braket")]
    {
        // Amazon managed simulators
        v.push("braket_sv1".into());
        v.push("braket_tn1".into());
        v.push("braket_dm1".into());
        // QPUs (auth + S3 bucket required at construction)
        v.push("braket_rigetti_ankaa".into());
        v.push("braket_ionq_aria_1".into());
        v.push("braket_ionq_aria_2".into());
        v.push("braket_ionq_forte_1".into());
        v.push("braket_iqm_garnet".into());
    }
    #[cfg(feature = "adapter-cudaq")]
    {
        v.push("cudaq_mqpu".into());
        v.push("cudaq_custatevec".into());
        v.push("cudaq_tensornet".into());
        v.push("cudaq_density_matrix".into());
    }
    #[cfg(feature = "adapter-ddsim")]
    {
        v.push("ddsim".into());
    }
    #[cfg(feature = "adapter-ibm")]
    {
        // IBM aggressively retires older devices (typically 6–18 months
        // after release). The names below were confirmed reachable
        // on 2026-06-25 against current US-East and Frankfurt CRNs.
        // Six older 127q Eagle systems (Kyoto, Osaka, Brisbane,
        // Sherbrooke, Nazca, Torino) were retired during 2025 and
        // have been pruned from this list — even though make_backend()
        // would still attempt them on request, surfacing
        // "Backend not available" via the IBM Cloud API.
        //
        // The make_backend() prefix branch accepts any `ibm_*` name
        // not listed here, so a future device or a name we missed
        // still works without a code change — list_backends() just
        // won't advertise it.
        //
        // US-East Heron r3 (156q):
        v.push("ibm_fez".into());
        v.push("ibm_marrakesh".into());
        // EU Frankfurt (require IBM_SERVICE_CRN_EU). Brussels and
        // Strasbourg are 127q Eagles, Aachen is a 156q Heron r3.
        v.push("ibm_brussels".into());
        v.push("ibm_strasbourg".into());
        v.push("ibm_aachen".into());
    }
    v
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Construct a native backend by name.
///
/// # Example
/// ```python
/// import arvak
/// backend = arvak.backend_for("sim")
/// result = backend.run(qasm_str, shots=1024)
/// print(result.counts)
/// ```
#[pyfunction]
pub fn backend_for(name: &str) -> PyResult<PyBackend> {
    let inner = make_backend(name).map_err(hal_to_py_err)?;
    Ok(PyBackend { inner })
}

/// Return a list of backend names known to this build of `arvak`.
#[pyfunction]
pub fn list_backends(py: Python<'_>) -> PyResult<Py<PyList>> {
    let names = known_backends();
    let list = PyList::empty(py);
    for n in names {
        list.append(n)?;
    }
    Ok(list.into())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBackend>()?;
    m.add_class::<PyJobHandle>()?;
    m.add_class::<PyCapabilities>()?;
    m.add_class::<PyAvailability>()?;
    m.add_class::<PyValidationResult>()?;
    m.add_class::<PyJobStatus>()?;
    m.add_class::<PyExecutionResult>()?;
    m.add_function(wrap_pyfunction!(backend_for, m)?)?;
    m.add_function(wrap_pyfunction!(list_backends, m)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "simulator")]
    fn known_backends_lists_sim() {
        let names = known_backends();
        assert!(names.iter().any(|n| n == "sim"));
    }

    #[test]
    #[cfg(feature = "simulator")]
    fn make_backend_sim_succeeds() {
        let b = match make_backend("sim") {
            Ok(b) => b,
            Err(e) => panic!("sim backend should build: {e}"),
        };
        assert!(b.capabilities().is_simulator);
    }

    #[test]
    fn make_backend_unknown_fails() {
        match make_backend("nope-not-a-backend") {
            Ok(_) => panic!("expected unknown backend to fail"),
            Err(e) => assert!(e.to_string().contains("unknown backend")),
        }
    }

    #[test]
    fn job_status_terminal_classification() {
        assert!(!PyJobStatus::from(JobStatus::Queued).is_terminal());
        assert!(!PyJobStatus::from(JobStatus::Running).is_terminal());
        assert!(PyJobStatus::from(JobStatus::Completed).is_terminal());
        assert!(PyJobStatus::from(JobStatus::Failed("x".into())).is_terminal());
        assert!(PyJobStatus::from(JobStatus::Cancelled).is_terminal());
    }

    #[test]
    fn validation_result_invalid_carries_reasons() {
        let v: PyValidationResult = ValidationResult::Invalid {
            reasons: vec!["too many qubits".into()],
        }
        .into();
        assert!(!v.valid);
        assert!(!v.requires_transpilation);
        assert_eq!(v.reasons.len(), 1);
    }

    #[test]
    fn availability_unavailable_message_preserved() {
        let a: PyAvailability = BackendAvailability::unavailable("maintenance").into();
        assert!(!a.is_available);
        assert_eq!(a.status_message.as_deref(), Some("maintenance"));
    }
}
