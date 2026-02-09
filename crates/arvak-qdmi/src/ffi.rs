// SPDX-License-Identifier: Apache-2.0
//! Raw FFI constants and type definitions for the QDMI C interface.
//!
//! These values must match the QDMI header definitions from
//! <https://github.com/Munich-Quantum-Software-Stack/QDMI>.
//!
//! The constants below are based on QDMI v1.2.x. If the upstream headers
//! change, regenerate or update these values accordingly.

use std::ffi::c_void;
use std::os::raw::c_int;

// ---------------------------------------------------------------------------
// Opaque handle types
// ---------------------------------------------------------------------------

/// Opaque device session handle. Each device prefix has its own concrete type,
/// but from the consumer side we treat it as `*mut c_void`.
pub type QdmiDeviceSession = *mut c_void;

/// Opaque site handle.
pub type QdmiSite = *mut c_void;

/// Opaque operation handle.
pub type QdmiOperation = *mut c_void;

/// Opaque job handle.
pub type QdmiJob = *mut c_void;

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

pub const QDMI_SUCCESS: c_int = 0;
pub const QDMI_ERROR_INVALIDARGUMENT: c_int = 1;
pub const QDMI_ERROR_NOTSUPPORTED: c_int = 2;
pub const QDMI_ERROR_OUTOFMEMORY: c_int = 3;
pub const QDMI_ERROR_FATAL: c_int = 4;
pub const QDMI_ERROR_NOTIMPLEMENTED: c_int = 5;
pub const QDMI_ERROR_TIMEOUT: c_int = 6;

// ---------------------------------------------------------------------------
// Device property keys
// ---------------------------------------------------------------------------

/// Device property enum type passed to `query_device_property`.
pub type QdmiDeviceProperty = c_int;

pub const QDMI_DEVICE_PROPERTY_NAME: QdmiDeviceProperty = 0;
pub const QDMI_DEVICE_PROPERTY_VERSION: QdmiDeviceProperty = 1;
pub const QDMI_DEVICE_PROPERTY_LIBRARYVERSION: QdmiDeviceProperty = 2;
pub const QDMI_DEVICE_PROPERTY_QUBITSNUM: QdmiDeviceProperty = 3;
pub const QDMI_DEVICE_PROPERTY_SITES: QdmiDeviceProperty = 4;
pub const QDMI_DEVICE_PROPERTY_COUPLINGMAP: QdmiDeviceProperty = 5;
pub const QDMI_DEVICE_PROPERTY_OPERATIONS: QdmiDeviceProperty = 6;

// ---------------------------------------------------------------------------
// Site property keys
// ---------------------------------------------------------------------------

pub type QdmiSiteProperty = c_int;

pub const QDMI_SITE_PROPERTY_T1: QdmiSiteProperty = 0;
pub const QDMI_SITE_PROPERTY_T2: QdmiSiteProperty = 1;
pub const QDMI_SITE_PROPERTY_READOUTERROR: QdmiSiteProperty = 2;
pub const QDMI_SITE_PROPERTY_READOUTDURATION: QdmiSiteProperty = 3;
pub const QDMI_SITE_PROPERTY_FREQUENCY: QdmiSiteProperty = 4;

// ---------------------------------------------------------------------------
// Operation property keys
// ---------------------------------------------------------------------------

pub type QdmiOperationProperty = c_int;

pub const QDMI_OPERATION_PROPERTY_NAME: QdmiOperationProperty = 0;
pub const QDMI_OPERATION_PROPERTY_DURATION: QdmiOperationProperty = 1;
pub const QDMI_OPERATION_PROPERTY_FIDELITY: QdmiOperationProperty = 2;
pub const QDMI_OPERATION_PROPERTY_QUBITSNUM: QdmiOperationProperty = 3;
pub const QDMI_OPERATION_PROPERTY_SITES: QdmiOperationProperty = 4;

// ---------------------------------------------------------------------------
// Job status values
// ---------------------------------------------------------------------------

pub type QdmiJobStatus = c_int;

pub const QDMI_JOB_STATUS_SUBMITTED: QdmiJobStatus = 0;
pub const QDMI_JOB_STATUS_RUNNING: QdmiJobStatus = 1;
pub const QDMI_JOB_STATUS_DONE: QdmiJobStatus = 2;
pub const QDMI_JOB_STATUS_ERROR: QdmiJobStatus = 3;
pub const QDMI_JOB_STATUS_CANCELLED: QdmiJobStatus = 4;

// ---------------------------------------------------------------------------
// Function pointer types for the QDMI device interface.
//
// Every QDMI device library exports these functions with a device-specific
// prefix: `{PREFIX}_QDMI_device_session_{name}`.
//
// For example the "EX" example device exports:
//   EX_QDMI_device_session_query_device_property
//   EX_QDMI_device_session_query_site_property
//   ...
// ---------------------------------------------------------------------------

/// `int PREFIX_QDMI_device_session_init(PREFIX_QDMI_Device_Session *session)`
pub type FnSessionInit = unsafe extern "C" fn(
    session_out: *mut QdmiDeviceSession,
) -> c_int;

/// `int PREFIX_QDMI_device_session_deinit(PREFIX_QDMI_Device_Session session)`
pub type FnSessionDeinit = unsafe extern "C" fn(
    session: QdmiDeviceSession,
) -> c_int;

/// ```text
/// int PREFIX_QDMI_device_session_query_device_property(
///     PREFIX_QDMI_Device_Session session,
///     QDMI_Device_Property prop,
///     size_t size,
///     void *value,
///     size_t *size_ret
/// )
/// ```
pub type FnQueryDeviceProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    prop: QdmiDeviceProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// Same signature pattern for site properties, with an additional site handle.
pub type FnQuerySiteProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    site: QdmiSite,
    prop: QdmiSiteProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// Same signature pattern for operation properties.
pub type FnQueryOperationProperty = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    operation: QdmiOperation,
    prop: QdmiOperationProperty,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// ```text
/// int PREFIX_QDMI_device_session_submit_job(
///     PREFIX_QDMI_Device_Session session,
///     const char *circuit,
///     size_t circuit_size,
///     PREFIX_QDMI_Job *job_out
/// )
/// ```
pub type FnSubmitJob = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    circuit: *const u8,
    circuit_size: usize,
    job_out: *mut QdmiJob,
) -> c_int;

/// `int PREFIX_QDMI_device_session_query_job_status(...)`
pub type FnQueryJobStatus = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    job: QdmiJob,
    status: *mut QdmiJobStatus,
) -> c_int;

/// `int PREFIX_QDMI_device_session_query_job_result(...)`
pub type FnQueryJobResult = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    job: QdmiJob,
    size: usize,
    value: *mut c_void,
    size_ret: *mut usize,
) -> c_int;

/// `int PREFIX_QDMI_device_session_cancel_job(...)`
pub type FnCancelJob = unsafe extern "C" fn(
    session: QdmiDeviceSession,
    job: QdmiJob,
) -> c_int;

// ---------------------------------------------------------------------------
// Helper: check whether a QDMI return code indicates success.
// ---------------------------------------------------------------------------

/// Returns `true` if the QDMI call succeeded.
#[inline]
pub fn is_success(code: c_int) -> bool {
    code == QDMI_SUCCESS
}
