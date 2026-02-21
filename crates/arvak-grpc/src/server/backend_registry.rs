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
        registry.register("simulator".to_string(), Arc::new(SimulatorBackend::new()));
    }

    // Note: Braket registration is async (BraketBackend::connect) — call
    // `register_braket_backends()` from an async context for Braket support.

    registry
}

/// Register AWS Braket backends (async due to credential resolution).
///
/// Attempts to connect to the SV1 simulator. Logs a warning and skips
/// if credentials or S3 bucket are not configured.
#[cfg(feature = "braket")]
pub async fn register_braket_backends(registry: &mut BackendRegistry) {
    use arvak_adapter_braket::BraketBackend;

    match BraketBackend::connect(arvak_adapter_braket::device::SV1).await {
        Ok(backend) => {
            registry.register("braket-sv1".to_string(), Arc::new(backend));
            tracing::info!("Registered Braket SV1 simulator");
        }
        Err(e) => {
            tracing::warn!("Braket SV1 not registered: {e}");
        }
    }
}

/// Register Quantinuum backends.
///
/// Registers the H2-1LE noiseless emulator as `"quantinuum-h2le"`.
/// Reads `QUANTINUUM_EMAIL` and `QUANTINUUM_PASSWORD` from the environment.
/// Logs a warning and skips if credentials are absent.
#[cfg(feature = "quantinuum")]
pub async fn register_quantinuum_backends(registry: &mut BackendRegistry) {
    use arvak_adapter_quantinuum::QuantinuumBackend;

    match QuantinuumBackend::new() {
        Ok(backend) => {
            registry.register("quantinuum-h2le".to_string(), Arc::new(backend));
            tracing::info!("Registered Quantinuum H2-1LE as 'quantinuum-h2le'");
        }
        Err(e) => {
            tracing::warn!("Quantinuum backend not registered: {e}");
        }
    }
}

/// Register AQT backends.
///
/// Registers the offline noiseless simulator as `"aqt-offline-sim"`.
/// Reads `AQT_TOKEN` from the environment (may be empty for offline simulators).
/// Reads `AQT_PORTAL_URL` to override the default base URL.
#[cfg(feature = "aqt")]
pub async fn register_aqt_backends(registry: &mut BackendRegistry) {
    use arvak_adapter_aqt::AqtBackend;

    match AqtBackend::new() {
        Ok(backend) => {
            registry.register("aqt-offline-sim".to_string(), Arc::new(backend));
            tracing::info!("Registered AQT offline_simulator_no_noise as 'aqt-offline-sim'");
        }
        Err(e) => {
            tracing::warn!("AQT backend not registered: {e}");
        }
    }
}

/// Register IBM Quantum backends (async — requires IAM token exchange).
///
/// Registers ibm_torino as "ibm-torino". Reads `IBM_API_KEY` and `IBM_SERVICE_CRN`
/// from the environment. Logs a warning and skips if credentials are absent.
#[cfg(feature = "ibm")]
pub async fn register_ibm_backends(registry: &mut BackendRegistry) {
    use arvak_adapter_ibm::IbmBackend;

    match IbmBackend::connect("ibm_torino").await {
        Ok(backend) => {
            registry.register("ibm-torino".to_string(), Arc::new(backend));
            tracing::info!("Registered IBM Quantum ibm_torino as 'ibm-torino'");
        }
        Err(e) => {
            tracing::warn!("IBM Quantum backend not registered: {e}");
        }
    }
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
