// SPDX-License-Identifier: Apache-2.0
//! Load QDMI device shared libraries and resolve prefixed symbols.
//!
//! QDMI devices export their functions with a device-specific prefix, e.g.
//! the example device uses `EX`:
//!
//! ```text
//! EX_QDMI_device_initialize
//! EX_QDMI_device_finalize
//! EX_QDMI_device_session_alloc
//! EX_QDMI_device_session_set_parameter
//! EX_QDMI_device_session_init
//! EX_QDMI_device_session_free
//! EX_QDMI_device_session_query_device_property
//! ...
//! ```
//!
//! This module handles the `dlopen` + prefix-aware `dlsym` dance, following the
//! same pattern as MQT Core's `Driver.cpp`.

use std::path::Path;

use libloading::{Library, Symbol};

use crate::error::{QdmiError, Result};
use crate::ffi;

// ---------------------------------------------------------------------------
// QDMI device function table
// ---------------------------------------------------------------------------

/// A loaded QDMI device with all function pointers resolved.
///
/// The library handle is kept alive for the lifetime of this struct so the
/// loaded `.so` is not unloaded while we still hold function pointers into it.
pub struct QdmiDevice {
    /// Prevent the shared library from being unloaded.
    _library: Library,

    /// The device-specific prefix (e.g. "EX", "DDSIM", "NA").
    prefix: String,

    /// Path the library was loaded from (for diagnostics).
    library_path: String,

    // -- Device lifecycle ----------------------------------------------------
    #[allow(dead_code)]
    pub(crate) fn_device_initialize: ffi::FnDeviceInitialize,
    pub(crate) fn_device_finalize: ffi::FnDeviceFinalize,

    // -- Session lifecycle ---------------------------------------------------
    pub(crate) fn_session_alloc: ffi::FnSessionAlloc,
    pub(crate) fn_session_set_parameter: ffi::FnSessionSetParameter,
    pub(crate) fn_session_init: ffi::FnSessionInit,
    pub(crate) fn_session_free: ffi::FnSessionFree,

    // -- Query interface -----------------------------------------------------
    pub(crate) fn_query_device_property: ffi::FnQueryDeviceProperty,
    pub(crate) fn_query_site_property: ffi::FnQuerySiteProperty,
    pub(crate) fn_query_operation_property: ffi::FnQueryOperationProperty,

    // -- Job interface (optional — some devices are query-only) ---------------
    pub(crate) fn_create_device_job: Option<ffi::FnCreateDeviceJob>,
    pub(crate) fn_job_set_parameter: Option<ffi::FnJobSetParameter>,
    #[allow(dead_code)]
    pub(crate) fn_job_query_property: Option<ffi::FnJobQueryProperty>,
    pub(crate) fn_job_submit: Option<ffi::FnJobSubmit>,
    pub(crate) fn_job_cancel: Option<ffi::FnJobCancel>,
    pub(crate) fn_job_check: Option<ffi::FnJobCheck>,
    pub(crate) fn_job_wait: Option<ffi::FnJobWait>,
    pub(crate) fn_job_get_results: Option<ffi::FnJobGetResults>,
    pub(crate) fn_job_free: Option<ffi::FnJobFree>,
}

impl QdmiDevice {
    /// Load a QDMI device shared library and resolve all function pointers.
    ///
    /// # Arguments
    ///
    /// * `path`   – Path to the `.so` / `.dylib` file.
    /// * `prefix` – The device prefix used for symbol name-shifting (e.g. `"EX"`).
    ///
    /// # Errors
    ///
    /// Returns [`QdmiError::LoadFailed`] if `dlopen` fails, or
    /// [`QdmiError::SymbolNotFound`] if a required symbol cannot be resolved.
    #[allow(clippy::too_many_lines)]
    pub fn load(path: &Path, prefix: &str) -> Result<Self> {
        let path_str = path.display().to_string();

        // SAFETY: we are loading an external shared library. The caller is
        // responsible for ensuring the library is trustworthy.
        let library = unsafe { Library::new(path) }.map_err(|e| QdmiError::LoadFailed {
            path: path_str.clone(),
            cause: e.to_string(),
        })?;

        tracing::info!("loaded QDMI device library '{path_str}' with prefix '{prefix}'");

        // -- Device lifecycle (required) ----------------------------------------

        let fn_device_initialize = resolve_required::<ffi::FnDeviceInitialize>(
            &library,
            prefix,
            "QDMI_device_initialize",
            &path_str,
        )?;
        let fn_device_finalize = resolve_required::<ffi::FnDeviceFinalize>(
            &library,
            prefix,
            "QDMI_device_finalize",
            &path_str,
        )?;

        // -- Session lifecycle (required) ---------------------------------------

        let fn_session_alloc = resolve_required::<ffi::FnSessionAlloc>(
            &library,
            prefix,
            "QDMI_device_session_alloc",
            &path_str,
        )?;
        let fn_session_set_parameter = resolve_required::<ffi::FnSessionSetParameter>(
            &library,
            prefix,
            "QDMI_device_session_set_parameter",
            &path_str,
        )?;
        let fn_session_init = resolve_required::<ffi::FnSessionInit>(
            &library,
            prefix,
            "QDMI_device_session_init",
            &path_str,
        )?;
        let fn_session_free = resolve_required::<ffi::FnSessionFree>(
            &library,
            prefix,
            "QDMI_device_session_free",
            &path_str,
        )?;

        // -- Query interface (required) -----------------------------------------

        let fn_query_device_property = resolve_required::<ffi::FnQueryDeviceProperty>(
            &library,
            prefix,
            "QDMI_device_session_query_device_property",
            &path_str,
        )?;
        let fn_query_site_property = resolve_required::<ffi::FnQuerySiteProperty>(
            &library,
            prefix,
            "QDMI_device_session_query_site_property",
            &path_str,
        )?;
        let fn_query_operation_property = resolve_required::<ffi::FnQueryOperationProperty>(
            &library,
            prefix,
            "QDMI_device_session_query_operation_property",
            &path_str,
        )?;

        // -- Job interface (optional) -------------------------------------------

        let fn_create_device_job = resolve_optional::<ffi::FnCreateDeviceJob>(
            &library,
            prefix,
            "QDMI_device_session_create_device_job",
        );
        let fn_job_set_parameter = resolve_optional::<ffi::FnJobSetParameter>(
            &library,
            prefix,
            "QDMI_device_job_set_parameter",
        );
        let fn_job_query_property = resolve_optional::<ffi::FnJobQueryProperty>(
            &library,
            prefix,
            "QDMI_device_job_query_property",
        );
        let fn_job_submit =
            resolve_optional::<ffi::FnJobSubmit>(&library, prefix, "QDMI_device_job_submit");
        let fn_job_cancel =
            resolve_optional::<ffi::FnJobCancel>(&library, prefix, "QDMI_device_job_cancel");
        let fn_job_check =
            resolve_optional::<ffi::FnJobCheck>(&library, prefix, "QDMI_device_job_check");
        let fn_job_wait =
            resolve_optional::<ffi::FnJobWait>(&library, prefix, "QDMI_device_job_wait");
        let fn_job_get_results = resolve_optional::<ffi::FnJobGetResults>(
            &library,
            prefix,
            "QDMI_device_job_get_results",
        );
        let fn_job_free =
            resolve_optional::<ffi::FnJobFree>(&library, prefix, "QDMI_device_job_free");

        if fn_create_device_job.is_none() {
            tracing::warn!("device '{prefix}' does not export job submission functions");
        }

        // -- Call device_initialize immediately after loading -------------------

        let ret = unsafe { fn_device_initialize() };
        if !ffi::is_success(ret) {
            return Err(QdmiError::SessionError(format!(
                "device_initialize failed for '{prefix}' (code {ret})"
            )));
        }

        tracing::debug!("device '{prefix}' initialized successfully");

        Ok(Self {
            _library: library,
            prefix: prefix.to_string(),
            library_path: path_str,
            fn_device_initialize,
            fn_device_finalize,
            fn_session_alloc,
            fn_session_set_parameter,
            fn_session_init,
            fn_session_free,
            fn_query_device_property,
            fn_query_site_property,
            fn_query_operation_property,
            fn_create_device_job,
            fn_job_set_parameter,
            fn_job_query_property,
            fn_job_submit,
            fn_job_cancel,
            fn_job_check,
            fn_job_wait,
            fn_job_get_results,
            fn_job_free,
        })
    }

    /// The device-specific prefix (e.g. `"EX"`, `"DDSIM"`).
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Filesystem path the library was loaded from.
    pub fn library_path(&self) -> &str {
        &self.library_path
    }

    /// Whether the device supports job submission.
    pub fn supports_jobs(&self) -> bool {
        self.fn_create_device_job.is_some()
    }
}

impl Drop for QdmiDevice {
    fn drop(&mut self) {
        let ret = unsafe { (self.fn_device_finalize)() };
        if ffi::is_success(ret) {
            tracing::debug!("device '{}' finalized", self.prefix);
        } else {
            tracing::error!(
                "device_finalize failed for '{}' (code {})",
                self.prefix,
                ret
            );
        }
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for QdmiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QdmiDevice")
            .field("prefix", &self.prefix)
            .field("library_path", &self.library_path)
            .field("supports_jobs", &self.supports_jobs())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Symbol resolution helpers
// ---------------------------------------------------------------------------

/// Construct the full prefixed symbol name: `{PREFIX}_{base_name}`.
fn prefixed_symbol(prefix: &str, base_name: &str) -> String {
    format!("{prefix}_{base_name}")
}

/// Resolve a required symbol. Returns an error if the symbol is missing.
fn resolve_required<T: Copy>(
    library: &Library,
    prefix: &str,
    base_name: &str,
    _lib_path: &str,
) -> Result<T> {
    let sym_name = prefixed_symbol(prefix, base_name);
    tracing::trace!("resolving required symbol '{sym_name}'");

    // SAFETY: The caller guarantees the type `T` matches the actual function
    // signature exported by the library. This is the core FFI contract.
    unsafe {
        let sym: Symbol<T> =
            library
                .get(sym_name.as_bytes())
                .map_err(|e| QdmiError::SymbolNotFound {
                    symbol: sym_name.clone(),
                    cause: e.to_string(),
                })?;
        Ok(*sym)
    }
}

/// Resolve an optional symbol. Returns `None` if the symbol is missing.
fn resolve_optional<T: Copy>(library: &Library, prefix: &str, base_name: &str) -> Option<T> {
    let sym_name = prefixed_symbol(prefix, base_name);
    tracing::trace!("resolving optional symbol '{sym_name}'");

    unsafe { library.get::<T>(sym_name.as_bytes()).ok().map(|s| *s) }
}

// ---------------------------------------------------------------------------
// Device discovery
// ---------------------------------------------------------------------------

/// Scan a directory for QDMI device shared libraries.
///
/// This is a convenience function that iterates over all `.so` / `.dylib`
/// files in a directory and attempts to load each one. Libraries that fail
/// to load are silently skipped (with a debug-level log message).
///
/// The caller must supply a mapping from library filename (stem) to prefix.
///
/// Note: Returns an error on IO failure from `read_dir`. Individual library
/// load failures are logged at debug level and skipped (not propagated).
#[allow(clippy::implicit_hasher)]
pub fn scan_directory(
    dir: &Path,
    prefix_map: &std::collections::HashMap<String, String>,
) -> Result<Vec<QdmiDevice>> {
    let mut devices = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(QdmiError::Io)?;

    for entry in entries {
        let entry = entry.map_err(QdmiError::Io)?;
        let path = entry.path();

        let is_shared_lib = path
            .extension()
            .is_some_and(|ext| ext == "so" || ext == "dylib");

        if !is_shared_lib {
            continue;
        }

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Strip conventional "lib" prefix from the stem for lookup.
        let lookup_key = stem.strip_prefix("lib").unwrap_or(&stem).to_string();

        if let Some(prefix) = prefix_map.get(&lookup_key) {
            match QdmiDevice::load(&path, prefix) {
                Ok(device) => {
                    tracing::info!("discovered QDMI device '{prefix}' at {}", path.display());
                    devices.push(device);
                }
                Err(e) => {
                    tracing::debug!("skipping {}: {e}", path.display());
                }
            }
        } else {
            tracing::debug!(
                "no prefix mapping for '{lookup_key}'; skipping {}",
                path.display()
            );
        }
    }

    Ok(devices)
}
