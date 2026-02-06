//! Backend registry for managing available quantum backends.

use arvak_hal::backend::Backend;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::error::{Error, Result};

/// Registry of available backends.
pub struct BackendRegistry {
    backends: FxHashMap<String, Arc<dyn Backend>>,
}

impl BackendRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            backends: FxHashMap::default(),
        }
    }

    /// Register a backend.
    pub fn register(&mut self, id: String, backend: Arc<dyn Backend>) {
        self.backends.insert(id, backend);
    }

    /// Get a backend by ID.
    pub fn get(&self, id: &str) -> Result<Arc<dyn Backend>> {
        self.backends
            .get(id)
            .cloned()
            .ok_or_else(|| Error::BackendNotFound(id.to_string()))
    }

    /// List all backend IDs.
    pub fn list(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }

    /// Check if a backend exists.
    pub fn contains(&self, id: &str) -> bool {
        self.backends.contains_key(id)
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create the default backend registry with feature-gated backends.
pub fn create_default_registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();

    #[cfg(feature = "simulator")]
    {
        use arvak_adapter_sim::SimulatorBackend;
        registry.register(
            "simulator".to_string(),
            Arc::new(SimulatorBackend::new()),
        );
    }

    // Future backends will be added here with feature gates:
    // #[cfg(feature = "iqm")]
    // {
    //     registry.register("iqm".to_string(), Arc::new(IqmBackend::new()));
    // }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basic() {
        let registry = create_default_registry();

        #[cfg(feature = "simulator")]
        {
            assert!(registry.contains("simulator"));
            let backend = registry.get("simulator").unwrap();
            assert_eq!(backend.name(), "simulator");
        }

        let result = registry.get("nonexistent");
        assert!(matches!(result, Err(Error::BackendNotFound(_))));
    }

    #[test]
    fn test_list_backends() {
        let registry = create_default_registry();
        let backends = registry.list();

        #[cfg(feature = "simulator")]
        {
            assert!(backends.contains(&"simulator".to_string()));
        }
    }
}
