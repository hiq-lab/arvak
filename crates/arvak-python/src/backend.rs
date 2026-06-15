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
    /// Uses the HAL `wait()` default implementation (500 ms poll interval,
    /// 5-minute timeout). For shot-only simulators this returns essentially
    /// immediately.
    fn result(&self, py: Python<'_>) -> PyResult<PyExecutionResult> {
        let backend = self.backend.clone();
        let job_id = self.job_id.clone();
        let res = py
            .detach(move || runtime().block_on(async move { backend.wait(&job_id).await }))
            .map_err(hal_to_py_err)?;
        Ok(PyExecutionResult {
            inner: Arc::new(res),
        })
    }

    /// Alias for [`Self::result`] (Qiskit `JobV1` compat).
    fn wait(&self, py: Python<'_>) -> PyResult<PyExecutionResult> {
        self.result(py)
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
    /// Equivalent to `backend.submit(qasm, shots, parameters).result()`.
    #[pyo3(signature = (qasm, shots=1024, parameters=None))]
    fn run(
        &self,
        qasm: &str,
        shots: u32,
        parameters: Option<HashMap<String, f64>>,
        py: Python<'_>,
    ) -> PyResult<PyExecutionResult> {
        let handle = self.submit(qasm, shots, parameters, py)?;
        handle.result(py)
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

/// Construct a backend by name. Phase 1 supports `"sim"` only.
///
/// Future phases will add IBM/IQM/Quantinuum/etc. behind feature flags.
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
        other => Err(HalError::Configuration(format!(
            "unknown backend: {other} (known: {})",
            known_backends().join(", ")
        ))),
    }
}

/// List backend names known to this build.
fn known_backends() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    #[cfg(feature = "simulator")]
    {
        v.push("sim".into());
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
