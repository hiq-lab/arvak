//! FFI bindings to QDMI (Quantum Device Management Interface) v1.2.1
//!
//! This module provides safe Rust wrappers around the QDMI C API.
//! The bindings are based on QDMI v1.2.1 headers from the Munich Quantum Software Stack.
//!
//! When the `system-qdmi` feature is enabled, this module links against the
//! system QDMI library. Otherwise, it provides mock implementations for testing.
//!
//! # API Design Notes
//!
//! QDMI v1.2.1 uses a **buffer-query pattern** for all query functions:
//! 1. Call with `value = NULL` to get required buffer size via `size_ret`
//! 2. Allocate a buffer of that size
//! 3. Call again with the buffer to retrieve the actual data
//!
//! Device discovery is done via `QDMI_session_query_session_property` with
//! `QDMI_SESSION_PROPERTY_DEVICES` — there is no `QDMI_session_get_devices`.

use std::ffi::{c_char, c_int};

#[cfg(feature = "system-qdmi")]
use std::ffi::c_void;

// ============================================================================
// QDMI Status Codes
// ============================================================================

/// Status codes returned by QDMI API functions.
///
/// Matches `QDMI_STATUS` enum from `constants.h`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiStatus {
    WarnGeneral = 1,
    Success = 0,
    ErrorFatal = -1,
    ErrorOutOfMem = -2,
    ErrorNotImplemented = -3,
    ErrorLibNotFound = -4,
    ErrorNotFound = -5,
    ErrorOutOfRange = -6,
    ErrorInvalidArgument = -7,
    ErrorPermissionDenied = -8,
    ErrorNotSupported = -9,
    ErrorBadState = -10,
    ErrorTimeout = -11,
}

impl From<c_int> for QdmiStatus {
    fn from(code: c_int) -> Self {
        match code {
            1 => QdmiStatus::WarnGeneral,
            0 => QdmiStatus::Success,
            -1 => QdmiStatus::ErrorFatal,
            -2 => QdmiStatus::ErrorOutOfMem,
            -3 => QdmiStatus::ErrorNotImplemented,
            -4 => QdmiStatus::ErrorLibNotFound,
            -5 => QdmiStatus::ErrorNotFound,
            -6 => QdmiStatus::ErrorOutOfRange,
            -7 => QdmiStatus::ErrorInvalidArgument,
            -8 => QdmiStatus::ErrorPermissionDenied,
            -9 => QdmiStatus::ErrorNotSupported,
            -10 => QdmiStatus::ErrorBadState,
            -11 => QdmiStatus::ErrorTimeout,
            _ => QdmiStatus::ErrorFatal,
        }
    }
}

impl QdmiStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, QdmiStatus::Success | QdmiStatus::WarnGeneral)
    }
}

// ============================================================================
// Session Parameters (QDMI_SESSION_PARAMETER_T)
// ============================================================================

/// Parameters that can be set on a QDMI session via `QDMI_session_set_parameter`.
///
/// Note: In v1.2.1, `BaseUrl` moved to the device-session layer
/// (`QDMI_DEVICE_SESSION_PARAMETER_T`). The client-session layer has
/// `Token`, `AuthFile`, `AuthUrl`, `Username`, `Password`, `ProjectId`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiSessionParameter {
    /// Authentication token (API key)
    Token = 0,
    /// Path to authentication file
    AuthFile = 1,
    /// Authentication server URL (for OIDC)
    AuthUrl = 2,
    /// Username for authentication
    Username = 3,
    /// Password for authentication
    Password = 4,
    /// Project ID (required for LRZ job accounting)
    ProjectId = 5,
    /// Sentinel: one past the last valid value
    Max = 6,
    /// Custom extension slot 1
    Custom1 = 999_999_995,
    /// Custom extension slot 2
    Custom2 = 999_999_996,
    /// Custom extension slot 3
    Custom3 = 999_999_997,
    /// Custom extension slot 4
    Custom4 = 999_999_998,
    /// Custom extension slot 5
    Custom5 = 999_999_999,
}

// ============================================================================
// Session Properties (QDMI_SESSION_PROPERTY_T)
// ============================================================================

/// Properties that can be queried from a QDMI session via
/// `QDMI_session_query_session_property`.
///
/// This is how device discovery works in QDMI v1.2.1 — there is no
/// standalone `QDMI_session_get_devices` function.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiSessionProperty {
    /// List of available devices (`QDMI_Device*` array)
    Devices = 0,
    /// Sentinel
    Max = 1,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Job Parameters (QDMI_JOB_PARAMETER_T)
// ============================================================================

/// Parameters that can be set on a QDMI job via `QDMI_job_set_parameter`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobParameter {
    /// Program format (QASM2, QASM3, QIR, etc.)
    ProgramFormat = 0,
    /// The program to execute
    Program = 1,
    /// Number of shots
    ShotsNum = 2,
    /// Sentinel
    Max = 3,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Job Properties (QDMI_JOB_PROPERTY_T)
// ============================================================================

/// Properties that can be queried from a QDMI job via `QDMI_job_query_property`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobProperty {
    /// Job identifier (char*)
    Id = 0,
    /// Program format
    ProgramFormat = 1,
    /// Program data
    Program = 2,
    /// Number of shots
    ShotsNum = 3,
    /// Sentinel
    Max = 4,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Device Properties (QDMI_DEVICE_PROPERTY_T)
// ============================================================================

/// Device properties that can be queried via `QDMI_device_query_device_property`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiDeviceProperty {
    /// Device name (char*)
    Name = 0,
    /// Device version (char*)
    Version = 1,
    /// Device status (`QDMI_Device_Status`)
    Status = 2,
    /// QDMI library version (char*)
    LibraryVersion = 3,
    /// Number of qubits (`size_t`)
    QubitsNum = 4,
    /// List of sites (`QDMI_Site`* array)
    Sites = 5,
    /// List of supported operations (`QDMI_Operation`* array)
    Operations = 6,
    /// Coupling map (`QDMI_Site`* flattened pairs)
    CouplingMap = 7,
    /// Whether calibration is needed (`size_t`, 0=no)
    NeedsCalibration = 8,
    /// Pulse support level (`QDMI_Device_Pulse_Support_Level`)
    PulseSupport = 9,
    /// Length unit, SI string (char*)
    LengthUnit = 10,
    /// Length scale factor (double)
    LengthScaleFactor = 11,
    /// Duration unit, SI string (char*)
    DurationUnit = 12,
    /// Duration scale factor (double)
    DurationScaleFactor = 13,
    /// Minimum atom distance (`uint64_t` raw)
    MinAtomDistance = 14,
    /// Supported program formats (`QDMI_Program_Format`* array)
    SupportedProgramFormats = 15,
    /// Sentinel
    Max = 16,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Site Properties (QDMI_SITE_PROPERTY_T)
// ============================================================================

/// Site (qubit) properties that can be queried via `QDMI_device_query_site_property`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiSiteProperty {
    /// Site index (`size_t`, required for all sites)
    Index = 0,
    /// T1 coherence time (`uint64_t` raw)
    T1 = 1,
    /// T2 coherence time (`uint64_t` raw)
    T2 = 2,
    /// Site name (char*)
    Name = 3,
    /// X coordinate (`int64_t` raw)
    XCoordinate = 4,
    /// Y coordinate (`int64_t` raw)
    YCoordinate = 5,
    /// Z coordinate (`int64_t` raw)
    ZCoordinate = 6,
    /// Whether this is a zone (bool)
    IsZone = 7,
    /// X extent for zones (`uint64_t` raw)
    XExtent = 8,
    /// Y extent for zones (`uint64_t` raw)
    YExtent = 9,
    /// Z extent for zones (`uint64_t` raw)
    ZExtent = 10,
    /// Module index (`uint64_t`)
    ModuleIndex = 11,
    /// Submodule index (`uint64_t`)
    SubmoduleIndex = 12,
    /// Sentinel
    Max = 13,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Operation Properties (QDMI_OPERATION_PROPERTY_T)
// ============================================================================

/// Operation (gate) properties that can be queried via
/// `QDMI_device_query_operation_property`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiOperationProperty {
    /// Operation name (char*)
    Name = 0,
    /// Number of qubits (`size_t`)
    QubitsNum = 1,
    /// Number of parameters (`size_t`)
    ParametersNum = 2,
    /// Duration (`uint64_t` raw)
    Duration = 3,
    /// Fidelity (double)
    Fidelity = 4,
    /// Interaction radius (`uint64_t` raw)
    InteractionRadius = 5,
    /// Blocking radius (`uint64_t` raw)
    BlockingRadius = 6,
    /// Idling fidelity (double)
    IdlingFidelity = 7,
    /// Whether this is a zoned operation (bool)
    IsZoned = 8,
    /// Applicable sites (`QDMI_Site`* array)
    Sites = 9,
    /// Mean shuttling speed (`uint64_t` raw)
    MeanShuttlingSpeed = 10,
    /// Sentinel
    Max = 11,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Device Status (QDMI_DEVICE_STATUS_T)
// ============================================================================

/// Device operational status.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiDeviceStatus {
    Offline = 0,
    Idle = 1,
    Busy = 2,
    Error = 3,
    Maintenance = 4,
    Calibration = 5,
    /// Sentinel
    Max = 6,
}

impl From<c_int> for QdmiDeviceStatus {
    fn from(status: c_int) -> Self {
        match status {
            0 => QdmiDeviceStatus::Offline,
            1 => QdmiDeviceStatus::Idle,
            2 => QdmiDeviceStatus::Busy,
            3 => QdmiDeviceStatus::Error,
            4 => QdmiDeviceStatus::Maintenance,
            5 => QdmiDeviceStatus::Calibration,
            _ => QdmiDeviceStatus::Error,
        }
    }
}

// ============================================================================
// Job Status (QDMI_JOB_STATUS_T)
// ============================================================================

/// Job execution status.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobStatus {
    Created = 0,
    Submitted = 1,
    Queued = 2,
    Running = 3,
    Done = 4,
    Canceled = 5,
    Failed = 6,
}

impl From<c_int> for QdmiJobStatus {
    fn from(status: c_int) -> Self {
        match status {
            0 => QdmiJobStatus::Created,
            1 => QdmiJobStatus::Submitted,
            2 => QdmiJobStatus::Queued,
            3 => QdmiJobStatus::Running,
            4 => QdmiJobStatus::Done,
            5 => QdmiJobStatus::Canceled,
            6 => QdmiJobStatus::Failed,
            _ => QdmiJobStatus::Failed,
        }
    }
}

// ============================================================================
// Program Formats (QDMI_PROGRAM_FORMAT_T)
// ============================================================================

/// Supported program formats.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiProgramFormat {
    /// `OpenQASM` 2.0 (char*)
    Qasm2 = 0,
    /// `OpenQASM` 3.0 (char*)
    Qasm3 = 1,
    /// QIR Base Profile text (char*)
    QirBaseString = 2,
    /// QIR Base Profile binary (void*)
    QirBaseModule = 3,
    /// QIR Adaptive Profile text (char*)
    QirAdaptiveString = 4,
    /// QIR Adaptive Profile binary (void*)
    QirAdaptiveModule = 5,
    /// Calibration request (void*)
    Calibration = 6,
    /// Qiskit QPY binary (void*)
    Qpy = 7,
    /// IQM JSON format (char*)
    IqmJson = 8,
    /// Sentinel
    Max = 9,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Job Result Types (QDMI_JOB_RESULT_T)
// ============================================================================

/// Result format types for `QDMI_job_get_results`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobResult {
    /// Raw shots as comma-separated strings (char*)
    Shots = 0,
    /// Histogram keys (char*)
    HistKeys = 1,
    /// Histogram values / counts (`size_t`*)
    HistValues = 2,
    /// Dense state vector (double* [re,im,re,im,...])
    StatevectorDense = 3,
    /// Dense probabilities (double*)
    ProbabilitiesDense = 4,
    /// Sparse state vector keys (char*)
    StatevectorSparseKeys = 5,
    /// Sparse state vector values (double*)
    StatevectorSparseValues = 6,
    /// Sparse probabilities keys (char*)
    ProbabilitiesSparseKeys = 7,
    /// Sparse probabilities values (double*)
    ProbabilitiesSparseValues = 8,
    /// Sentinel
    Max = 9,
    /// Custom extension slots
    Custom1 = 999_999_995,
    Custom2 = 999_999_996,
    Custom3 = 999_999_997,
    Custom4 = 999_999_998,
    Custom5 = 999_999_999,
}

// ============================================================================
// Pulse Support Level (QDMI_DEVICE_PULSE_SUPPORT_LEVEL_T)
// ============================================================================

/// Device pulse support level.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiDevicePulseSupportLevel {
    /// No pulse-level control
    None = 0,
    /// Site-level pulse control
    Site = 1,
    /// Channel-level pulse control
    Channel = 2,
    /// Site and channel pulse control
    SiteAndChannel = 3,
}

// ============================================================================
// Opaque Handle Types
// ============================================================================

/// Opaque handle to a QDMI session.
#[repr(C)]
pub struct QdmiSession {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI device.
#[repr(C)]
pub struct QdmiDevice {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI job.
#[repr(C)]
pub struct QdmiJob {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI site.
#[repr(C)]
pub struct QdmiSite {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI operation.
#[repr(C)]
pub struct QdmiOperation {
    _private: [u8; 0],
}

// ============================================================================
// FFI Function Declarations — QDMI v1.2.1 Client API
// ============================================================================
//
// These match the function signatures from `include/qdmi/client.h` in the
// QDMI v1.2.1 release (December 2024).
//
// Key differences from earlier API drafts:
// - `QDMI_session_set_parameter` takes a `size` parameter
// - `QDMI_session_free` returns `void` (not `int`)
// - No `QDMI_session_get_devices` — use `QDMI_session_query_session_property`
// - `QDMI_job_set_parameter` takes a `size` parameter
// - `QDMI_job_free` returns `void` (not `int`)
// - `QDMI_device_query_operation_property` takes site/param arrays
// - All query functions use the buffer-query pattern (size + size_ret)
// - `QDMI_job_wait` timeout is `size_t` (not `c_ulong`)
// - `QDMI_job_check` writes to `QDMI_Job_Status*` (c_int)

#[cfg(feature = "system-qdmi")]
#[link(name = "qdmi")]
extern "C" {
    // ── Session management ──────────────────────────────────────────────────

    /// Allocate a new QDMI session.
    pub fn QDMI_session_alloc(session: *mut *mut QdmiSession) -> c_int;

    /// Set a session parameter (token, auth file, project ID, etc.).
    ///
    /// `size` is the size of the value buffer in bytes (including null
    /// terminator for strings).
    pub fn QDMI_session_set_parameter(
        session: *mut QdmiSession,
        param: c_int,
        size: usize,
        value: *const c_void,
    ) -> c_int;

    /// Initialize the session (connects to backends, loads device drivers).
    pub fn QDMI_session_init(session: *mut QdmiSession) -> c_int;

    /// Query a session property (e.g., list of available devices).
    ///
    /// Uses the buffer-query pattern:
    /// - Pass `value = NULL` to get required size via `size_ret`
    /// - Then allocate and call again with the buffer
    pub fn QDMI_session_query_session_property(
        session: *mut QdmiSession,
        prop: c_int,
        size: usize,
        value: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    /// Free a QDMI session. Returns void in v1.2.1.
    pub fn QDMI_session_free(session: *mut QdmiSession);

    // ── Device queries ──────────────────────────────────────────────────────

    /// Query a device-level property (name, qubit count, topology, etc.).
    ///
    /// Uses the buffer-query pattern.
    pub fn QDMI_device_query_device_property(
        device: *mut QdmiDevice,
        property: c_int,
        size: usize,
        value: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    /// Query a site-level property (T1, T2, coordinates, etc.).
    ///
    /// Uses the buffer-query pattern.
    pub fn QDMI_device_query_site_property(
        device: *mut QdmiDevice,
        site: *mut QdmiSite,
        property: c_int,
        size: usize,
        value: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    /// Query an operation-level property (fidelity, duration, etc.).
    ///
    /// This is the most complex query: operation properties can depend on
    /// which sites they act on and which parameter values are used.
    pub fn QDMI_device_query_operation_property(
        device: *mut QdmiDevice,
        operation: *mut QdmiOperation,
        num_sites: usize,
        sites: *const QdmiSite,
        num_params: usize,
        params: *const f64,
        property: c_int,
        size: usize,
        value: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    // ── Job management ──────────────────────────────────────────────────────

    /// Create a new job on a device.
    pub fn QDMI_device_create_job(device: *mut QdmiDevice, job: *mut *mut QdmiJob) -> c_int;

    /// Set a job parameter (program format, program, shots, etc.).
    ///
    /// `size` is the size of the value buffer in bytes.
    pub fn QDMI_job_set_parameter(
        job: *mut QdmiJob,
        param: c_int,
        size: usize,
        value: *const c_void,
    ) -> c_int;

    /// Query a job property (ID, program format, etc.).
    ///
    /// Uses the buffer-query pattern.
    pub fn QDMI_job_query_property(
        job: *mut QdmiJob,
        prop: c_int,
        size: usize,
        value: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    /// Submit a job for execution.
    pub fn QDMI_job_submit(job: *mut QdmiJob) -> c_int;

    /// Check the current status of a job (non-blocking).
    pub fn QDMI_job_check(job: *mut QdmiJob, status: *mut c_int) -> c_int;

    /// Wait for a job to complete (blocking).
    ///
    /// `timeout` is in milliseconds. Pass 0 for infinite wait.
    pub fn QDMI_job_wait(job: *mut QdmiJob, timeout: usize) -> c_int;

    /// Get results from a completed job.
    ///
    /// Uses the buffer-query pattern.
    pub fn QDMI_job_get_results(
        job: *mut QdmiJob,
        result_type: c_int,
        size: usize,
        data: *mut c_void,
        size_ret: *mut usize,
    ) -> c_int;

    /// Cancel a running job.
    pub fn QDMI_job_cancel(job: *mut QdmiJob) -> c_int;

    /// Free a job handle. Returns void in v1.2.1.
    pub fn QDMI_job_free(job: *mut QdmiJob);
}

// ============================================================================
// Mock Implementations (for testing without system QDMI)
// ============================================================================

#[cfg(not(feature = "system-qdmi"))]
pub mod mock {
    use super::{QdmiDeviceStatus, QdmiJobStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static MOCK_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Mock session for testing.
    pub struct MockSession {
        pub id: usize,
        pub token: Option<String>,
        pub base_url: Option<String>,
        pub project_id: Option<String>,
        pub initialized: bool,
    }

    /// Mock device for testing.
    pub struct MockDevice {
        pub name: String,
        pub num_qubits: usize,
        pub status: QdmiDeviceStatus,
    }

    /// Mock job for testing.
    pub struct MockJob {
        pub id: String,
        pub status: QdmiJobStatus,
        pub program: Option<String>,
        pub shots: usize,
        pub results: Option<Vec<String>>,
    }

    impl MockSession {
        pub fn new() -> Self {
            MockSession {
                id: MOCK_COUNTER.fetch_add(1, Ordering::SeqCst),
                token: None,
                base_url: None,
                project_id: None,
                initialized: false,
            }
        }
    }

    impl Default for MockSession {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockDevice {
        pub fn new(name: &str, num_qubits: usize) -> Self {
            MockDevice {
                name: name.to_string(),
                num_qubits,
                status: QdmiDeviceStatus::Idle,
            }
        }
    }

    impl MockJob {
        pub fn new() -> Self {
            MockJob {
                id: uuid::Uuid::new_v4().to_string(),
                status: QdmiJobStatus::Created,
                program: None,
                shots: 1000,
                results: None,
            }
        }
    }

    impl Default for MockJob {
        fn default() -> Self {
            Self::new()
        }
    }
}

// ============================================================================
// Safe Rust Wrappers
// ============================================================================

use std::ffi::CStr;

/// Convert a C string pointer to a Rust String.
///
/// # Safety
/// The pointer must be valid and null-terminated.
pub unsafe fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: Caller must ensure ptr is valid and null-terminated
    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .ok()
            .map(std::string::ToString::to_string)
    }
}

/// Result type for QDMI operations.
pub type QdmiResult<T> = Result<T, QdmiStatus>;

/// Check QDMI status and convert to Result.
pub fn check_status(status: c_int) -> QdmiResult<()> {
    let s = QdmiStatus::from(status);
    if s.is_success() { Ok(()) } else { Err(s) }
}
