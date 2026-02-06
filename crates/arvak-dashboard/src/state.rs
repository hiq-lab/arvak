//! Application state for the dashboard server.

use std::net::SocketAddr;
use std::sync::Arc;

use hiq_hal::Backend;
use hiq_sched::StateStore;
use rustc_hash::FxHashMap;
use tokio::sync::RwLock;

/// Dashboard configuration.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Address to bind the server to.
    pub bind_address: SocketAddr,
    /// Default backend name.
    pub default_backend: Option<String>,
    /// Maximum qubits for circuit visualization (performance limit).
    pub max_circuit_qubits: usize,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            bind_address: ([127, 0, 0, 1], 3000).into(),
            default_backend: None,
            max_circuit_qubits: 50,
        }
    }
}

/// Shared application state.
pub struct AppState {
    /// Configured backends (name -> Backend instance).
    pub backends: Arc<RwLock<FxHashMap<String, Arc<dyn Backend>>>>,
    /// Dashboard configuration.
    pub config: DashboardConfig,
    /// Job store for persistence (optional).
    pub store: Option<Arc<dyn StateStore>>,
}

impl AppState {
    /// Create a new application state with default configuration.
    pub fn new() -> Self {
        Self {
            backends: Arc::new(RwLock::new(FxHashMap::default())),
            config: DashboardConfig::default(),
            store: None,
        }
    }

    /// Create application state with custom configuration.
    pub fn with_config(config: DashboardConfig) -> Self {
        Self {
            backends: Arc::new(RwLock::new(FxHashMap::default())),
            config,
            store: None,
        }
    }

    /// Set the job store for persistence.
    pub fn with_store(mut self, store: Arc<dyn StateStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Register a backend.
    pub async fn register_backend(&self, backend: Arc<dyn Backend>) {
        let name = backend.name().to_string();
        let mut backends = self.backends.write().await;
        backends.insert(name, backend);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
