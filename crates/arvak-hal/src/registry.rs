//! Backend registry for managing available backends.
//!
//! The [`BackendRegistry`] provides a central point for discovering and
//! creating backend instances, including dynamically loaded plugins.

use rustc_hash::FxHashMap;
use tracing::debug;
#[cfg(feature = "dynamic-backends")]
use tracing::{info, warn};

use crate::backend::{Backend, BackendConfig, BackendFactory};
use crate::error::{HalError, HalResult};
use crate::plugin::PluginInfo;

/// Factory function type for built-in backends.
type BuiltinFactory = Box<dyn Fn(BackendConfig) -> HalResult<Box<dyn Backend>> + Send + Sync>;

/// Central registry for quantum backends.
///
/// Manages both built-in and dynamically loaded backends,
/// providing a unified interface for backend discovery and creation.
pub struct BackendRegistry {
    /// Built-in backend factories keyed by name.
    builtins: FxHashMap<String, BuiltinFactory>,
    /// Dynamically loaded plugins.
    #[cfg(feature = "dynamic-backends")]
    plugins: Vec<crate::plugin::LoadedPlugin>,
    /// Plugin metadata (available without dynamic-backends feature).
    plugin_infos: Vec<PluginInfo>,
}

impl BackendRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            builtins: FxHashMap::default(),
            #[cfg(feature = "dynamic-backends")]
            plugins: Vec::new(),
            plugin_infos: Vec::new(),
        }
    }

    /// Register a built-in backend factory.
    pub fn register<B>(&mut self, name: impl Into<String>)
    where
        B: BackendFactory + Backend + 'static,
    {
        let name = name.into();
        debug!("Registering built-in backend: {}", name);
        self.builtins.insert(
            name,
            Box::new(|config| {
                let backend = B::from_config(config)?;
                Ok(Box::new(backend))
            }),
        );
    }

    /// Register a backend factory with a custom constructor.
    pub fn register_factory(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn(BackendConfig) -> HalResult<Box<dyn Backend>> + Send + Sync + 'static,
    ) {
        let name = name.into();
        debug!("Registering factory backend: {}", name);
        self.builtins.insert(name, Box::new(factory));
    }

    /// Load plugins from the default plugin directory.
    ///
    /// Searches `$ARVAK_PLUGIN_DIR` or `~/.arvak/plugins/` for shared libraries.
    #[cfg(feature = "dynamic-backends")]
    pub fn load_plugins(&mut self) -> HalResult<usize> {
        let plugin_dir = std::env::var("ARVAK_PLUGIN_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".arvak")
                    .join("plugins")
            });

        self.load_plugins_from(&plugin_dir)
    }

    /// Load plugins from a specific directory.
    #[cfg(feature = "dynamic-backends")]
    pub fn load_plugins_from(&mut self, dir: &std::path::Path) -> HalResult<usize> {
        if !dir.exists() {
            debug!("Plugin directory does not exist: {}", dir.display());
            return Ok(0);
        }

        let mut count = 0;

        let entries = std::fs::read_dir(dir).map_err(|e| {
            HalError::Backend(format!(
                "Failed to read plugin directory '{}': {}",
                dir.display(),
                e
            ))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            let extension = path.extension().and_then(|e| e.to_str());

            let is_plugin = matches!(extension, Some("so") | Some("dylib") | Some("dll"));
            if !is_plugin {
                continue;
            }

            match unsafe { crate::plugin::LoadedPlugin::load(&path) } {
                Ok(loaded) => {
                    let info = loaded.info();
                    info!("Loaded plugin: {} v{} from {}", info.name, info.version, info.library_path);
                    self.plugin_infos.push(info);
                    self.plugins.push(loaded);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to load plugin '{}': {}", path.display(), e);
                }
            }
        }

        Ok(count)
    }

    /// Create a backend by name.
    pub fn create(&self, name: &str, config: BackendConfig) -> HalResult<Box<dyn Backend>> {
        // Try built-in first
        if let Some(factory) = self.builtins.get(name) {
            return factory(config);
        }

        // Try plugins
        #[cfg(feature = "dynamic-backends")]
        for plugin in &self.plugins {
            let info = plugin.info();
            if info.name == name {
                return plugin.create_backend(config);
            }
        }

        Err(HalError::BackendUnavailable(format!(
            "No backend registered with name '{}'",
            name
        )))
    }

    /// List all available backend names.
    pub fn available_backends(&self) -> Vec<String> {
        let mut names: Vec<_> = self.builtins.keys().cloned().collect();
        for info in &self.plugin_infos {
            names.push(info.name.clone());
        }
        names.sort();
        names
    }

    /// Get metadata for all loaded plugins.
    pub fn plugin_infos(&self) -> &[PluginInfo] {
        &self.plugin_infos
    }

    /// Check if a backend is available by name.
    pub fn has_backend(&self, name: &str) -> bool {
        self.builtins.contains_key(name)
            || self.plugin_infos.iter().any(|info| info.name == name)
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = BackendRegistry::new();
        assert!(registry.available_backends().is_empty());
        assert!(!registry.has_backend("simulator"));
    }

    #[test]
    fn test_register_factory() {
        let mut registry = BackendRegistry::new();
        registry.register_factory("test", |_config| {
            Err(HalError::BackendUnavailable("test only".into()))
        });

        assert!(registry.has_backend("test"));
        assert_eq!(registry.available_backends(), vec!["test"]);
    }

    #[test]
    fn test_create_unknown_backend() {
        let registry = BackendRegistry::new();
        let result = registry.create("nonexistent", BackendConfig::new("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_available_backends_sorted() {
        let mut registry = BackendRegistry::new();
        registry.register_factory("zebra", |_| {
            Err(HalError::BackendUnavailable("test".into()))
        });
        registry.register_factory("alpha", |_| {
            Err(HalError::BackendUnavailable("test".into()))
        });

        let backends = registry.available_backends();
        assert_eq!(backends, vec!["alpha", "zebra"]);
    }
}
