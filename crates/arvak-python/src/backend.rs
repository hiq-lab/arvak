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
    #[cfg(feature = "adapter-ibm")]
    {
        // US-East
        v.push("ibm_torino".into());
        v.push("ibm_fez".into());
        v.push("ibm_marrakesh".into());
        v.push("ibm_brisbane".into());
        v.push("ibm_kyoto".into());
        v.push("ibm_osaka".into());
        v.push("ibm_sherbrooke".into());
        v.push("ibm_nazca".into());
        // EU (Frankfurt)
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
