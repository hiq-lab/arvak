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

use std::collections::HashMap;
use std::ffi::c_void;

use crate::device_loader::QdmiDevice;
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
        let ret = unsafe { (device.fn_session_alloc)(&mut handle) };

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
                (device.fn_session_set_parameter)(handle, param, value.len(), value.as_ptr() as *const c_void)
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

        log::debug!(
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
    pub fn raw_query_device_property(&self, prop: ffi::QdmiDeviceProperty) -> Result<Vec<u8>> {
        // Phase 1: size probe
        let mut size: usize = 0;
        let ret = unsafe {
            (self.device.fn_query_device_property)(
                self.handle,
                prop,
                0,
                std::ptr::null_mut(),
                &mut size,
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
                buf.as_mut_ptr() as *mut c_void,
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }

    /// Query a site-level property for a specific site. Returns raw bytes.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // opaque QDMI handle, deref is in C via FFI
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
                &mut size,
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
                buf.as_mut_ptr() as *mut c_void,
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }

    /// Query an operation-level property. Returns raw bytes.
    ///
    /// The QDMI v1.2.1 operation query takes additional site and parameter
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
                &mut size,
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
                buf.as_mut_ptr() as *mut c_void,
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
        let ret = unsafe { create_fn(self.handle, &mut job) };
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
}

impl<'dev> Drop for DeviceSession<'dev> {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.device.fn_session_free)(self.handle) };
            log::debug!("freed session on device '{}'", self.device.prefix());
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
/// 2. Set parameters (program, format, shots) via [`DeviceJob::set_parameter`]
/// 3. Submit via [`DeviceJob::submit`]
/// 4. Poll via [`DeviceJob::check`] or block via [`DeviceJob::wait`]
/// 5. Retrieve results via [`DeviceJob::get_results`]
///
/// The job is automatically freed when dropped.
pub struct DeviceJob<'sess, 'dev> {
    handle: ffi::QdmiDeviceJob,
    session: &'sess DeviceSession<'dev>,
}

impl<'sess, 'dev> DeviceJob<'sess, 'dev> {
    /// Set a job parameter (e.g. program format, program, shots).
    pub fn set_parameter(
        &self,
        param: ffi::QdmiDeviceJobParameter,
        value: &[u8],
    ) -> Result<()> {
        let set_fn = self
            .session
            .device
            .fn_job_set_parameter
            .ok_or(QdmiError::NotSupported)?;

        let ret =
            unsafe { set_fn(self.handle, param, value.len(), value.as_ptr() as *const c_void) };
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
        let ret = unsafe { check_fn(self.handle, &mut status) };
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

    /// Get results from a completed job. Uses the two-phase query pattern.
    pub fn get_results(&self, result_type: ffi::QdmiJobResultType) -> Result<Vec<u8>> {
        let get_fn = self
            .session
            .device
            .fn_job_get_results
            .ok_or(QdmiError::NotSupported)?;

        // Phase 1: size probe
        let mut size: usize = 0;
        let ret = unsafe { get_fn(self.handle, result_type, 0, std::ptr::null_mut(), &mut size) };
        check_qdmi_result(ret)?;

        if size == 0 {
            return Ok(Vec::new());
        }

        // Phase 2: data read
        let mut buf = vec![0u8; size];
        let ret = unsafe {
            get_fn(
                self.handle,
                result_type,
                size,
                buf.as_mut_ptr() as *mut c_void,
                std::ptr::null_mut(),
            )
        };
        check_qdmi_result(ret)?;

        Ok(buf)
    }
}

impl Drop for DeviceJob<'_, '_> {
    fn drop(&mut self) {
        if let Some(free_fn) = self.session.device.fn_job_free {
            unsafe { free_fn(self.handle) };
            log::debug!("freed job on device '{}'", self.session.device.prefix());
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn check_qdmi_result(code: i32) -> Result<()> {
    if ffi::is_success(code) {
        Ok(())
    } else {
        Err(QdmiError::from_code(code))
    }
}
