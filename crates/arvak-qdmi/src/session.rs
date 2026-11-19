// SPDX-License-Identifier: Apache-2.0
//! QDMI device session management and job submission.
//!
//! A session is the primary handle for all queries and job submissions to a
//! QDMI device. Sessions follow a three-phase lifecycle:
//!
//! 1. **Alloc** – `session_alloc(&session)` allocates the handle
//! 2. **Configure** – `session_set_parameter(session, param, size, value)` (optional, repeatable)
//! 3. **Init** – `session_init(session)` connects/validates
//!
//! Sessions are automatically freed when dropped (RAII via `session_free`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::{CString, c_void};

use crate::device_loader::{JobGetResultsFn, QdmiAbi, QdmiDevice};
use crate::error::{QdmiError, Result};
use crate::ffi;

/// An active session with a QDMI device.
///
/// All query and job operations require a session. Sessions are not `Send` or
/// `Sync` — the QDMI spec does not guarantee thread-safety within a single
/// session.
pub struct DeviceSession<'dev> {
    pub(crate) handle: ffi::QdmiDeviceSession,
    pub(crate) device: &'dev QdmiDevice,
}

impl<'dev> DeviceSession<'dev> {
    /// Open a new session on the given device (no parameters).
    pub fn open(device: &'dev QdmiDevice) -> Result<Self> {
        Self::open_with_params(device, &HashMap::new())
    }

    /// Open a new session with device session parameters.
    ///
    /// Parameters are key-value pairs where keys are `QdmiDeviceSessionParameter`
    /// constants and values are the raw byte representations.
    pub fn open_with_params(
        device: &'dev QdmiDevice,
        params: &HashMap<ffi::QdmiDeviceSessionParameter, Vec<u8>>,
    ) -> Result<Self> {
        // Phase 1: Allocate
        let mut handle: ffi::QdmiDeviceSession = std::ptr::null_mut();
        let ret = unsafe { (device.fn_session_alloc)(&raw mut handle) };

        if !ffi::is_success(ret) {
            return Err(QdmiError::SessionError(format!(
                "session_alloc failed on device '{}' (code {})",
                device.prefix(),
                ret
            )));
        }

        if handle.is_null() {
            return Err(QdmiError::SessionError(
                "session_alloc returned null handle".into(),
            ));
        }

        // Phase 2: Set parameters (optional)
        for (&param, value) in params {
            let ret = unsafe {
                (device.fn_session_set_parameter)(
                    handle,
                    param,
                    value.len(),
                    value.as_ptr().cast::<c_void>(),
                )
            };
            if !ffi::is_success(ret) {
                // Free the allocated session before returning error
                unsafe { (device.fn_session_free)(handle) };
                return Err(QdmiError::SessionError(format!(
                    "session_set_parameter({}) failed on device '{}' (code {})",
                    param,
                    device.prefix(),
                    ret
                )));
            }
        }

        // Phase 3: Initialize
        let ret = unsafe { (device.fn_session_init)(handle) };
        if !ffi::is_success(ret) {
            // Free the allocated session before returning error
            unsafe { (device.fn_session_free)(handle) };
            return Err(QdmiError::SessionError(format!(
                "session_init failed on device '{}' (code {})",
                device.prefix(),
                ret
            )));
        }

        tracing::debug!(
            "opened session on device '{}' (handle {:?})",
            device.prefix(),
            handle
        );

        Ok(Self { handle, device })
    }

    /// The underlying device this session belongs to.
    pub fn device(&self) -> &QdmiDevice {
        self.device
    }

    /// Whether the session handle is valid (non-null).
    pub fn is_active(&self) -> bool {
        !self.handle.is_null()
    }

    // -----------------------------------------------------------------------
    // Raw property queries (two-phase: size probe → data read)
    // -----------------------------------------------------------------------

    /// Query a device-level property. Returns the raw byte buffer.
    ///
    // SAFETY: Trusts QDMI library to return consistent sizes between queries.
    // The two-phase (size probe, then data read) pattern has no overflow protection
    // if the library returns a larger size on the second call than the first.
    pub fn raw_query_device_property(&self, prop: ffi::QdmiDeviceProperty) -> Result<Vec<u8>> {
        // Phase 1: size probe
        let mut size: usize = 0;
        let ret = unsafe {
            (self.device.fn_query_device_property)(
                self.handle,
                prop,
                0,
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        check_qdmi_result(ret)?;

        if size == 0 {
            return Ok(Vec::new());
        }

        // Phase 2: data read
        let mut buf = vec![0u8; size];
        let ret = unsafe {
            (self.device.fn_query_device_property)(
                self.handle,
                prop,
                size,
                buf.as_mut_ptr().cast::<c_void>(),
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }

    /// Query a site-level property for a specific site. Returns raw bytes.
    #[allow(clippy::not_unsafe_ptr_arg_deref, clippy::similar_names)] // opaque QDMI handle, deref is in C via FFI
    pub fn raw_query_site_property(
        &self,
        site: ffi::QdmiSite,
        prop: ffi::QdmiSiteProperty,
    ) -> Result<Vec<u8>> {
        // Phase 1
        let mut size: usize = 0;
        let ret = unsafe {
            (self.device.fn_query_site_property)(
                self.handle,
                site,
                prop,
                0,
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        check_qdmi_result(ret)?;

        if size == 0 {
            return Ok(Vec::new());
        }

        // Phase 2
        let mut buf = vec![0u8; size];
        let ret = unsafe {
            (self.device.fn_query_site_property)(
                self.handle,
                site,
                prop,
                size,
                buf.as_mut_ptr().cast::<c_void>(),
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }

    /// Query an operation-level property. Returns raw bytes.
    ///
    /// The QDMI v1.3.2 operation query takes additional site and parameter
    /// arrays for site-dependent properties. Pass `(&[], &[])` for
    /// site/parameter-independent queries.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // opaque QDMI handles, deref is in C via FFI
    pub fn raw_query_operation_property(
        &self,
        operation: ffi::QdmiOperation,
        sites: &[ffi::QdmiSite],
        params: &[f64],
        prop: ffi::QdmiOperationProperty,
    ) -> Result<Vec<u8>> {
        let sites_ptr = if sites.is_empty() {
            std::ptr::null()
        } else {
            sites.as_ptr()
        };
        let params_ptr = if params.is_empty() {
            std::ptr::null()
        } else {
            params.as_ptr()
        };

        // Phase 1
        let mut size: usize = 0;
        let ret = unsafe {
            (self.device.fn_query_operation_property)(
                self.handle,
                operation,
                sites.len(),
                sites_ptr,
                params.len(),
                params_ptr,
                prop,
                0,
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        check_qdmi_result(ret)?;

        if size == 0 {
            return Ok(Vec::new());
        }

        // Phase 2
        let mut buf = vec![0u8; size];
        let ret = unsafe {
            (self.device.fn_query_operation_property)(
                self.handle,
                operation,
                sites.len(),
                sites_ptr,
                params.len(),
                params_ptr,
                prop,
                size,
                buf.as_mut_ptr().cast::<c_void>(),
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }

    // -----------------------------------------------------------------------
    // Typed convenience queries
    // -----------------------------------------------------------------------

    /// Query a device property and interpret the result as a string.
    pub fn query_device_string(&self, prop: ffi::QdmiDeviceProperty) -> Result<String> {
        let buf = self.raw_query_device_property(prop)?;
        let s = std::ffi::CStr::from_bytes_until_nul(&buf)
            .map_err(|_| QdmiError::ParseError("invalid C string in property response".into()))?
            .to_str()
            .map_err(|e| QdmiError::ParseError(format!("invalid UTF-8: {e}")))?
            .to_string();
        Ok(s)
    }

    /// Query a device property and interpret the result as a `usize`.
    pub fn query_device_usize(&self, prop: ffi::QdmiDeviceProperty) -> Result<usize> {
        let buf = self.raw_query_device_property(prop)?;
        if buf.len() < std::mem::size_of::<usize>() {
            return Err(QdmiError::ParseError(format!(
                "expected {} bytes for usize, got {}",
                std::mem::size_of::<usize>(),
                buf.len()
            )));
        }
        let value = usize::from_ne_bytes(
            buf[..std::mem::size_of::<usize>()]
                .try_into()
                .map_err(|_| QdmiError::ParseError("usize conversion failed".into()))?,
        );
        Ok(value)
    }

    /// Query a device property and interpret the result as an `f64`.
    pub fn query_device_f64(&self, prop: ffi::QdmiDeviceProperty) -> Result<f64> {
        let buf = self.raw_query_device_property(prop)?;
        if buf.len() < std::mem::size_of::<f64>() {
            return Err(QdmiError::ParseError(format!(
                "expected {} bytes for f64, got {}",
                std::mem::size_of::<f64>(),
                buf.len()
            )));
        }
        let value = f64::from_ne_bytes(
            buf[..std::mem::size_of::<f64>()]
                .try_into()
                .map_err(|_| QdmiError::ParseError("f64 conversion failed".into()))?,
        );
        Ok(value)
    }

    /// Query a device property and interpret the result as a `u64`.
    pub fn query_device_u64(&self, prop: ffi::QdmiDeviceProperty) -> Result<u64> {
        let buf = self.raw_query_device_property(prop)?;
        if buf.len() < std::mem::size_of::<u64>() {
            return Err(QdmiError::ParseError(format!(
                "expected {} bytes for u64, got {}",
                std::mem::size_of::<u64>(),
                buf.len()
            )));
        }
        let value = u64::from_ne_bytes(
            buf[..std::mem::size_of::<u64>()]
                .try_into()
                .map_err(|_| QdmiError::ParseError("u64 conversion failed".into()))?,
        );
        Ok(value)
    }

    /// Query a site property as `f64`. Returns `Ok(None)` for unsupported props.
    pub fn query_site_f64_optional(
        &self,
        site: ffi::QdmiSite,
        prop: ffi::QdmiSiteProperty,
    ) -> Result<Option<f64>> {
        match self.raw_query_site_property(site, prop) {
            Ok(buf) => {
                if buf.len() < std::mem::size_of::<f64>() {
                    return Err(QdmiError::ParseError(format!(
                        "expected {} bytes for f64, got {}",
                        std::mem::size_of::<f64>(),
                        buf.len()
                    )));
                }
                let v = f64::from_ne_bytes(
                    buf[..std::mem::size_of::<f64>()]
                        .try_into()
                        .map_err(|_| QdmiError::ParseError("f64 conversion".into()))?,
                );
                Ok(Some(v))
            }
            Err(QdmiError::NotSupported) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Query a site property as `u64`. Returns `Ok(None)` for unsupported props.
    pub fn query_site_u64_optional(
        &self,
        site: ffi::QdmiSite,
        prop: ffi::QdmiSiteProperty,
    ) -> Result<Option<u64>> {
        match self.raw_query_site_property(site, prop) {
            Ok(buf) => {
                if buf.len() < std::mem::size_of::<u64>() {
                    return Err(QdmiError::ParseError(format!(
                        "expected {} bytes for u64, got {}",
                        std::mem::size_of::<u64>(),
                        buf.len()
                    )));
                }
                let v = u64::from_ne_bytes(
                    buf[..std::mem::size_of::<u64>()]
                        .try_into()
                        .map_err(|_| QdmiError::ParseError("u64 conversion".into()))?,
                );
                Ok(Some(v))
            }
            Err(QdmiError::NotSupported) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Query an operation property as `f64`. Returns `Ok(None)` if unsupported.
    pub fn query_operation_f64_optional(
        &self,
        operation: ffi::QdmiOperation,
        prop: ffi::QdmiOperationProperty,
    ) -> Result<Option<f64>> {
        match self.raw_query_operation_property(operation, &[], &[], prop) {
            Ok(buf) => {
                if buf.len() < std::mem::size_of::<f64>() {
                    return Err(QdmiError::ParseError(format!(
                        "expected {} bytes for f64, got {}",
                        std::mem::size_of::<f64>(),
                        buf.len()
                    )));
                }
                let v = f64::from_ne_bytes(
                    buf[..std::mem::size_of::<f64>()]
                        .try_into()
                        .map_err(|_| QdmiError::ParseError("f64 conversion".into()))?,
                );
                Ok(Some(v))
            }
            Err(QdmiError::NotSupported) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------
    // Job management
    // -----------------------------------------------------------------------

    /// Create a new device job for this session.
    pub fn create_job(&self) -> Result<DeviceJob<'_, 'dev>> {
        let create_fn = self
            .device
            .fn_create_device_job
            .ok_or(QdmiError::NotSupported)?;

        let mut job: ffi::QdmiDeviceJob = std::ptr::null_mut();
        let ret = unsafe { create_fn(self.handle, &raw mut job) };
        check_qdmi_result(ret)?;

        if job.is_null() {
            return Err(QdmiError::SessionError(
                "create_device_job returned null handle".into(),
            ));
        }

        Ok(DeviceJob {
            handle: job,
            session: self,
        })
    }

    /// Re-attach to an existing job by the ID from [`DeviceJob::id`].
    ///
    /// QDMI ≥ 1.4 only; returns [`QdmiError::NotSupported`] on 1.3 devices
    /// and on 1.4 devices that cannot reconstruct the job.
    pub fn retrieve_job(&self, job_id: &str) -> Result<DeviceJob<'_, 'dev>> {
        let retrieve_fn = self
            .device
            .fn_retrieve_device_job_by_id
            .ok_or(QdmiError::NotSupported)?;
        let c_id = CString::new(job_id).map_err(|_| QdmiError::InvalidArgument)?;

        let mut job: ffi::QdmiDeviceJob = std::ptr::null_mut();
        let ret = unsafe { retrieve_fn(self.handle, c_id.as_ptr(), &raw mut job) };
        check_qdmi_result(ret)?;

        if job.is_null() {
            return Err(QdmiError::SessionError(
                "retrieve_device_job_by_id returned null handle".into(),
            ));
        }

        Ok(DeviceJob {
            handle: job,
            session: self,
        })
    }
}

impl Drop for DeviceSession<'_> {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.device.fn_session_free)(self.handle) };
            tracing::debug!("freed session on device '{}'", self.device.prefix());
        }
    }
}

// ===========================================================================
// Device Job
// ===========================================================================

/// A QDMI device job with RAII lifetime management.
///
/// Jobs follow a multi-step lifecycle:
/// 1. Create via [`DeviceSession::create_job`]
/// 2. Set the program via [`DeviceJob::set_program`] and shots via
///    [`DeviceJob::set_parameter`]
/// 3. Submit via [`DeviceJob::submit`]
/// 4. Poll via [`DeviceJob::check`] or block via [`DeviceJob::wait`]
/// 5. Retrieve results via [`DeviceJob::get_results`]
///
/// The job is automatically freed when dropped.
pub struct DeviceJob<'sess, 'dev> {
    handle: ffi::QdmiDeviceJob,
    session: &'sess DeviceSession<'dev>,
}

impl DeviceJob<'_, '_> {
    /// Set a single program for this job, independent of the device's ABI.
    ///
    /// On QDMI 1.4 devices this calls `set_programs` with one program; on
    /// 1.2/1.3 devices it sets the `PROGRAMFORMAT` and `PROGRAM` parameters.
    pub fn set_program(&self, format: ffi::QdmiProgramFormat, program: &[u8]) -> Result<()> {
        self.set_programs(format, &[program])
    }

    /// Set an ordered list of programs sharing one format and shot count.
    ///
    /// Results for program `i` are read with [`DeviceJob::get_program_results`].
    /// QDMI 1.2/1.3 devices accept exactly one program; longer lists return
    /// [`QdmiError::NotSupported`], as do 1.4 devices without list support.
    ///
    /// Text formats get the single trailing `'\0'` that QDMI 1.4 requires
    /// appended if it is missing.
    pub fn set_programs(&self, format: ffi::QdmiProgramFormat, programs: &[&[u8]]) -> Result<()> {
        if programs.is_empty() || programs.iter().any(|p| p.is_empty()) {
            return Err(QdmiError::InvalidArgument);
        }

        match self.session.device.abi() {
            QdmiAbi::V1_4 => self.set_programs_v14(format, programs),
            QdmiAbi::V1_3 => {
                let [program] = programs else {
                    return Err(QdmiError::NotSupported);
                };
                self.set_parameter(
                    ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAMFORMAT,
                    &format.to_ne_bytes(),
                )?;
                self.set_parameter(ffi::QDMI_DEVICE_JOB_PARAMETER_PROGRAM, program)
            }
        }
    }

    fn set_programs_v14(&self, format: ffi::QdmiProgramFormat, programs: &[&[u8]]) -> Result<()> {
        let set_fn = self
            .session
            .device
            .fn_job_set_programs
            .ok_or(QdmiError::NotSupported)?;

        let payloads: Vec<Cow<'_, [u8]>> = programs
            .iter()
            .map(|p| {
                if ffi::is_text_program_format(format) && p.last() != Some(&0) {
                    Cow::Owned([*p, &[0u8]].concat())
                } else {
                    Cow::Borrowed(*p)
                }
            })
            .collect();
        let sizes: Vec<usize> = payloads.iter().map(|p| p.len()).collect();
        let ptrs: Vec<*const c_void> = payloads
            .iter()
            .map(|p| p.as_ptr().cast::<c_void>())
            .collect();

        // SAFETY: `sizes` and `ptrs` both hold `payloads.len()` elements and
        // every pointer refers to a live buffer of the matching size; the
        // device deep-copies all data before returning.
        let ret = unsafe {
            set_fn(
                self.handle,
                format,
                payloads.len(),
                sizes.as_ptr(),
                ptrs.as_ptr(),
            )
        };
        check_qdmi_result(ret)
    }

    /// Set a raw job parameter (e.g. `SHOTSNUM`, which takes a `size_t`).
    ///
    /// Prefer [`DeviceJob::set_program`] for programs: the `PROGRAMFORMAT` and
    /// `PROGRAM` parameters only exist up to QDMI 1.3.
    pub fn set_parameter(&self, param: ffi::QdmiDeviceJobParameter, value: &[u8]) -> Result<()> {
        let set_fn = self
            .session
            .device
            .fn_job_set_parameter
            .ok_or(QdmiError::NotSupported)?;

        let ret = unsafe {
            set_fn(
                self.handle,
                param,
                value.len(),
                value.as_ptr().cast::<c_void>(),
            )
        };
        check_qdmi_result(ret)
    }

    /// Submit the job for execution.
    pub fn submit(&self) -> Result<()> {
        let submit_fn = self
            .session
            .device
            .fn_job_submit
            .ok_or(QdmiError::NotSupported)?;

        let ret = unsafe { submit_fn(self.handle) };
        check_qdmi_result(ret)
    }

    /// Check the current status of the job (non-blocking).
    pub fn check(&self) -> Result<ffi::QdmiJobStatus> {
        let check_fn = self
            .session
            .device
            .fn_job_check
            .ok_or(QdmiError::NotSupported)?;

        let mut status: ffi::QdmiJobStatus = 0;
        let ret = unsafe { check_fn(self.handle, &raw mut status) };
        check_qdmi_result(ret)?;
        Ok(status)
    }

    /// Wait for the job to complete (blocking).
    ///
    /// `timeout_ms` is in milliseconds. Pass 0 for infinite wait.
    pub fn wait(&self, timeout_ms: usize) -> Result<()> {
        let wait_fn = self
            .session
            .device
            .fn_job_wait
            .ok_or(QdmiError::NotSupported)?;

        let ret = unsafe { wait_fn(self.handle, timeout_ms) };
        check_qdmi_result(ret)
    }

    /// Cancel the job.
    pub fn cancel(&self) -> Result<()> {
        let cancel_fn = self
            .session
            .device
            .fn_job_cancel
            .ok_or(QdmiError::NotSupported)?;

        let ret = unsafe { cancel_fn(self.handle) };
        check_qdmi_result(ret)
    }

    /// Get results from a completed job (first program). Uses the two-phase
    /// query pattern.
    pub fn get_results(&self, result_type: ffi::QdmiJobResultType) -> Result<Vec<u8>> {
        self.get_program_results(0, result_type)
    }

    /// Get results for the program at `program_index` (zero-based).
    ///
    /// On QDMI 1.2/1.3 devices only index 0 exists; other indices return
    /// `QDMI_ERROR_OUTOFRANGE` as a [`QdmiError::DeviceError`].
    pub fn get_program_results(
        &self,
        program_index: usize,
        result_type: ffi::QdmiJobResultType,
    ) -> Result<Vec<u8>> {
        let get_fn = self
            .session
            .device
            .fn_job_get_results
            .ok_or(QdmiError::NotSupported)?;

        if matches!(get_fn, JobGetResultsFn::V1_3(_)) && program_index != 0 {
            return Err(QdmiError::from_code(ffi::QDMI_ERROR_OUTOFRANGE));
        }

        // SAFETY: the variant was resolved with the signature matching the
        // device's ABI; `value`/`size_ret` follow the two-phase contract.
        let call = |size: usize, value: *mut c_void, size_ret: *mut usize| unsafe {
            match get_fn {
                JobGetResultsFn::V1_4(f) => f(
                    self.handle,
                    program_index,
                    result_type,
                    size,
                    value,
                    size_ret,
                ),
                JobGetResultsFn::V1_3(f) => f(self.handle, result_type, size, value, size_ret),
            }
        };
        two_phase_read(call)
    }

    /// Fetch the bytes of the program at `program_index` back from the device.
    ///
    /// QDMI ≥ 1.4 only; text programs include their trailing `'\0'`.
    pub fn get_program(&self, program_index: usize) -> Result<Vec<u8>> {
        let get_fn = self
            .session
            .device
            .fn_job_get_program
            .ok_or(QdmiError::NotSupported)?;

        // SAFETY: signature resolved from a QDMI 1.4 device; two-phase contract.
        two_phase_read(|size, data, size_ret| unsafe {
            get_fn(self.handle, program_index, size, data, size_ret)
        })
    }

    /// Status of a single program within the job.
    ///
    /// QDMI ≥ 1.4 only, and optional for devices: returns
    /// [`QdmiError::NotSupported`] if the device only reports aggregate status
    /// (use [`DeviceJob::check`] then).
    pub fn program_status(&self, program_index: usize) -> Result<ffi::QdmiJobStatus> {
        let status_fn = self
            .session
            .device
            .fn_job_get_program_status
            .ok_or(QdmiError::NotSupported)?;

        let mut status: ffi::QdmiJobStatus = 0;
        let ret = unsafe { status_fn(self.handle, program_index, &raw mut status) };
        check_qdmi_result(ret)?;
        Ok(status)
    }

    /// Query a raw job property. Uses the two-phase query pattern.
    pub fn query_property(&self, prop: ffi::QdmiDeviceJobProperty) -> Result<Vec<u8>> {
        let query_fn = self
            .session
            .device
            .fn_job_query_property
            .ok_or(QdmiError::NotSupported)?;

        // SAFETY: two-phase contract of `QDMI_device_job_query_property`.
        two_phase_read(|size, value, size_ret| unsafe {
            query_fn(self.handle, prop, size, value, size_ret)
        })
    }

    /// The job's device-assigned ID (`QDMI_DEVICE_JOB_PROPERTY_ID`).
    pub fn id(&self) -> Result<String> {
        let buf = self.query_property(ffi::QDMI_DEVICE_JOB_PROPERTY_ID)?;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..end].to_vec())
            .map_err(|e| QdmiError::ParseError(format!("job ID is not UTF-8: {e}")))
    }
}

impl Drop for DeviceJob<'_, '_> {
    fn drop(&mut self) {
        if let Some(free_fn) = self.session.device.fn_job_free {
            unsafe { free_fn(self.handle) };
            tracing::debug!("freed job on device '{}'", self.session.device.prefix());
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Run the QDMI two-phase buffer query: size probe, then data read.
fn two_phase_read(call: impl Fn(usize, *mut c_void, *mut usize) -> i32) -> Result<Vec<u8>> {
    let mut size: usize = 0;
    check_qdmi_result(call(0, std::ptr::null_mut(), &raw mut size))?;

    if size == 0 {
        return Ok(Vec::new());
    }

    let mut buf = vec![0u8; size];
    check_qdmi_result(call(
        size,
        buf.as_mut_ptr().cast::<c_void>(),
        std::ptr::null_mut(),
    ))?;
    Ok(buf)
}

fn check_qdmi_result(code: i32) -> Result<()> {
    if ffi::is_success(code) {
        Ok(())
    } else {
        Err(QdmiError::from_code(code))
    }
}
