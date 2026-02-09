// SPDX-License-Identifier: Apache-2.0
//! QDMI device session management.
//!
//! A session is the primary handle for all queries and job submissions to a
//! QDMI device. Sessions are created via [`DeviceSession::open`] and are
//! automatically torn down when dropped.

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
    /// Open a new session on the given device.
    pub fn open(device: &'dev QdmiDevice) -> Result<Self> {
        let mut handle: ffi::QdmiDeviceSession = std::ptr::null_mut();

        let ret = unsafe { (device.fn_session_init)(&mut handle) };

        if !ffi::is_success(ret) {
            return Err(QdmiError::SessionError(format!(
                "session init failed on device '{}' (code {})",
                device.prefix(),
                ret
            )));
        }

        if handle.is_null() {
            return Err(QdmiError::SessionError(
                "session init returned null handle".into(),
            ));
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
    ///
    /// Uses the standard QDMI two-phase pattern:
    /// 1. Call with `size = 0` and `value = null` to get the required size.
    /// 2. Allocate and call again with the correct size.
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
    pub fn raw_query_operation_property(
        &self,
        operation: ffi::QdmiOperation,
        prop: ffi::QdmiOperationProperty,
    ) -> Result<Vec<u8>> {
        // Phase 1
        let mut size: usize = 0;
        let ret = unsafe {
            (self.device.fn_query_operation_property)(
                self.handle,
                operation,
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
        // QDMI strings are null-terminated C strings.
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

    /// Query an operation property as `f64`. Returns `Ok(None)` if unsupported.
    pub fn query_operation_f64_optional(
        &self,
        operation: ffi::QdmiOperation,
        prop: ffi::QdmiOperationProperty,
    ) -> Result<Option<f64>> {
        match self.raw_query_operation_property(operation, prop) {
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
    // Job submission
    // -----------------------------------------------------------------------

    /// Submit a circuit for execution.
    pub fn submit_job(&self, circuit_bytes: &[u8]) -> Result<ffi::QdmiJob> {
        let submit_fn = self.device.fn_submit_job.ok_or_else(|| {
            QdmiError::NotSupported
        })?;

        let mut job: ffi::QdmiJob = std::ptr::null_mut();
        let ret = unsafe {
            submit_fn(
                self.handle,
                circuit_bytes.as_ptr(),
                circuit_bytes.len(),
                &mut job,
            )
        };
        check_qdmi_result(ret)?;

        Ok(job)
    }

    /// Query the status of a submitted job.
    pub fn query_job_status(&self, job: ffi::QdmiJob) -> Result<ffi::QdmiJobStatus> {
        let status_fn = self.device.fn_query_job_status.ok_or(QdmiError::NotSupported)?;

        let mut status: ffi::QdmiJobStatus = 0;
        let ret = unsafe { status_fn(self.handle, job, &mut status) };
        check_qdmi_result(ret)?;

        Ok(status)
    }
}

impl<'dev> Drop for DeviceSession<'dev> {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let ret = unsafe { (self.device.fn_session_deinit)(self.handle) };
            if !ffi::is_success(ret) {
                log::error!(
                    "session deinit failed for device '{}' (code {})",
                    self.device.prefix(),
                    ret
                );
            } else {
                log::debug!("closed session on device '{}'", self.device.prefix());
            }
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
