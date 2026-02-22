//! QDMI Backend implementation.
//!
//! This module provides an Arvak Backend implementation that communicates
//! with quantum devices via the QDMI (Quantum Device Management Interface).

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, RwLock};
#[cfg(feature = "system-qdmi")]
use tracing::warn;
use tracing::{debug, info};

use arvak_hal::backend::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, ValidationResult,
};
use arvak_hal::capability::{Capabilities, GateSet, Topology};
use arvak_hal::error::{HalError, HalResult};
use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::{Counts, ExecutionResult};
use arvak_ir::Circuit;

use crate::error::{QdmiError, QdmiResult};
use crate::ffi::{QdmiDeviceStatus, QdmiJobStatus};

#[cfg(not(feature = "system-qdmi"))]
use crate::ffi::mock::{MockDevice, MockJob, MockSession};

#[cfg(feature = "system-qdmi")]
use crate::ffi::{
    self, QdmiDevice, QdmiJob, QdmiJobParameter, QdmiJobResult, QdmiProgramFormat, QdmiSession,
    QdmiSessionParameter, QdmiSessionProperty,
};

#[cfg(feature = "system-qdmi")]
use std::ffi::{CString, c_int, c_void};

/// QDMI Backend for Arvak.
///
/// This backend connects to quantum devices via the QDMI interface,
/// which is part of the Munich Quantum Software Stack (MQSS).
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_qdmi::QdmiBackend;
/// use arvak_hal::Backend;
///
/// let backend = QdmiBackend::new()
///     .with_token("your-api-token")
///     .with_base_url("https://qdmi.lrz.de");
///
/// // Get device capabilities (sync, infallible)
/// let caps = backend.capabilities();
/// println!("Device: {} with {} qubits", caps.name, caps.num_qubits);
///
/// // Submit a circuit
/// let circuit = Circuit::bell()?;
/// let job_id = backend.submit(&circuit, 1000).await?;
/// let result = backend.wait(&job_id).await?;
/// ```
pub struct QdmiBackend {
    /// Configuration
    config: BackendConfig,

    /// Internal state (mock mode)
    #[cfg(not(feature = "system-qdmi"))]
    state: Arc<RwLock<MockState>>,

    /// Internal state (system QDMI mode)
    #[cfg(feature = "system-qdmi")]
    state: Arc<RwLock<SystemState>>,

    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
}

/// Mock state for testing without system QDMI
#[cfg(not(feature = "system-qdmi"))]
#[derive(Default)]
struct MockState {
    session: Option<MockSession>,
    device: Option<MockDevice>,
    jobs: FxHashMap<String, MockJob>,
}

/// System QDMI state holding FFI handles
#[cfg(feature = "system-qdmi")]
struct SystemState {
    session: *mut QdmiSession,
    device: *mut QdmiDevice,
    jobs: FxHashMap<String, *mut QdmiJob>,
    initialized: bool,
}

#[cfg(feature = "system-qdmi")]
impl Default for SystemState {
    fn default() -> Self {
        Self {
            session: std::ptr::null_mut(),
            device: std::ptr::null_mut(),
            jobs: FxHashMap::default(),
            initialized: false,
        }
    }
}

// SAFETY: `SystemState` contains raw pointers (`*mut QdmiSession`, `*mut QdmiDevice`,
// `*mut QdmiJob`) which are not `Send`/`Sync` by default. We assert these traits are
// safe because:
//
// 1. All access to these pointers is gated behind an `RwLock<SystemState>`, ensuring
//    mutual exclusion for writes and shared access only for reads.
// 2. The QDMI specification (v1.2.1, Section 4.2 "Thread Safety") guarantees that
//    session handles are safe to use from any thread, provided that concurrent
//    mutations are externally synchronized — which the `RwLock` provides.
// 3. The pointers are opaque handles into the QDMI C library and are never
//    dereferenced on the Rust side; all dereferences happen inside the FFI calls.
// 4. The `Drop` implementation ensures pointers are freed exactly once under the
//    write lock, preventing use-after-free across threads.
#[cfg(feature = "system-qdmi")]
unsafe impl Send for SystemState {}
#[cfg(feature = "system-qdmi")]
unsafe impl Sync for SystemState {}

// ============================================================================
// Shared Helpers
// ============================================================================

/// Build a [`GateSet`] from a list of `(operation_name, num_qubits)` pairs
/// discovered at runtime (e.g., via QDMI device property queries).
///
/// All discovered operations are treated as native — QDMI drivers only
/// advertise gates the device can execute without further decomposition.
fn gate_set_from_operations(ops: &[(String, usize)]) -> GateSet {
    let mut single_qubit: Vec<String> = Vec::new();
    let mut two_qubit: Vec<String> = Vec::new();
    let mut three_qubit: Vec<String> = Vec::new();
    let mut native: Vec<String> = Vec::new();

    for (name, arity) in ops {
        match arity {
            1 => {
                single_qubit.push(name.clone());
                native.push(name.clone());
            }
            2 => {
                two_qubit.push(name.clone());
                native.push(name.clone());
            }
            3 => {
                three_qubit.push(name.clone());
                native.push(name.clone());
            }
            _ => {} // Arity 0 (barriers, etc.) and ≥4 are not represented in GateSet
        }
    }

    GateSet {
        single_qubit,
        two_qubit,
        three_qubit,
        native,
    }
}

// ============================================================================
// Mock Mode Implementation
// ============================================================================

#[cfg(not(feature = "system-qdmi"))]
impl QdmiBackend {
    /// Create a new QDMI backend.
    pub fn new() -> Self {
        // Default capabilities — updated when initialize() is called.
        let capabilities = Capabilities {
            name: "QDMI Mock Device".into(),
            num_qubits: 20,
            gate_set: GateSet::universal(),
            topology: Topology::full(20),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["qdmi".into(), "mqss".into()],
            noise_profile: None,
            cooling_profile: None,
        };
        Self {
            config: BackendConfig::new("qdmi"),
            state: Arc::new(RwLock::new(MockState::default())),
            capabilities,
        }
    }

    /// Set the authentication token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.config.token = Some(token.into());
        self
    }

    /// Set the base URL for the QDMI endpoint.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.endpoint = Some(url.into());
        self
    }

    /// Initialize the QDMI session (mock mode).
    pub fn initialize(&self) -> QdmiResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        // Create mock session
        let mut session = MockSession::new();
        if let Some(ref token) = self.config.token {
            session.token = Some(token.clone());
        }
        if let Some(ref endpoint) = self.config.endpoint {
            session.base_url = Some(endpoint.clone());
        }
        session.initialized = true;

        // Create mock device
        let device = MockDevice::new("QDMI Mock Device", 20);

        state.session = Some(session);
        state.device = Some(device);

        info!("QDMI session initialized (mock mode)");
        Ok(())
    }

    /// Convert an Arvak circuit to QASM3 for QDMI submission.
    fn circuit_to_qasm3(&self, circuit: &Circuit) -> QdmiResult<String> {
        arvak_qasm3::emit(circuit).map_err(|e| QdmiError::CircuitConversion(e.to_string()))
    }

    /// Parse QDMI results into Arvak Counts.
    #[allow(dead_code)]
    fn parse_results(&self, hist_keys: &[String], hist_values: &[u64]) -> Counts {
        let mut counts = Counts::new();
        for (key, &value) in hist_keys.iter().zip(hist_values.iter()) {
            counts.insert(key.clone(), value);
        }
        counts
    }

    /// Convert QDMI job status to Arvak job status.
    fn convert_job_status(&self, qdmi_status: QdmiJobStatus) -> JobStatus {
        match qdmi_status {
            QdmiJobStatus::Created | QdmiJobStatus::Submitted | QdmiJobStatus::Queued => {
                JobStatus::Queued
            }
            QdmiJobStatus::Running => JobStatus::Running,
            QdmiJobStatus::Done => JobStatus::Completed,
            QdmiJobStatus::Canceled => JobStatus::Cancelled,
            QdmiJobStatus::Failed => JobStatus::Failed("Job failed".into()),
        }
    }

    /// Build capabilities from QDMI device properties (mock mode).
    ///
    /// Reads `MockDevice::operations` and `MockDevice::coupling_map` to
    /// build a realistic `GateSet` and `Topology` — the same logic that the
    /// system-QDMI path will use once a real driver is available.
    fn build_capabilities(&self) -> QdmiResult<Capabilities> {
        let state = self
            .state
            .read()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        let device = state.device.as_ref().ok_or(QdmiError::NoDevice)?;

        let num_qubits = u32::try_from(device.num_qubits).unwrap_or(u32::MAX);

        // Build gate set from device-reported operations.
        let ops: Vec<(String, usize)> = device
            .operations
            .iter()
            .map(|op| (op.name.clone(), op.num_qubits))
            .collect();
        let gate_set = if ops.is_empty() {
            GateSet::universal()
        } else {
            gate_set_from_operations(&ops)
        };

        // Build topology from device-reported coupling map.
        let topology = if device.coupling_map.is_empty() {
            Topology::full(num_qubits)
        } else {
            Topology::custom(device.coupling_map.clone())
        };

        Ok(Capabilities {
            name: device.name.clone(),
            num_qubits,
            gate_set,
            topology,
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["qdmi".into(), "mqss".into()],
            noise_profile: None,
            cooling_profile: None,
        })
    }
}

// ============================================================================
// System QDMI Implementation
// ============================================================================

#[cfg(feature = "system-qdmi")]
impl QdmiBackend {
    /// Create a new QDMI backend.
    pub fn new() -> Self {
        // Default capabilities — updated when initialize() discovers the device.
        let capabilities = Capabilities {
            name: "QDMI Device".into(),
            num_qubits: 0,
            gate_set: GateSet::universal(),
            topology: Topology::full(1),
            max_shots: 100_000,
            max_circuit_ops: None,
            is_simulator: false,
            features: vec!["qdmi".into(), "mqss".into(), "system".into()],
            noise_profile: None,
            cooling_profile: None,
        };
        Self {
            config: BackendConfig::new("qdmi"),
            state: Arc::new(RwLock::new(SystemState::default())),
            capabilities,
        }
    }

    /// Set the authentication token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.config.token = Some(token.into());
        self
    }

    /// Set the base URL for the QDMI endpoint.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.endpoint = Some(url.into());
        self
    }

    /// Initialize the QDMI session via FFI.
    ///
    /// Performs the following steps:
    /// 1. Allocate a new QDMI session
    /// 2. Set session parameters (token, base URL)
    /// 3. Initialize the session (connects to the backend)
    /// 4. Discover available devices
    pub fn initialize(&self) -> QdmiResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        if state.initialized {
            return Ok(());
        }

        unsafe {
            // 1. Allocate session
            let mut session: *mut QdmiSession = std::ptr::null_mut();
            let status = ffi::QDMI_session_alloc(&mut session);
            ffi::check_status(status)
                .map_err(|s| QdmiError::Ffi(format!("session_alloc failed: {s:?}")))?;

            if session.is_null() {
                return Err(QdmiError::Ffi("session_alloc returned null".into()));
            }

            // 2. Set session parameters
            // Note: In QDMI v1.2.1, BaseUrl moved to device-session layer.
            // The client-session layer uses Token, AuthFile, etc.
            if let Some(ref token) = self.config.token {
                let c_token = CString::new(token.as_str())
                    .map_err(|e| QdmiError::InvalidParameter(e.to_string()))?;
                let token_bytes = c_token.as_bytes_with_nul();
                let status = ffi::QDMI_session_set_parameter(
                    session,
                    QdmiSessionParameter::Token as c_int,
                    token_bytes.len(),
                    c_token.as_ptr() as *const c_void,
                );
                ffi::check_status(status)
                    .map_err(|s| QdmiError::Ffi(format!("set Token failed: {s:?}")))?;
            }

            // 3. Initialize session
            let status = ffi::QDMI_session_init(session);
            ffi::check_status(status)
                .map_err(|s| QdmiError::Ffi(format!("session_init failed: {s:?}")))?;

            // 4. Discover devices via session property query (buffer-query pattern)
            // First call: get required buffer size.
            // The QDMI spec states that the first call (with size=0) may return
            // ErrorInvalidArgument while still writing the required size via size_ret;
            // so we intentionally ignore the status here and check devices_size instead.
            let mut devices_size: usize = 0;
            let _status = ffi::QDMI_session_query_session_property(
                session,
                QdmiSessionProperty::Devices as c_int,
                0,
                std::ptr::null_mut(),
                &mut devices_size,
            );
            if devices_size == 0 {
                ffi::QDMI_session_free(session);
                return Err(QdmiError::NoDevice);
            }

            // Second call: retrieve device pointers
            let device_count = devices_size / std::mem::size_of::<*mut QdmiDevice>();
            let mut device_ptrs: Vec<*mut QdmiDevice> = vec![std::ptr::null_mut(); device_count];
            let status = ffi::QDMI_session_query_session_property(
                session,
                QdmiSessionProperty::Devices as c_int,
                devices_size,
                device_ptrs.as_mut_ptr() as *mut c_void,
                &mut devices_size,
            );
            ffi::check_status(status)
                .map_err(|s| QdmiError::Ffi(format!("query devices failed: {s:?}")))?;

            if device_ptrs.is_empty() || device_ptrs[0].is_null() {
                ffi::QDMI_session_free(session);
                return Err(QdmiError::NoDevice);
            }

            // Use the first available device
            state.session = session;
            state.device = device_ptrs[0];
            state.initialized = true;

            info!("QDMI session initialized with {} device(s)", device_count);
        }

        Ok(())
    }

    /// Convert an Arvak circuit to QASM3 for QDMI submission.
    fn circuit_to_qasm3(&self, circuit: &Circuit) -> QdmiResult<String> {
        arvak_qasm3::emit(circuit).map_err(|e| QdmiError::CircuitConversion(e.to_string()))
    }

    /// Parse QDMI results into Arvak Counts.
    fn parse_results(&self, hist_keys: &[String], hist_values: &[u64]) -> Counts {
        let mut counts = Counts::new();
        for (key, &value) in hist_keys.iter().zip(hist_values.iter()) {
            counts.insert(key.clone(), value);
        }
        counts
    }

    /// Convert QDMI job status to Arvak job status.
    fn convert_job_status(&self, qdmi_status: QdmiJobStatus) -> JobStatus {
        match qdmi_status {
            QdmiJobStatus::Created | QdmiJobStatus::Submitted | QdmiJobStatus::Queued => {
                JobStatus::Queued
            }
            QdmiJobStatus::Running => JobStatus::Running,
            QdmiJobStatus::Done => JobStatus::Completed,
            QdmiJobStatus::Canceled => JobStatus::Cancelled,
            QdmiJobStatus::Failed => JobStatus::Failed("Job failed".into()),
        }
    }

    /// Build capabilities by querying QDMI device properties via FFI.
    ///
    /// Queries:
    /// - `Name` and `QubitsNum` — device identity
    /// - `Operations` + per-operation `Name`/`QubitsNum` → dynamic `GateSet`
    /// - `CouplingMap` + per-site `Index` queries → dynamic `Topology`
    fn build_capabilities(&self) -> QdmiResult<Capabilities> {
        let state = self
            .state
            .read()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        if !state.initialized || state.device.is_null() {
            return Err(QdmiError::NoDevice);
        }

        unsafe {
            use crate::ffi::QdmiDeviceProperty;

            // ── Device name ──────────────────────────────────────────────────
            let mut name_size: usize = 0;
            let _status = ffi::QDMI_device_query_device_property(
                state.device,
                QdmiDeviceProperty::Name as c_int,
                0,
                std::ptr::null_mut(),
                &mut name_size,
            );
            let device_name = if name_size > 0 {
                let mut name_buf = vec![0u8; name_size];
                let status = ffi::QDMI_device_query_device_property(
                    state.device,
                    QdmiDeviceProperty::Name as c_int,
                    name_size,
                    name_buf.as_mut_ptr() as *mut c_void,
                    &mut name_size,
                );
                if ffi::check_status(status).is_ok() {
                    String::from_utf8_lossy(&name_buf[..name_size.saturating_sub(1)]).to_string()
                } else {
                    "QDMI Device".to_string()
                }
            } else {
                "QDMI Device".to_string()
            };

            // ── Qubit count ──────────────────────────────────────────────────
            let mut num_qubits_raw: usize = 0;
            let mut qubits_size = std::mem::size_of::<usize>();
            let status = ffi::QDMI_device_query_device_property(
                state.device,
                QdmiDeviceProperty::QubitsNum as c_int,
                qubits_size,
                &mut num_qubits_raw as *mut usize as *mut c_void,
                &mut qubits_size,
            );
            let num_qubits = if ffi::check_status(status).is_ok() {
                u32::try_from(num_qubits_raw).unwrap_or_else(|_| {
                    warn!("Qubit count {} overflows u32; using 0", num_qubits_raw);
                    0
                })
            } else {
                warn!("Failed to query qubit count, defaulting to 0");
                0
            };

            // ── Gate set (from QDMI Operations property) ─────────────────────
            let gate_set = query_gate_set_from_qdmi(state.device, num_qubits);

            // ── Topology (from QDMI CouplingMap property) ────────────────────
            let topology = query_topology_from_qdmi(state.device, num_qubits);

            Ok(Capabilities {
                name: device_name,
                num_qubits,
                gate_set,
                topology,
                max_shots: 100_000,
                max_circuit_ops: None,
                is_simulator: false,
                features: vec!["qdmi".into(), "mqss".into(), "system".into()],
                noise_profile: None,
                cooling_profile: None,
            })
        }
    }
}

#[cfg(feature = "system-qdmi")]
impl Drop for QdmiBackend {
    fn drop(&mut self) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe {
            // Free all outstanding jobs
            for (_id, job_ptr) in state.jobs.drain() {
                if !job_ptr.is_null() {
                    ffi::QDMI_job_free(job_ptr);
                }
            }

            // Free session (which implicitly frees associated devices)
            if !state.session.is_null() {
                ffi::QDMI_session_free(state.session);
                state.session = std::ptr::null_mut();
                state.device = std::ptr::null_mut();
            }
        }
    }
}

impl Default for QdmiBackend {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// System QDMI Capability Query Helpers
// ============================================================================

/// Query the gate set from QDMI by enumerating all supported operations.
///
/// Uses `QDMI_DEVICE_PROPERTY_OPERATIONS` to get a list of `QdmiOperation*`
/// pointers, then queries `Name` and `QubitsNum` for each. Falls back to
/// `GateSet::universal()` if the device doesn't support the Operations
/// property or returns no operations.
#[cfg(feature = "system-qdmi")]
unsafe fn query_gate_set_from_qdmi(device: *mut ffi::QdmiDevice, _num_qubits: u32) -> GateSet {
    use std::ffi::{c_int, c_void};

    // First call: get required buffer size for operation pointer array.
    let mut ops_size: usize = 0;
    let _status = ffi::QDMI_device_query_device_property(
        device,
        ffi::QdmiDeviceProperty::Operations as c_int,
        0,
        std::ptr::null_mut(),
        &mut ops_size,
    );
    if ops_size == 0 {
        return GateSet::universal();
    }

    // Second call: retrieve operation pointers.
    let op_count = ops_size / std::mem::size_of::<*mut ffi::QdmiOperation>();
    let mut op_ptrs: Vec<*mut ffi::QdmiOperation> = vec![std::ptr::null_mut(); op_count];
    let status = ffi::QDMI_device_query_device_property(
        device,
        ffi::QdmiDeviceProperty::Operations as c_int,
        ops_size,
        op_ptrs.as_mut_ptr() as *mut c_void,
        &mut ops_size,
    );
    if ffi::check_status(status).is_err() {
        return GateSet::universal();
    }

    let mut ops: Vec<(String, usize)> = Vec::with_capacity(op_count);
    for &op_ptr in &op_ptrs {
        if op_ptr.is_null() {
            continue;
        }

        // Query operation name.
        let mut name_size: usize = 0;
        let _s = ffi::QDMI_device_query_operation_property(
            device,
            op_ptr,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            ffi::QdmiOperationProperty::Name as c_int,
            0,
            std::ptr::null_mut(),
            &mut name_size,
        );
        if name_size == 0 {
            continue;
        }
        let mut name_buf = vec![0u8; name_size];
        let status = ffi::QDMI_device_query_operation_property(
            device,
            op_ptr,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            ffi::QdmiOperationProperty::Name as c_int,
            name_size,
            name_buf.as_mut_ptr() as *mut c_void,
            &mut name_size,
        );
        if ffi::check_status(status).is_err() {
            continue;
        }
        let op_name = String::from_utf8_lossy(&name_buf[..name_size.saturating_sub(1)]).to_string();

        // Query qubit arity.
        let mut qn: usize = 0;
        let mut qn_size = std::mem::size_of::<usize>();
        let status = ffi::QDMI_device_query_operation_property(
            device,
            op_ptr,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            ffi::QdmiOperationProperty::QubitsNum as c_int,
            qn_size,
            &mut qn as *mut usize as *mut c_void,
            &mut qn_size,
        );
        if ffi::check_status(status).is_ok() {
            ops.push((op_name, qn));
        }
    }

    if ops.is_empty() {
        GateSet::universal()
    } else {
        gate_set_from_operations(&ops)
    }
}

/// Query the coupling map from QDMI and build a [`Topology`].
///
/// Uses `QDMI_DEVICE_PROPERTY_COUPLING_MAP` which returns a flattened array
/// of `(QdmiSite*, QdmiSite*)` pairs. For each pair, the site index is read
/// via `QDMI_SITE_PROPERTY_INDEX`. Falls back to `Topology::full(num_qubits)`
/// if the property is unsupported or returns no edges.
#[cfg(feature = "system-qdmi")]
unsafe fn query_topology_from_qdmi(device: *mut ffi::QdmiDevice, num_qubits: u32) -> Topology {
    use std::ffi::{c_int, c_void};

    let mut cm_size: usize = 0;
    let _status = ffi::QDMI_device_query_device_property(
        device,
        ffi::QdmiDeviceProperty::CouplingMap as c_int,
        0,
        std::ptr::null_mut(),
        &mut cm_size,
    );
    if cm_size == 0 {
        return Topology::full(num_qubits);
    }

    let site_count = cm_size / std::mem::size_of::<*mut ffi::QdmiSite>();
    if site_count % 2 != 0 {
        // Coupling map must be flattened pairs — odd count is malformed.
        return Topology::full(num_qubits);
    }

    let mut site_ptrs: Vec<*mut ffi::QdmiSite> = vec![std::ptr::null_mut(); site_count];
    let status = ffi::QDMI_device_query_device_property(
        device,
        ffi::QdmiDeviceProperty::CouplingMap as c_int,
        cm_size,
        site_ptrs.as_mut_ptr() as *mut c_void,
        &mut cm_size,
    );
    if ffi::check_status(status).is_err() {
        return Topology::full(num_qubits);
    }

    let pair_count = site_count / 2;
    let mut edges: Vec<(u32, u32)> = Vec::with_capacity(pair_count);
    for pair_idx in 0..pair_count {
        let site_a = site_ptrs[pair_idx * 2];
        let site_b = site_ptrs[pair_idx * 2 + 1];
        if site_a.is_null() || site_b.is_null() {
            continue;
        }
        let idx_a = query_site_index(device, site_a);
        let idx_b = query_site_index(device, site_b);
        if let (Some(a), Some(b)) = (idx_a, idx_b) {
            edges.push((a, b));
        }
    }

    if edges.is_empty() {
        Topology::full(num_qubits)
    } else {
        Topology::custom(edges)
    }
}

/// Query the index of a QDMI site via `QDMI_SITE_PROPERTY_INDEX`.
#[cfg(feature = "system-qdmi")]
unsafe fn query_site_index(device: *mut ffi::QdmiDevice, site: *mut ffi::QdmiSite) -> Option<u32> {
    use std::ffi::{c_int, c_void};

    let mut idx: usize = 0;
    let mut size = std::mem::size_of::<usize>();
    let status = ffi::QDMI_device_query_site_property(
        device,
        site,
        ffi::QdmiSiteProperty::Index as c_int,
        size,
        &mut idx as *mut usize as *mut c_void,
        &mut size,
    );
    if ffi::check_status(status).is_ok() {
        u32::try_from(idx).ok()
    } else {
        None
    }
}

#[async_trait]
impl Backend for QdmiBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            if let Some(ref device) = state.device {
                let available = matches!(
                    device.status,
                    QdmiDeviceStatus::Idle | QdmiDeviceStatus::Busy
                );
                if available {
                    return Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: None,
                    });
                }
                return Ok(BackendAvailability::unavailable("device not idle"));
            }

            // Try to initialize
            drop(state);
            if self.initialize().is_ok() {
                return Ok(BackendAvailability {
                    is_available: true,
                    queue_depth: None,
                    estimated_wait: None,
                    status_message: None,
                });
            }
        }

        #[cfg(feature = "system-qdmi")]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            if state.initialized && !state.device.is_null() {
                unsafe {
                    use crate::ffi::QdmiDeviceProperty;
                    let mut status_val: c_int = 0;
                    let mut size = std::mem::size_of::<c_int>();
                    let result = ffi::QDMI_device_query_device_property(
                        state.device,
                        QdmiDeviceProperty::Status as c_int,
                        size,
                        &mut status_val as *mut c_int as *mut c_void,
                        &mut size,
                    );
                    if ffi::check_status(result).is_ok() {
                        let device_status = QdmiDeviceStatus::from(status_val);
                        let available = matches!(
                            device_status,
                            QdmiDeviceStatus::Idle | QdmiDeviceStatus::Busy
                        );
                        if available {
                            return Ok(BackendAvailability {
                                is_available: true,
                                queue_depth: None,
                                estimated_wait: None,
                                status_message: None,
                            });
                        }
                        return Ok(BackendAvailability::unavailable("device not idle"));
                    }
                }
            }

            // Try to initialize
            drop(state);
            if self.initialize().is_ok() {
                return Ok(BackendAvailability {
                    is_available: true,
                    queue_depth: None,
                    estimated_wait: None,
                    status_message: None,
                });
            }
        }

        Ok(BackendAvailability::unavailable("failed to initialize"))
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if caps.num_qubits > 0 && circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but device only supports {}",
                circuit.num_qubits(),
                caps.num_qubits
            ));
        }

        // Check gate set support
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if !gate_set.contains(name) {
                    reasons.push(format!("Unsupported gate: {}", name));
                    break;
                }
            }
        }

        if reasons.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid { reasons })
        }
    }

    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        debug!("Submitting circuit with {} shots via QDMI", shots);

        // Initialize if needed
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;
            if state.session.is_none() {
                drop(state);
                self.initialize()
                    .map_err(|e| HalError::Backend(e.to_string()))?;
            }
        }

        // Convert circuit to QASM3
        let qasm = self
            .circuit_to_qasm3(circuit)
            .map_err(|e| HalError::Backend(e.to_string()))?;

        debug!("Circuit converted to QASM3 ({} bytes)", qasm.len());

        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            // Create mock job
            let mut job = MockJob::new();
            job.program = Some(qasm);
            job.shots = shots as usize;
            job.status = QdmiJobStatus::Submitted;

            let job_id = job.id.clone();
            state.jobs.insert(job_id.clone(), job);

            info!("Submitted job {} via QDMI (mock)", job_id);
            return Ok(JobId::new(job_id));
        }

        #[cfg(feature = "system-qdmi")]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            if !state.initialized || state.device.is_null() {
                return Err(HalError::Backend("QDMI not initialized".into()));
            }

            unsafe {
                // 1. Create job
                let mut job: *mut QdmiJob = std::ptr::null_mut();
                let status = ffi::QDMI_device_create_job(state.device, &mut job);
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("create_job failed: {s:?}")))?;

                if job.is_null() {
                    return Err(HalError::Backend("create_job returned null".into()));
                }

                // 2. Set program format (QASM3)
                let format = QdmiProgramFormat::Qasm3 as c_int;
                let status = ffi::QDMI_job_set_parameter(
                    job,
                    QdmiJobParameter::ProgramFormat as c_int,
                    std::mem::size_of::<c_int>(),
                    &format as *const c_int as *const c_void,
                );
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("set ProgramFormat failed: {s:?}")))?;

                // 3. Set program
                let c_qasm = CString::new(qasm.as_str())
                    .map_err(|e| HalError::Backend(format!("Invalid QASM string: {e}")))?;
                let qasm_bytes = c_qasm.as_bytes_with_nul();
                let status = ffi::QDMI_job_set_parameter(
                    job,
                    QdmiJobParameter::Program as c_int,
                    qasm_bytes.len(),
                    c_qasm.as_ptr() as *const c_void,
                );
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("set Program failed: {s:?}")))?;

                // 4. Set shots (QDMI expects size_t / usize)
                let shots_val = shots as usize;
                let status = ffi::QDMI_job_set_parameter(
                    job,
                    QdmiJobParameter::ShotsNum as c_int,
                    std::mem::size_of::<usize>(),
                    &shots_val as *const usize as *const c_void,
                );
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("set ShotsNum failed: {s:?}")))?;

                // 5. Submit
                let status = ffi::QDMI_job_submit(job);
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("job_submit failed: {s:?}")))?;

                // Generate a unique ID and store the job handle
                let job_id = uuid::Uuid::new_v4().to_string();
                state.jobs.insert(job_id.clone(), job);

                info!("Submitted job {} via QDMI (system)", job_id);
                return Ok(JobId::new(job_id));
            }
        }
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get_mut(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            // Simulate job progression
            match job.status {
                QdmiJobStatus::Submitted => {
                    job.status = QdmiJobStatus::Queued;
                }
                QdmiJobStatus::Queued => {
                    job.status = QdmiJobStatus::Running;
                }
                QdmiJobStatus::Running => {
                    // Generate mock results
                    let num_qubits = 2; // Simplified
                    let mut results = Vec::new();
                    for i in 0..job.shots {
                        // Simple mock: 50% |00⟩, 50% |11⟩
                        if i % 2 == 0 {
                            results.push("0".repeat(num_qubits));
                        } else {
                            results.push("1".repeat(num_qubits));
                        }
                    }
                    job.results = Some(results);
                    job.status = QdmiJobStatus::Done;
                }
                _ => {}
            }

            return Ok(self.convert_job_status(job.status));
        }

        #[cfg(feature = "system-qdmi")]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let &job_ptr = state
                .jobs
                .get(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            unsafe {
                let mut status_val: c_int = 0;
                let status = ffi::QDMI_job_check(job_ptr, &mut status_val);
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("job_check failed: {s:?}")))?;

                let qdmi_status = QdmiJobStatus::from(status_val);
                return Ok(self.convert_job_status(qdmi_status));
            }
        }
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            if !matches!(job.status, QdmiJobStatus::Done) {
                return Err(HalError::JobFailed("Job not completed".into()));
            }

            // Convert results to counts
            let mut counts = Counts::new();
            if let Some(ref results) = job.results {
                for result in results {
                    counts.insert(result.as_str(), 1);
                }
            }

            return Ok(ExecutionResult::new(counts, job.shots as u32));
        }

        #[cfg(feature = "system-qdmi")]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let &job_ptr = state
                .jobs
                .get(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            unsafe {
                // First get the size of histogram keys (buffer-query pattern)
                let mut keys_size: usize = 0;
                let _status = ffi::QDMI_job_get_results(
                    job_ptr,
                    QdmiJobResult::HistKeys as c_int,
                    0,
                    std::ptr::null_mut(),
                    &mut keys_size,
                );
                // Zero size means no results yet
                if keys_size == 0 {
                    return Err(HalError::JobFailed("No results available".into()));
                }

                // Second call: retrieve histogram keys
                let mut keys_buf = vec![0u8; keys_size];
                let status = ffi::QDMI_job_get_results(
                    job_ptr,
                    QdmiJobResult::HistKeys as c_int,
                    keys_size,
                    keys_buf.as_mut_ptr() as *mut c_void,
                    &mut keys_size,
                );
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("get HistKeys failed: {s:?}")))?;

                // Parse keys (null-separated C strings)
                let keys_str = String::from_utf8_lossy(&keys_buf[..keys_size]);
                let hist_keys: Vec<String> = keys_str
                    .split('\0')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();

                // Get histogram values (buffer-query pattern — two calls required).
                // QDMI returns size_t (usize) values; read into usize buffer
                // then convert to u64 for the Arvak Counts type.
                let mut values_size: usize = 0;
                let _status = ffi::QDMI_job_get_results(
                    job_ptr,
                    QdmiJobResult::HistValues as c_int,
                    0,
                    std::ptr::null_mut(),
                    &mut values_size,
                );
                if values_size == 0 {
                    return Err(HalError::Backend("No histogram values available".into()));
                }
                let num_values = values_size / std::mem::size_of::<usize>();
                let mut hist_values_raw = vec![0usize; num_values];
                let mut values_size_ret = values_size;
                let status = ffi::QDMI_job_get_results(
                    job_ptr,
                    QdmiJobResult::HistValues as c_int,
                    values_size,
                    hist_values_raw.as_mut_ptr() as *mut c_void,
                    &mut values_size_ret,
                );
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("get HistValues failed: {s:?}")))?;

                let hist_values: Vec<u64> = hist_values_raw.iter().map(|&v| v as u64).collect();
                let counts = self.parse_results(&hist_keys, &hist_values);
                let total_shots: u64 = hist_values.iter().sum();

                return Ok(ExecutionResult::new(counts, total_shots as u32));
            }
        }
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get_mut(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            job.status = QdmiJobStatus::Canceled;
            info!("Cancelled job {} via QDMI", job_id);
            return Ok(());
        }

        #[cfg(feature = "system-qdmi")]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let &job_ptr = state
                .jobs
                .get(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            unsafe {
                let status = ffi::QDMI_job_cancel(job_ptr);
                ffi::check_status(status)
                    .map_err(|s| HalError::Backend(format!("job_cancel failed: {s:?}")))?;
            }

            info!("Cancelled job {} via QDMI (system)", job_id);
            return Ok(());
        }
    }
}

impl BackendFactory for QdmiBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let mut backend = QdmiBackend::new();
        backend.config = config;

        // Auto-initialize if we have credentials, then refresh capabilities
        if backend.config.token.is_some() || backend.config.endpoint.is_some() {
            backend
                .initialize()
                .map_err(|e| HalError::Backend(e.to_string()))?;
            // Refresh capabilities after initialization
            if let Ok(caps) = backend.build_capabilities() {
                backend.capabilities = caps;
            }
        }

        Ok(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_qdmi_backend_creation() {
        let backend = QdmiBackend::new();
        assert_eq!(backend.name(), "qdmi");
    }

    #[tokio::test]
    async fn test_qdmi_backend_capabilities() {
        let backend = QdmiBackend::new();
        let caps = backend.capabilities();

        assert!(caps.num_qubits > 0);
        assert!(!caps.is_simulator);
        assert!(caps.features.contains(&"qdmi".to_string()));
    }

    #[tokio::test]
    async fn test_qdmi_backend_availability() {
        let backend = QdmiBackend::new();
        let avail = backend.availability().await.unwrap();
        assert!(avail.is_available);
    }

    #[tokio::test]
    async fn test_qdmi_backend_submit_and_wait() {
        let backend = QdmiBackend::new();

        // Create a simple Bell state circuit
        let mut circuit = Circuit::with_size("bell", 2, 2);
        circuit.h(arvak_ir::QubitId(0)).unwrap();
        circuit
            .cx(arvak_ir::QubitId(0), arvak_ir::QubitId(1))
            .unwrap();
        let _ = circuit.measure_all();

        // Submit
        let job_id = backend.submit(&circuit, 1000).await.unwrap();
        assert!(!job_id.0.is_empty());

        // Wait for completion
        let result = backend.wait(&job_id).await.unwrap();

        assert_eq!(result.shots, 1000);
        assert!(!result.counts.is_empty());
    }

    #[tokio::test]
    async fn test_qdmi_backend_cancel() {
        let backend = QdmiBackend::new();

        let mut circuit = Circuit::with_size("test", 1, 1);
        circuit.h(arvak_ir::QubitId(0)).unwrap();

        let job_id = backend.submit(&circuit, 100).await.unwrap();
        backend.cancel(&job_id).await.unwrap();

        let status = backend.status(&job_id).await.unwrap();
        assert!(matches!(status, JobStatus::Cancelled));
    }

    /// Gate set is now built from `MockDevice::operations`, not hardcoded to universal.
    #[tokio::test]
    async fn test_qdmi_gate_set_from_device_operations() {
        let backend = QdmiBackend::new();
        backend.initialize().unwrap();
        let caps = backend.build_capabilities().unwrap();

        // MockDevice defaults to neutral-atom gate set.
        assert!(caps.gate_set.contains("rz"), "rz should be supported");
        assert!(caps.gate_set.contains("rx"), "rx should be supported");
        assert!(caps.gate_set.contains("ry"), "ry should be supported");
        assert!(caps.gate_set.contains("cz"), "cz should be supported");

        // Universal gates not in the neutral-atom basis should be absent.
        assert!(
            !caps.gate_set.contains("h"),
            "h should not be in neutral-atom set"
        );
        assert!(
            !caps.gate_set.contains("cx"),
            "cx should not be in neutral-atom set"
        );
        assert!(
            !caps.gate_set.contains("sx"),
            "sx should not be in neutral-atom set"
        );

        // All discovered gates are native.
        assert!(caps.gate_set.is_native("rz"));
        assert!(caps.gate_set.is_native("cz"));
    }

    /// Topology is now built from `MockDevice::coupling_map` (linear chain by default).
    #[tokio::test]
    async fn test_qdmi_topology_from_coupling_map() {
        let backend = QdmiBackend::new();
        backend.initialize().unwrap();
        let caps = backend.build_capabilities().unwrap();

        // Mock defaults to linear chain: (0,1), (1,2), ..., (18,19) for 20 qubits.
        assert!(caps.topology.is_connected(0, 1), "0-1 should be connected");
        assert!(caps.topology.is_connected(1, 2), "1-2 should be connected");
        assert!(
            caps.topology.is_connected(18, 19),
            "18-19 should be connected"
        );

        // Non-adjacent qubits are not directly connected.
        assert!(
            !caps.topology.is_connected(0, 2),
            "0-2 should not be connected"
        );
        assert!(
            !caps.topology.is_connected(0, 19),
            "0-19 should not be connected"
        );
    }

    #[test]
    fn test_gate_set_from_operations_single_and_two_qubit() {
        let ops = vec![
            ("rx".to_string(), 1),
            ("rz".to_string(), 1),
            ("cz".to_string(), 2),
        ];
        let gs = gate_set_from_operations(&ops);
        assert!(gs.contains("rx"));
        assert!(gs.contains("rz"));
        assert!(gs.contains("cz"));
        assert!(!gs.contains("h"));
        assert!(gs.is_native("rx"));
        assert!(gs.is_native("cz"));
    }

    #[test]
    fn test_gate_set_from_operations_empty_falls_back_to_universal() {
        // This tests the calling code's fallback, not gate_set_from_operations itself.
        let ops: Vec<(String, usize)> = vec![];
        // Callers use: if ops.is_empty() { GateSet::universal() } else { ... }
        let gs = if ops.is_empty() {
            GateSet::universal()
        } else {
            gate_set_from_operations(&ops)
        };
        assert!(gs.contains("h"), "universal set should contain h");
        assert!(gs.contains("cx"), "universal set should contain cx");
    }
}
