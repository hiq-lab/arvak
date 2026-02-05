//! FFI bindings to QDMI (Quantum Device Management Interface)
//!
//! This module provides safe Rust wrappers around the QDMI C API.
//! The bindings are based on QDMI headers from the Munich Quantum Software Stack.
//!
//! When the `system-qdmi` feature is enabled, this module links against the
//! system QDMI library. Otherwise, it provides mock implementations for testing.

use std::ffi::{c_char, c_int};

#[cfg(feature = "system-qdmi")]
use std::ffi::c_void;
#[cfg(feature = "system-qdmi")]
use std::os::raw::c_ulong;

// ============================================================================
// QDMI Status Codes
// ============================================================================

/// Status codes returned by QDMI API functions
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
// Session Parameters
// ============================================================================

/// Parameters that can be set on a QDMI session
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiSessionParameter {
    /// Base URL or API endpoint
    BaseUrl = 0,
    /// Authentication token (API key)
    Token = 1,
    /// Path to authentication file
    AuthFile = 2,
    /// Authentication server URL (for OIDC)
    AuthUrl = 3,
    /// Username for authentication
    Username = 4,
    /// Password for authentication
    Password = 5,
}

// ============================================================================
// Job Parameters
// ============================================================================

/// Parameters that can be set on a QDMI job
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobParameter {
    /// Program format (QASM2, QASM3, QIR, etc.)
    ProgramFormat = 0,
    /// The program to execute
    Program = 1,
    /// Number of shots
    ShotsNum = 2,
}

// ============================================================================
// Device Properties
// ============================================================================

/// Device properties that can be queried
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiDeviceProperty {
    /// Device name
    Name = 0,
    /// Device version
    Version = 1,
    /// Device status (online/offline/busy)
    Status = 2,
    /// QDMI library version
    LibraryVersion = 3,
    /// Number of qubits
    QubitsNum = 4,
    /// List of sites
    Sites = 5,
    /// List of supported operations
    Operations = 6,
    /// Coupling map
    CouplingMap = 7,
    /// Whether calibration is needed
    NeedsCalibration = 8,
    /// Pulse support level
    PulseSupport = 9,
    /// Length unit (mm, um, nm)
    LengthUnit = 10,
    /// Length scale factor
    LengthScaleFactor = 11,
    /// Duration unit (ms, us, ns)
    DurationUnit = 12,
    /// Duration scale factor
    DurationScaleFactor = 13,
    /// Minimum atom distance
    MinAtomDistance = 14,
    /// Supported program formats
    SupportedProgramFormats = 15,
}

// ============================================================================
// Site Properties
// ============================================================================

/// Site (qubit) properties that can be queried
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiSiteProperty {
    /// Site index
    Index = 0,
    /// T1 coherence time
    T1 = 1,
    /// T2 coherence time
    T2 = 2,
    /// Site name
    Name = 3,
    /// X coordinate
    XCoordinate = 4,
    /// Y coordinate
    YCoordinate = 5,
    /// Z coordinate
    ZCoordinate = 6,
    /// Whether this is a zone
    IsZone = 7,
    /// X extent (for zones)
    XExtent = 8,
    /// Y extent (for zones)
    YExtent = 9,
    /// Z extent (for zones)
    ZExtent = 10,
    /// Module index
    ModuleIndex = 11,
    /// Submodule index
    SubmoduleIndex = 12,
}

// ============================================================================
// Operation Properties
// ============================================================================

/// Operation (gate) properties that can be queried
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiOperationProperty {
    /// Operation name
    Name = 0,
    /// Number of qubits
    QubitsNum = 1,
    /// Number of parameters
    ParametersNum = 2,
    /// Duration
    Duration = 3,
    /// Fidelity
    Fidelity = 4,
    /// Interaction radius
    InteractionRadius = 5,
    /// Blocking radius
    BlockingRadius = 6,
    /// Idling fidelity
    IdlingFidelity = 7,
    /// Whether this is a zoned operation
    IsZoned = 8,
    /// Applicable sites
    Sites = 9,
    /// Mean shuttling speed
    MeanShuttlingSpeed = 10,
}

// ============================================================================
// Device Status
// ============================================================================

/// Device operational status
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiDeviceStatus {
    Offline = 0,
    Idle = 1,
    Busy = 2,
    Error = 3,
    Maintenance = 4,
    Calibration = 5,
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
// Job Status
// ============================================================================

/// Job execution status
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
// Program Formats
// ============================================================================

/// Supported program formats
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiProgramFormat {
    /// OpenQASM 2.0
    Qasm2 = 0,
    /// OpenQASM 3.0
    Qasm3 = 1,
    /// QIR Base Profile (text)
    QirBaseString = 2,
    /// QIR Base Profile (binary)
    QirBaseModule = 3,
    /// QIR Adaptive Profile (text)
    QirAdaptiveString = 4,
    /// QIR Adaptive Profile (binary)
    QirAdaptiveModule = 5,
    /// Calibration request
    Calibration = 6,
    /// Qiskit QPY format
    Qpy = 7,
    /// IQM JSON format
    IqmJson = 8,
}

// ============================================================================
// Job Result Types
// ============================================================================

/// Result format types
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QdmiJobResult {
    /// Raw shots as comma-separated strings
    Shots = 0,
    /// Histogram keys
    HistKeys = 1,
    /// Histogram values (counts)
    HistValues = 2,
    /// Dense state vector
    StatevectorDense = 3,
    /// Dense probabilities
    ProbabilitiesDense = 4,
    /// Sparse state vector keys
    StatevectorSparseKeys = 5,
    /// Sparse state vector values
    StatevectorSparseValues = 6,
    /// Sparse probabilities keys
    ProbabilitiesSparseKeys = 7,
    /// Sparse probabilities values
    ProbabilitiesSparseValues = 8,
}

// ============================================================================
// Opaque Handle Types
// ============================================================================

/// Opaque handle to a QDMI session
#[repr(C)]
pub struct QdmiSession {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI device
#[repr(C)]
pub struct QdmiDevice {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI job
#[repr(C)]
pub struct QdmiJob {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI site
#[repr(C)]
pub struct QdmiSite {
    _private: [u8; 0],
}

/// Opaque handle to a QDMI operation
#[repr(C)]
pub struct QdmiOperation {
    _private: [u8; 0],
}

// ============================================================================
// FFI Function Declarations (when linking against system QDMI)
// ============================================================================

#[cfg(feature = "system-qdmi")]
#[link(name = "qdmi")]
extern "C" {
    // Session management
    pub fn QDMI_session_alloc(session: *mut *mut QdmiSession) -> c_int;
    pub fn QDMI_session_set_parameter(
        session: *mut QdmiSession,
        param: c_int,
        value: *const c_void,
    ) -> c_int;
    pub fn QDMI_session_init(session: *mut QdmiSession) -> c_int;
    pub fn QDMI_session_free(session: *mut QdmiSession) -> c_int;

    // Device queries
    pub fn QDMI_session_get_devices(
        session: *mut QdmiSession,
        devices: *mut *mut QdmiDevice,
        count: *mut usize,
    ) -> c_int;
    pub fn QDMI_device_query_device_property(
        device: *mut QdmiDevice,
        property: c_int,
        value: *mut c_void,
        size: *mut usize,
    ) -> c_int;
    pub fn QDMI_device_query_site_property(
        device: *mut QdmiDevice,
        site: *mut QdmiSite,
        property: c_int,
        value: *mut c_void,
        size: *mut usize,
    ) -> c_int;
    pub fn QDMI_device_query_operation_property(
        device: *mut QdmiDevice,
        operation: *mut QdmiOperation,
        property: c_int,
        value: *mut c_void,
        size: *mut usize,
    ) -> c_int;

    // Job management
    pub fn QDMI_device_create_job(device: *mut QdmiDevice, job: *mut *mut QdmiJob) -> c_int;
    pub fn QDMI_job_set_parameter(job: *mut QdmiJob, param: c_int, value: *const c_void) -> c_int;
    pub fn QDMI_job_submit(job: *mut QdmiJob) -> c_int;
    pub fn QDMI_job_check(job: *mut QdmiJob, status: *mut c_int) -> c_int;
    pub fn QDMI_job_wait(job: *mut QdmiJob, timeout_ms: c_ulong) -> c_int;
    pub fn QDMI_job_get_results(
        job: *mut QdmiJob,
        result_type: c_int,
        data: *mut c_void,
        size: *mut usize,
    ) -> c_int;
    pub fn QDMI_job_cancel(job: *mut QdmiJob) -> c_int;
    pub fn QDMI_job_free(job: *mut QdmiJob) -> c_int;
}

// ============================================================================
// Mock Implementations (for testing without system QDMI)
// ============================================================================

#[cfg(not(feature = "system-qdmi"))]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static MOCK_COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Mock session for testing
    pub struct MockSession {
        pub id: usize,
        pub token: Option<String>,
        pub base_url: Option<String>,
        pub initialized: bool,
    }

    /// Mock device for testing
    pub struct MockDevice {
        pub name: String,
        pub num_qubits: usize,
        pub status: QdmiDeviceStatus,
    }

    /// Mock job for testing
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

/// Convert a C string pointer to a Rust String
///
/// # Safety
/// The pointer must be valid and null-terminated
pub unsafe fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: Caller must ensure ptr is valid and null-terminated
    unsafe { CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string()) }
}

/// Result type for QDMI operations
pub type QdmiResult<T> = Result<T, QdmiStatus>;

/// Check QDMI status and convert to Result
pub fn check_status(status: c_int) -> QdmiResult<()> {
    let s = QdmiStatus::from(status);
    if s.is_success() { Ok(()) } else { Err(s) }
}
