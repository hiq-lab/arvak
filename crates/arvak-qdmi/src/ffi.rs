// SPDX-License-Identifier: Apache-2.0
//! Raw FFI constants and type definitions for the QDMI v1.2.1 **device** interface.
//!
//! These values must match the QDMI header definitions from
//! <https://github.com/Munich-Quantum-Software-Stack/QDMI>.
//!
//! We consume the **device interface** directly (prefix-aware dlsym), not the
//! client/driver layer. This means all function pointers are resolved at runtime
//! from the device shared library, not linked statically.
//!
//! Reference: `adapters/arvak-adapter-qdmi/src/ffi.rs` (client-side, verified
//! against upstream QDMI v1.2.1 headers).

use std::ffi::c_void;
use std::os::raw::c_int;

// ===========================================================================
// Opaque handle types
// ===========================================================================

/// Opaque device session handle (`PREFIX_QDMI_Device_Session`).
pub type QdmiDeviceSession = *mut c_void;

/// Opaque site handle (`QDMI_Site`).
pub type QdmiSite = *mut c_void;

/// Opaque operation handle (`QDMI_Operation`).
pub type QdmiOperation = *mut c_void;

/// Opaque device job handle (`PREFIX_QDMI_Device_Job`).
pub type QdmiDeviceJob = *mut c_void;

// ===========================================================================
// Status codes (QDMI_STATUS)
// ===========================================================================

pub const QDMI_SUCCESS: c_int = 0;
pub const QDMI_WARN_GENERAL: c_int = 1;
pub const QDMI_ERROR_FATAL: c_int = -1;
pub const QDMI_ERROR_OUTOFMEM: c_int = -2;
pub const QDMI_ERROR_NOTIMPLEMENTED: c_int = -3;
pub const QDMI_ERROR_LIBNOTFOUND: c_int = -4;
pub const QDMI_ERROR_NOTFOUND: c_int = -5;
pub const QDMI_ERROR_OUTOFRANGE: c_int = -6;
pub const QDMI_ERROR_INVALIDARGUMENT: c_int = -7;
pub const QDMI_ERROR_PERMISSIONDENIED: c_int = -8;
pub const QDMI_ERROR_NOTSUPPORTED: c_int = -9;
pub const QDMI_ERROR_BADSTATE: c_int = -10;
pub const QDMI_ERROR_TIMEOUT: c_int = -11;

/// Returns `true` if the QDMI return code indicates success.
/// Accepts both `QDMI_SUCCESS` (0) and `QDMI_WARN_GENERAL` (1).
#[inline]
pub fn is_success(code: c_int) -> bool {
    code == QDMI_SUCCESS || code == QDMI_WARN_GENERAL
}

// ===========================================================================
// Device property keys (QDMI_DEVICE_PROPERTY_T)
// ===========================================================================

pub type QdmiDeviceProperty = c_int;

pub const QDMI_DEVICE_PROPERTY_NAME: QdmiDeviceProperty = 0;
pub const QDMI_DEVICE_PROPERTY_VERSION: QdmiDeviceProperty = 1;
pub const QDMI_DEVICE_PROPERTY_STATUS: QdmiDeviceProperty = 2;
pub const QDMI_DEVICE_PROPERTY_LIBRARYVERSION: QdmiDeviceProperty = 3;
pub const QDMI_DEVICE_PROPERTY_QUBITSNUM: QdmiDeviceProperty = 4;
pub const QDMI_DEVICE_PROPERTY_SITES: QdmiDeviceProperty = 5;
pub const QDMI_DEVICE_PROPERTY_OPERATIONS: QdmiDeviceProperty = 6;
pub const QDMI_DEVICE_PROPERTY_COUPLINGMAP: QdmiDeviceProperty = 7;
pub const QDMI_DEVICE_PROPERTY_NEEDSCALIBRATION: QdmiDeviceProperty = 8;
pub const QDMI_DEVICE_PROPERTY_PULSESUPPORT: QdmiDeviceProperty = 9;
pub const QDMI_DEVICE_PROPERTY_LENGTHUNIT: QdmiDeviceProperty = 10;
pub const QDMI_DEVICE_PROPERTY_LENGTHSCALEFACTOR: QdmiDeviceProperty = 11;
pub const QDMI_DEVICE_PROPERTY_DURATIONUNIT: QdmiDeviceProperty = 12;
pub const QDMI_DEVICE_PROPERTY_DURATIONSCALEFACTOR: QdmiDeviceProperty = 13;
pub const QDMI_DEVICE_PROPERTY_MINATOMDISTANCE: QdmiDeviceProperty = 14;
pub const QDMI_DEVICE_PROPERTY_SUPPORTEDPROGRAMFORMATS: QdmiDeviceProperty = 15;

// ===========================================================================
// Site property keys (QDMI_SITE_PROPERTY_T)
// ===========================================================================

pub type QdmiSiteProperty = c_int;

pub const QDMI_SITE_PROPERTY_INDEX: QdmiSiteProperty = 0;
pub const QDMI_SITE_PROPERTY_T1: QdmiSiteProperty = 1;
pub const QDMI_SITE_PROPERTY_T2: QdmiSiteProperty = 2;
pub const QDMI_SITE_PROPERTY_NAME: QdmiSiteProperty = 3;
pub const QDMI_SITE_PROPERTY_XCOORDINATE: QdmiSiteProperty = 4;
pub const QDMI_SITE_PROPERTY_YCOORDINATE: QdmiSiteProperty = 5;
pub const QDMI_SITE_PROPERTY_ZCOORDINATE: QdmiSiteProperty = 6;
pub const QDMI_SITE_PROPERTY_ISZONE: QdmiSiteProperty = 7;
pub const QDMI_SITE_PROPERTY_XEXTENT: QdmiSiteProperty = 8;
pub const QDMI_SITE_PROPERTY_YEXTENT: QdmiSiteProperty = 9;
pub const QDMI_SITE_PROPERTY_ZEXTENT: QdmiSiteProperty = 10;
pub const QDMI_SITE_PROPERTY_MODULEINDEX: QdmiSiteProperty = 11;
pub const QDMI_SITE_PROPERTY_SUBMODULEINDEX: QdmiSiteProperty = 12;

// ===========================================================================
// Operation property keys (QDMI_OPERATION_PROPERTY_T)
// ===========================================================================

pub type QdmiOperationProperty = c_int;

pub const QDMI_OPERATION_PROPERTY_NAME: QdmiOperationProperty = 0;
pub const QDMI_OPERATION_PROPERTY_QUBITSNUM: QdmiOperationProperty = 1;
pub const QDMI_OPERATION_PROPERTY_PARAMETERSNUM: QdmiOperationProperty = 2;
pub const QDMI_OPERATION_PROPERTY_DURATION: QdmiOperationProperty = 3;
pub const QDMI_OPERATION_PROPERTY_FIDELITY: QdmiOperationProperty = 4;
pub const QDMI_OPERATION_PROPERTY_INTERACTIONRADIUS: QdmiOperationProperty = 5;
pub const QDMI_OPERATION_PROPERTY_BLOCKINGRADIUS: QdmiOperationProperty = 6;
pub const QDMI_OPERATION_PROPERTY_IDLINGFIDELITY: QdmiOperationProperty = 7;
pub const QDMI_OPERATION_PROPERTY_ISZONED: QdmiOperationProperty = 8;
pub const QDMI_OPERATION_PROPERTY_SITES: QdmiOperationProperty = 9;
pub const QDMI_OPERATION_PROPERTY_MEANSHUTTLINGSPEED: QdmiOperationProperty = 10;

// ===========================================================================
// Device session parameters (QDMI_DEVICE_SESSION_PARAMETER_T)
// ===========================================================================

pub type QdmiDeviceSessionParameter = c_int;

pub const QDMI_DEVICE_SESSION_PARAMETER_BASEURL: QdmiDeviceSessionParameter = 0;
pub const QDMI_DEVICE_SESSION_PARAMETER_TOKEN: QdmiDeviceSessionParameter = 1;
pub const QDMI_DEVICE_SESSION_PARAMETER_AUTHFILE: QdmiDeviceSessionParameter = 2;
pub const QDMI_DEVICE_SESSION_PARAMETER_AUTHURL: QdmiDeviceSessionParameter = 3;
pub const QDMI_DEVICE_SESSION_PARAMETER_USERNAME: QdmiDeviceSessionParameter = 4;
pub const QDMI_DEVICE_SESSION_PARAMETER_PASSWORD: QdmiDeviceSessionParameter = 5;

// ===========================================================================
// Device job parameters (QDMI_DEVICE_JOB_PARAMETER_T)
// ===========================================================================

pub type QdmiDeviceJobParameter = c_int;

pub const QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT: QdmiDeviceJobParameter = 0;
pub const QDMI_DEVICE_JOB_PARAMETER_PROGRAM: QdmiDeviceJobParameter = 1;
pub const QDMI_DEVICE_JOB_PARAMETER_SHOTSNUM: QdmiDeviceJobParameter = 2;

// ===========================================================================
// Device job properties (QDMI_DEVICE_JOB_PROPERTY_T)
// ===========================================================================

pub type QdmiDeviceJobProperty = c_int;

pub const QDMI_DEVICE_JOB_PROPERTY_ID: QdmiDeviceJobProperty = 0;
pub const QDMI_DEVICE_JOB_PROPERTY_PROGRAMFORMAT: QdmiDeviceJobProperty = 1;
pub const QDMI_DEVICE_JOB_PROPERTY_PROGRAM: QdmiDeviceJobProperty = 2;
pub const QDMI_DEVICE_JOB_PROPERTY_SHOTSNUM: QdmiDeviceJobProperty = 3;

// ===========================================================================
// Job status (QDMI_JOB_STATUS_T)
// ===========================================================================

pub type QdmiJobStatus = c_int;

pub const QDMI_JOB_STATUS_CREATED: QdmiJobStatus = 0;
pub const QDMI_JOB_STATUS_SUBMITTED: QdmiJobStatus = 1;
pub const QDMI_JOB_STATUS_QUEUED: QdmiJobStatus = 2;
pub const QDMI_JOB_STATUS_RUNNING: QdmiJobStatus = 3;
pub const QDMI_JOB_STATUS_DONE: QdmiJobStatus = 4;
pub const QDMI_JOB_STATUS_CANCELED: QdmiJobStatus = 5;
pub const QDMI_JOB_STATUS_FAILED: QdmiJobStatus = 6;

// ===========================================================================
// Device status (QDMI_DEVICE_STATUS_T)
// ===========================================================================

pub type QdmiDeviceStatusCode = c_int;

pub const QDMI_DEVICE_STATUS_OFFLINE: QdmiDeviceStatusCode = 0;
pub const QDMI_DEVICE_STATUS_IDLE: QdmiDeviceStatusCode = 1;
pub const QDMI_DEVICE_STATUS_BUSY: QdmiDeviceStatusCode = 2;
pub const QDMI_DEVICE_STATUS_ERROR: QdmiDeviceStatusCode = 3;
pub const QDMI_DEVICE_STATUS_MAINTENANCE: QdmiDeviceStatusCode = 4;
pub const QDMI_DEVICE_STATUS_CALIBRATION: QdmiDeviceStatusCode = 5;

// ===========================================================================
// Program formats (QDMI_PROGRAM_FORMAT_T)
// ===========================================================================

pub type QdmiProgramFormat = c_int;

pub const QDMI_PROGRAM_FORMAT_QASM2: QdmiProgramFormat = 0;
pub const QDMI_PROGRAM_FORMAT_QASM3: QdmiProgramFormat = 1;
pub const QDMI_PROGRAM_FORMAT_QIRBASESTRING: QdmiProgramFormat = 2;
pub const QDMI_PROGRAM_FORMAT_QIRBASEMODULE: QdmiProgramFormat = 3;
pub const QDMI_PROGRAM_FORMAT_QIRADAPTIVESTRING: QdmiProgramFormat = 4;
pub const QDMI_PROGRAM_FORMAT_QIRADAPTIVEMODULE: QdmiProgramFormat = 5;
pub const QDMI_PROGRAM_FORMAT_CALIBRATION: QdmiProgramFormat = 6;
pub const QDMI_PROGRAM_FORMAT_QPY: QdmiProgramFormat = 7;
pub const QDMI_PROGRAM_FORMAT_IQMJSON: QdmiProgramFormat = 8;

// ===========================================================================
// Job result types (QDMI_JOB_RESULT_T)
// ===========================================================================

pub type QdmiJobResultType = c_int;

pub const QDMI_JOB_RESULT_SHOTS: QdmiJobResultType = 0;
pub const QDMI_JOB_RESULT_HISTKEYS: QdmiJobResultType = 1;
pub const QDMI_JOB_RESULT_HISTVALUES: QdmiJobResultType = 2;
pub const QDMI_JOB_RESULT_STATEVECTORDENSE: QdmiJobResultType = 3;
pub const QDMI_JOB_RESULT_PROBABILITIESDENSE: QdmiJobResultType = 4;
pub const QDMI_JOB_RESULT_STATEVECTORSPARSEKEYS: QdmiJobResultType = 5;
pub const QDMI_JOB_RESULT_STATEVECTORSPARSEVALUES: QdmiJobResultType = 6;
pub const QDMI_JOB_RESULT_PROBABILITIESSPARSEKEYS: QdmiJobResultType = 7;
pub const QDMI_JOB_RESULT_PROBABILITIESSPARSEVALUES: QdmiJobResultType = 8;

// ===========================================================================
// Function pointer types — QDMI device interface
//
// Every QDMI device library exports 18 functions with a device-specific
// prefix. For example the "MOCK" device exports:
//   MOCK_QDMI_device_initialize
//   MOCK_QDMI_device_finalize
//   MOCK_QDMI_device_session_alloc
//   MOCK_QDMI_device_session_set_parameter
//   MOCK_QDMI_device_session_init
//   MOCK_QDMI_device_session_free
//   MOCK_QDMI_device_session_query_device_property
//   MOCK_QDMI_device_session_query_site_property
//   MOCK_QDMI_device_session_query_operation_property
//   MOCK_QDMI_device_session_create_device_job
//   MOCK_QDMI_device_job_set_parameter
//   MOCK_QDMI_device_job_query_property
//   MOCK_QDMI_device_job_submit
//   MOCK_QDMI_device_job_cancel
//   MOCK_QDMI_device_job_check
//   MOCK_QDMI_device_job_wait
//   MOCK_QDMI_device_job_get_results
//   MOCK_QDMI_device_job_free
// ===========================================================================

// -- Device lifecycle (2) ---------------------------------------------------

/// `int PREFIX_QDMI_device_initialize(void)`
pub type FnDeviceInitialize = unsafe extern "C" fn() -> c_int;

/// `int PREFIX_QDMI_device_finalize(void)`
pub type FnDeviceFinalize = unsafe extern "C" fn() -> c_int;

// -- Session lifecycle (4) --------------------------------------------------

/// `int PREFIX_QDMI_device_session_alloc(PREFIX_QDMI_Device_Session *session)`
pub type FnSessionAlloc = unsafe extern "C" fn(session_out: *mut QdmiDeviceSession) -> c_int;

/// `int PREFIX_QDMI_device_session_set_parameter(session, param, size, value)`
pub type FnSessionSetParameter = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    param: QdmiDeviceSessionParameter,
    size: usize,
    value: *const c_void,
) -> c_int;

/// `int PREFIX_QDMI_device_session_init(PREFIX_QDMI_Device_Session session)`
pub type FnSessionInit = unsafe extern "C" fn(session: QdmiDeviceSession) -> c_int;

/// `void PREFIX_QDMI_device_session_free(PREFIX_QDMI_Device_Session session)`
pub type FnSessionFree = unsafe extern "C" fn(session: QdmiDeviceSession);

// -- Query interface (3) ----------------------------------------------------

/// `int PREFIX_QDMI_device_session_query_device_property(session, prop, size, value, size_ret)`
pub type FnQueryDeviceProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    prop: QdmiDeviceProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// `int PREFIX_QDMI_device_session_query_site_property(session, site, prop, size, value, size_ret)`
pub type FnQuerySiteProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    site: QdmiSite,
    prop: QdmiSiteProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// `int PREFIX_QDMI_device_session_query_operation_property(
///     session, operation, num_sites, sites, num_params, params, prop, size, value, size_ret)`
pub type FnQueryOperationProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    operation: QdmiOperation,
    num_sites: usize,
    sites: *const QdmiSite,
    num_params: usize,
    params: *const f64,
    prop: QdmiOperationProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

// -- Job interface (9) ------------------------------------------------------

/// `int PREFIX_QDMI_device_session_create_device_job(session, job_out)`
pub type FnCreateDeviceJob =
    unsafe extern "C" fn(session: QdmiDeviceSession, job_out: *mut QdmiDeviceJob) -> c_int;

/// `int PREFIX_QDMI_device_job_set_parameter(job, param, size, value)`
pub type FnJobSetParameter = unsafe extern "C" fn(
    job: QdmiDeviceJob,
    param: QdmiDeviceJobParameter,
    size: usize,
    value: *const c_void,
) -> c_int;

/// `int PREFIX_QDMI_device_job_query_property(job, prop, size, value, size_ret)`
pub type FnJobQueryProperty = unsafe extern "C" fn(
    job: QdmiDeviceJob,
    prop: QdmiDeviceJobProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// `int PREFIX_QDMI_device_job_submit(job)`
pub type FnJobSubmit = unsafe extern "C" fn(job: QdmiDeviceJob) -> c_int;

/// `int PREFIX_QDMI_device_job_cancel(job)`
pub type FnJobCancel = unsafe extern "C" fn(job: QdmiDeviceJob) -> c_int;

/// `int PREFIX_QDMI_device_job_check(job, status_out)`
pub type FnJobCheck = unsafe extern "C" fn(job: QdmiDeviceJob, status: *mut QdmiJobStatus) -> c_int;

/// `int PREFIX_QDMI_device_job_wait(job, timeout_ms)`
pub type FnJobWait = unsafe extern "C" fn(job: QdmiDeviceJob, timeout_ms: usize) -> c_int;

/// `int PREFIX_QDMI_device_job_get_results(job, result_type, size, value, size_ret)`
pub type FnJobGetResults = unsafe extern "C" fn(
    job: QdmiDeviceJob,
    result_type: QdmiJobResultType,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// `void PREFIX_QDMI_device_job_free(job)`
pub type FnJobFree = unsafe extern "C" fn(job: QdmiDeviceJob);
