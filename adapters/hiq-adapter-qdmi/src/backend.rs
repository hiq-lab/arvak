//! QDMI Backend implementation.
//!
//! This module provides a HIQ Backend implementation that communicates
//! with quantum devices via the QDMI (Quantum Device Management Interface).

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

use hiq_hal::backend::{Backend, BackendConfig, BackendFactory};
use hiq_hal::capability::{Capabilities, GateSet, Topology};
use hiq_hal::error::{HalError, HalResult};
use hiq_hal::job::{JobId, JobStatus};
use hiq_hal::result::{Counts, ExecutionResult};
use hiq_ir::Circuit;

use crate::error::{QdmiError, QdmiResult};
use crate::ffi::{QdmiDeviceStatus, QdmiJobStatus};

#[cfg(not(feature = "system-qdmi"))]
use crate::ffi::mock::{MockDevice, MockJob, MockSession};

/// QDMI Backend for HIQ.
///
/// This backend connects to quantum devices via the QDMI interface,
/// which is part of the Munich Quantum Software Stack (MQSS).
///
/// # Example
///
/// ```ignore
/// use hiq_adapter_qdmi::QdmiBackend;
/// use hiq_hal::Backend;
///
/// let backend = QdmiBackend::new()
///     .with_token("your-api-token")
///     .with_base_url("https://qdmi.lrz.de");
///
/// // Get device capabilities
/// let caps = backend.capabilities().await?;
/// println!("Device: {} with {} qubits", caps.name, caps.num_qubits);
///
/// // Submit a circuit
/// let circuit = Circuit::bell()?;
/// let job_id = backend.submit(&circuit, 1000).await?;
/// let result = backend.wait(&job_id).await?;
/// ```
pub struct QdmiBackend {
    /// Configuration
    config: BackendConfig,

    /// Internal state
    #[cfg(not(feature = "system-qdmi"))]
    state: Arc<RwLock<MockState>>,

    /// Cached capabilities
    capabilities_cache: Arc<RwLock<Option<Capabilities>>>,
}

/// Mock state for testing without system QDMI
#[cfg(not(feature = "system-qdmi"))]
struct MockState {
    session: Option<MockSession>,
    device: Option<MockDevice>,
    jobs: FxHashMap<String, MockJob>,
}

#[cfg(not(feature = "system-qdmi"))]
impl Default for MockState {
    fn default() -> Self {
        Self {
            session: None,
            device: None,
            jobs: FxHashMap::default(),
        }
    }
}

impl QdmiBackend {
    /// Create a new QDMI backend.
    pub fn new() -> Self {
        Self {
            config: BackendConfig::new("qdmi"),
            #[cfg(not(feature = "system-qdmi"))]
            state: Arc::new(RwLock::new(MockState::default())),
            capabilities_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the authentication token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.config.token = Some(token.into());
        self
    }

    /// Set the base URL for the QDMI endpoint.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.endpoint = Some(url.into());
        self
    }

    /// Initialize the QDMI session.
    #[cfg(not(feature = "system-qdmi"))]
    pub fn initialize(&self) -> QdmiResult<()> {
        let mut state = self
            .state
            .write()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        // Create mock session
        let mut session = MockSession::new();
        if let Some(ref token) = self.config.token {
            session.token = Some(token.clone());
        }
        if let Some(ref endpoint) = self.config.endpoint {
            session.base_url = Some(endpoint.clone());
        }
        session.initialized = true;

        // Create mock device
        let device = MockDevice::new("QDMI Mock Device", 20);

        state.session = Some(session);
        state.device = Some(device);

        info!("QDMI session initialized (mock mode)");
        Ok(())
    }

    /// Initialize the QDMI session (system QDMI).
    #[cfg(feature = "system-qdmi")]
    pub fn initialize(&self) -> QdmiResult<()> {
        // TODO: Implement real QDMI session initialization
        // This would use the FFI functions to:
        // 1. QDMI_session_alloc
        // 2. QDMI_session_set_parameter (token, base_url, etc.)
        // 3. QDMI_session_init
        // 4. QDMI_session_get_devices
        unimplemented!("System QDMI integration requires linking against libqdmi")
    }

    /// Convert a HIQ circuit to QASM3 for QDMI submission.
    fn circuit_to_qasm3(&self, circuit: &Circuit) -> QdmiResult<String> {
        hiq_qasm3::emit(circuit).map_err(|e| QdmiError::CircuitConversion(e.to_string()))
    }

    /// Parse QDMI results into HIQ Counts.
    #[allow(dead_code)]
    fn parse_results(&self, hist_keys: &[String], hist_values: &[u64]) -> Counts {
        let mut counts = Counts::new();
        for (key, &value) in hist_keys.iter().zip(hist_values.iter()) {
            counts.insert(key.clone(), value);
        }
        counts
    }

    /// Convert QDMI job status to HIQ job status.
    fn convert_job_status(&self, qdmi_status: QdmiJobStatus) -> JobStatus {
        match qdmi_status {
            QdmiJobStatus::Created | QdmiJobStatus::Submitted | QdmiJobStatus::Queued => {
                JobStatus::Queued
            }
            QdmiJobStatus::Running => JobStatus::Running,
            QdmiJobStatus::Done => JobStatus::Completed,
            QdmiJobStatus::Canceled => JobStatus::Cancelled,
            QdmiJobStatus::Failed => JobStatus::Failed("Job failed".into()),
        }
    }

    /// Build capabilities from QDMI device properties.
    #[cfg(not(feature = "system-qdmi"))]
    fn build_capabilities(&self) -> QdmiResult<Capabilities> {
        let state = self
            .state
            .read()
            .map_err(|_| QdmiError::Ffi("Failed to acquire lock".into()))?;

        let device = state.device.as_ref().ok_or(QdmiError::NoDevice)?;

        Ok(Capabilities {
            name: device.name.clone(),
            num_qubits: device.num_qubits as u32,
            gate_set: GateSet::universal(), // QDMI devices typically accept standard gates
            topology: Topology::full(device.num_qubits as u32), // Simplified
            max_shots: 100_000,
            is_simulator: false,
            features: vec!["qdmi".into(), "mqss".into()],
        })
    }
}

impl Default for QdmiBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for QdmiBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn capabilities(&self) -> HalResult<Capabilities> {
        // Check cache first
        {
            let cache = self
                .capabilities_cache
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;
            if let Some(ref caps) = *cache {
                return Ok(caps.clone());
            }
        }

        // Initialize if needed
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;
            if state.session.is_none() {
                drop(state);
                self.initialize()
                    .map_err(|e| HalError::Backend(e.to_string()))?;
            }
        }

        // Build capabilities
        let caps = self
            .build_capabilities()
            .map_err(|e| HalError::Backend(e.to_string()))?;

        // Cache the result
        {
            let mut cache = self
                .capabilities_cache
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;
            *cache = Some(caps.clone());
        }

        Ok(caps)
    }

    async fn is_available(&self) -> HalResult<bool> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            if let Some(ref device) = state.device {
                return Ok(matches!(
                    device.status,
                    QdmiDeviceStatus::Idle | QdmiDeviceStatus::Busy
                ));
            }

            // Try to initialize
            drop(state);
            if self.initialize().is_ok() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        debug!("Submitting circuit with {} shots via QDMI", shots);

        // Initialize if needed
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;
            if state.session.is_none() {
                drop(state);
                self.initialize()
                    .map_err(|e| HalError::Backend(e.to_string()))?;
            }
        }

        // Convert circuit to QASM3
        let qasm = self
            .circuit_to_qasm3(circuit)
            .map_err(|e| HalError::Backend(e.to_string()))?;

        debug!("Circuit converted to QASM3 ({} bytes)", qasm.len());

        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            // Create mock job
            let mut job = MockJob::new();
            job.program = Some(qasm);
            job.shots = shots as usize;
            job.status = QdmiJobStatus::Submitted;

            let job_id = job.id.clone();
            state.jobs.insert(job_id.clone(), job);

            info!("Submitted job {} via QDMI (mock)", job_id);
            return Ok(JobId::new(job_id));
        }

        #[cfg(feature = "system-qdmi")]
        {
            // TODO: Real QDMI job submission
            unimplemented!("System QDMI integration requires linking against libqdmi")
        }
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get_mut(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            // Simulate job progression
            match job.status {
                QdmiJobStatus::Submitted => {
                    job.status = QdmiJobStatus::Queued;
                }
                QdmiJobStatus::Queued => {
                    job.status = QdmiJobStatus::Running;
                }
                QdmiJobStatus::Running => {
                    // Generate mock results
                    let num_qubits = 2; // Simplified
                    let mut results = Vec::new();
                    for i in 0..job.shots {
                        // Simple mock: 50% |00⟩, 50% |11⟩
                        if i % 2 == 0 {
                            results.push("0".repeat(num_qubits));
                        } else {
                            results.push("1".repeat(num_qubits));
                        }
                    }
                    job.results = Some(results);
                    job.status = QdmiJobStatus::Done;
                }
                _ => {}
            }

            return Ok(self.convert_job_status(job.status));
        }

        #[cfg(feature = "system-qdmi")]
        {
            unimplemented!("System QDMI integration requires linking against libqdmi")
        }
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let state = self
                .state
                .read()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            if !matches!(job.status, QdmiJobStatus::Done) {
                return Err(HalError::JobFailed("Job not completed".into()));
            }

            // Convert results to counts
            let mut counts = Counts::new();
            if let Some(ref results) = job.results {
                for result in results {
                    counts.insert(result.as_str(), 1);
                }
            }

            return Ok(ExecutionResult::new(counts, job.shots as u32));
        }

        #[cfg(feature = "system-qdmi")]
        {
            unimplemented!("System QDMI integration requires linking against libqdmi")
        }
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        #[cfg(not(feature = "system-qdmi"))]
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| HalError::Backend("Failed to acquire lock".into()))?;

            let job = state
                .jobs
                .get_mut(&job_id.0)
                .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))?;

            job.status = QdmiJobStatus::Canceled;
            info!("Cancelled job {} via QDMI", job_id);
            return Ok(());
        }

        #[cfg(feature = "system-qdmi")]
        {
            unimplemented!("System QDMI integration requires linking against libqdmi")
        }
    }
}

impl BackendFactory for QdmiBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let mut backend = QdmiBackend::new();
        backend.config = config;

        // Auto-initialize if we have credentials
        if backend.config.token.is_some() || backend.config.endpoint.is_some() {
            backend
                .initialize()
                .map_err(|e| HalError::Backend(e.to_string()))?;
        }

        Ok(backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_qdmi_backend_creation() {
        let backend = QdmiBackend::new();
        assert_eq!(backend.name(), "qdmi");
    }

    #[tokio::test]
    async fn test_qdmi_backend_capabilities() {
        let backend = QdmiBackend::new();
        let caps = backend.capabilities().await.unwrap();

        assert!(caps.num_qubits > 0);
        assert!(!caps.is_simulator);
        assert!(caps.features.contains(&"qdmi".to_string()));
    }

    #[tokio::test]
    async fn test_qdmi_backend_availability() {
        let backend = QdmiBackend::new();
        let available = backend.is_available().await.unwrap();
        assert!(available);
    }

    #[tokio::test]
    async fn test_qdmi_backend_submit_and_wait() {
        let backend = QdmiBackend::new();

        // Create a simple Bell state circuit
        let mut circuit = Circuit::with_size("bell", 2, 2);
        circuit.h(hiq_ir::QubitId(0)).unwrap();
        circuit.cx(hiq_ir::QubitId(0), hiq_ir::QubitId(1)).unwrap();
        let _ = circuit.measure_all();

        // Submit
        let job_id = backend.submit(&circuit, 1000).await.unwrap();
        assert!(!job_id.0.is_empty());

        // Wait for completion
        let result = backend.wait(&job_id).await.unwrap();

        assert_eq!(result.shots, 1000);
        assert!(!result.counts.is_empty());
    }

    #[tokio::test]
    async fn test_qdmi_backend_cancel() {
        let backend = QdmiBackend::new();

        let mut circuit = Circuit::with_size("test", 1, 1);
        circuit.h(hiq_ir::QubitId(0)).unwrap();

        let job_id = backend.submit(&circuit, 100).await.unwrap();
        backend.cancel(&job_id).await.unwrap();

        let status = backend.status(&job_id).await.unwrap();
        assert!(matches!(status, JobStatus::Cancelled));
    }
}
