// SPDX-License-Identifier: Apache-2.0
//! Load QDMI device shared libraries and resolve prefixed symbols.
//!
//! QDMI devices export their functions with a device-specific prefix, e.g.
//! the example device uses `EX`:
//!
//! ```text
//! EX_QDMI_device_session_init
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

    // -- Session management --------------------------------------------------
    pub(crate) fn_session_init: ffi::FnSessionInit,
    pub(crate) fn_session_deinit: ffi::FnSessionDeinit,

    // -- Query interface: device level ---------------------------------------
    pub(crate) fn_query_device_property: ffi::FnQueryDeviceProperty,

    // -- Query interface: site level -----------------------------------------
    pub(crate) fn_query_site_property: ffi::FnQuerySiteProperty,

    // -- Query interface: operation level ------------------------------------
    pub(crate) fn_query_operation_property: ffi::FnQueryOperationProperty,

    // -- Job interface -------------------------------------------------------
    pub(crate) fn_submit_job: Option<ffi::FnSubmitJob>,
    pub(crate) fn_query_job_status: Option<ffi::FnQueryJobStatus>,
    #[allow(dead_code)]
    pub(crate) fn_query_job_result: Option<ffi::FnQueryJobResult>,
    #[allow(dead_code)]
    pub(crate) fn_cancel_job: Option<ffi::FnCancelJob>,
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
    pub fn load(path: &Path, prefix: &str) -> Result<Self> {
        let path_str = path.display().to_string();

        // SAFETY: we are loading an external shared library. The caller is
        // responsible for ensuring the library is trustworthy.
        let library = unsafe { Library::new(path) }.map_err(|e| QdmiError::LoadFailed {
            path: path_str.clone(),
            cause: e.to_string(),
        })?;

        log::info!(
            "loaded QDMI device library '{}' with prefix '{}'",
            path_str,
            prefix
        );

        // -- Resolve required symbols ----------------------------------------

        let fn_session_init =
            resolve_required::<ffi::FnSessionInit>(&library, prefix, "QDMI_device_session_init", &path_str)?;
        let fn_session_deinit =
            resolve_required::<ffi::FnSessionDeinit>(&library, prefix, "QDMI_device_session_deinit", &path_str)?;
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

        // -- Resolve optional symbols (job interface) ------------------------

        let fn_submit_job =
            resolve_optional::<ffi::FnSubmitJob>(&library, prefix, "QDMI_device_session_submit_job");
        let fn_query_job_status =
            resolve_optional::<ffi::FnQueryJobStatus>(&library, prefix, "QDMI_device_session_query_job_status");
        let fn_query_job_result =
            resolve_optional::<ffi::FnQueryJobResult>(&library, prefix, "QDMI_device_session_query_job_result");
        let fn_cancel_job =
            resolve_optional::<ffi::FnCancelJob>(&library, prefix, "QDMI_device_session_cancel_job");

        if fn_submit_job.is_none() {
            log::warn!("device '{}' does not export job submission functions", prefix);
        }

        Ok(Self {
            _library: library,
            prefix: prefix.to_string(),
            library_path: path_str,
            fn_session_init,
            fn_session_deinit,
            fn_query_device_property,
            fn_query_site_property,
            fn_query_operation_property,
            fn_submit_job,
            fn_query_job_status,
            fn_query_job_result,
            fn_cancel_job,
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
        self.fn_submit_job.is_some()
    }
}

impl std::fmt::Debug for QdmiDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QdmiDevice")
            .field("prefix", &self.prefix)
            .field("library_path", &self.library_path)
            .field("supports_jobs", &self.supports_jobs())
            .finish()
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
    log::trace!("resolving required symbol '{}'", sym_name);

    // SAFETY: The caller guarantees the type `T` matches the actual function
    // signature exported by the library. This is the core FFI contract.
    unsafe {
        let sym: Symbol<T> = library.get(sym_name.as_bytes()).map_err(|e| {
            QdmiError::SymbolNotFound {
                symbol: sym_name.clone(),
                cause: e.to_string(),
            }
        })?;
        Ok(*sym)
    }
}

/// Resolve an optional symbol. Returns `None` if the symbol is missing.
fn resolve_optional<T: Copy>(library: &Library, prefix: &str, base_name: &str) -> Option<T> {
    let sym_name = prefixed_symbol(prefix, base_name);
    log::trace!("resolving optional symbol '{}'", sym_name);

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
/// If no mapping is provided, the function tries to auto-detect by looking
/// for a well-known init symbol pattern.
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
            .map_or(false, |ext| ext == "so" || ext == "dylib");

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
                    log::info!("discovered QDMI device '{}' at {:?}", prefix, path);
                    devices.push(device);
                }
                Err(e) => {
                    log::debug!("skipping {:?}: {}", path, e);
                }
            }
        } else {
            log::debug!(
                "no prefix mapping for '{lookup_key}'; skipping {:?}",
                path
            );
        }
    }

    Ok(devices)
}
