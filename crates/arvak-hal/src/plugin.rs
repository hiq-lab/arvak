//! Dynamic backend plugin system.
//!
//! Allows loading quantum backend implementations from shared libraries
//! (`.so` on Linux, `.dylib` on macOS) at runtime.
//!
//! # Plugin Interface
//!
//! Plugins must export a C-compatible constructor function:
//!
//! ```c
//! BackendPlugin* arvak_plugin_create(const char* config_json);
//! ```
//!
//! In Rust, this is exposed via the [`BackendPlugin`] trait and the
//! `arvak_export_plugin!` macro.
//!
//! # Feature Gate
//!
//! This module requires `--features dynamic-backends`.

use crate::backend::{Backend, BackendConfig};
#[cfg(feature = "dynamic-backends")]
use crate::error::HalError;
use crate::error::HalResult;

/// Trait that dynamic backend plugins must implement.
///
/// Plugins provide a factory method to create backend instances
/// from a configuration.
pub trait BackendPlugin: Send + Sync {
    /// Unique name identifying this plugin.
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Plugin version string.
    fn version(&self) -> &str;

    /// Create a backend instance from the given configuration.
    fn create_backend(&self, config: BackendConfig) -> HalResult<Box<dyn Backend>>;
}

/// Metadata for a loaded plugin.
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin name.
    pub name: String,
    /// Plugin description.
    pub description: String,
    /// Plugin version.
    pub version: String,
    /// Path to the shared library.
    pub library_path: String,
}

/// Type signature for the plugin constructor function exported by shared libraries.
///
/// Plugin shared libraries must export a function with this signature
/// named `arvak_plugin_create`. The returned pointer is a Rust trait object,
/// so this is Rust-to-Rust FFI only (not C-compatible).
///
// SAFETY: Passing Rust trait objects (fat pointers) across FFI boundaries is
// undefined behaviour unless the host and plugin are compiled with the **same**
// Rust compiler version, the **same** global allocator, and the **same**
// optimisation / codegen settings.  `#[allow(improper_ctypes_definitions)]`
// below is intentional but fragile -- it silences the lint that normally
// guards against exactly this kind of ABI mismatch.  Any change to the
// compiler toolchain on either side can silently break the vtable layout.
#[cfg(feature = "dynamic-backends")]
#[allow(improper_ctypes_definitions)]
pub type PluginCreateFn = unsafe extern "C" fn() -> *mut dyn BackendPlugin;

/// A loaded plugin backed by a shared library.
#[cfg(feature = "dynamic-backends")]
pub struct LoadedPlugin {
    plugin: Box<dyn BackendPlugin>,
    _library: libloading::Library,
    path: String,
}

#[cfg(feature = "dynamic-backends")]
impl LoadedPlugin {
    /// Load a plugin from a shared library path.
    ///
    /// # Safety
    ///
    /// The shared library must export `arvak_plugin_create` with the correct
    /// signature. Loading untrusted libraries is inherently unsafe.
    pub unsafe fn load(path: impl AsRef<std::path::Path>) -> HalResult<Self> {
        let path_str = path.as_ref().display().to_string();

        let library = unsafe {
            libloading::Library::new(path.as_ref()).map_err(|e| {
                HalError::Backend(format!("Failed to load plugin '{}': {}", path_str, e))
            })?
        };

        let create_fn: libloading::Symbol<PluginCreateFn> = unsafe {
            library.get(b"arvak_plugin_create").map_err(|e| {
                HalError::Backend(format!(
                    "Plugin '{}' missing arvak_plugin_create: {}",
                    path_str, e
                ))
            })?
        };

        let raw_plugin = unsafe { create_fn() };
        if raw_plugin.is_null() {
            return Err(HalError::Backend(format!(
                "Plugin '{}' returned null from constructor",
                path_str
            )));
        }

        let plugin = unsafe { Box::from_raw(raw_plugin) };

        Ok(Self {
            plugin,
            _library: library,
            path: path_str,
        })
    }

    /// Get plugin metadata.
    pub fn info(&self) -> PluginInfo {
        PluginInfo {
            name: self.plugin.name().to_string(),
            description: self.plugin.description().to_string(),
            version: self.plugin.version().to_string(),
            library_path: self.path.clone(),
        }
    }

    /// Create a backend from this plugin.
    pub fn create_backend(&self, config: BackendConfig) -> HalResult<Box<dyn Backend>> {
        self.plugin.create_backend(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info() {
        let info = PluginInfo {
            name: "test-plugin".into(),
            description: "A test plugin".into(),
            version: "0.1.0".into(),
            library_path: "/usr/lib/libtest.so".into(),
        };

        assert_eq!(info.name, "test-plugin");
        assert_eq!(info.version, "0.1.0");
    }
}
