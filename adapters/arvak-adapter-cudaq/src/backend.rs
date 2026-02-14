//! CUDA-Q backend implementation.
//!
//! Provides an Arvak [`Backend`] that submits circuits to NVIDIA CUDA-Q
//! targets (GPU simulators and hardware backends) via REST API, using
//! `OpenQASM` 3.0 as the interchange format.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, instrument};

use arvak_hal::backend::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, ValidationResult,
};
use arvak_hal::capability::{Capabilities, GateSet, Topology};
use arvak_hal::error::{HalError, HalResult};
use arvak_hal::job::{Job, JobId, JobStatus};
use arvak_hal::result::{Counts, ExecutionResult};
use arvak_ir::Circuit;

use crate::api::{CudaqClient, ProgramFormat, SubmitRequest, TargetInfo};
use crate::error::{CudaqError, CudaqResult};

/// Default CUDA-Q cloud API endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://api.quantum.nvidia.com/v1";

/// Default target (multi-GPU statevector simulator).
pub const DEFAULT_TARGET: &str = "nvidia-mqpu";

/// Known CUDA-Q simulator targets.
pub mod targets {
    /// Multi-GPU statevector simulator (up to 40 qubits).
    pub const MQPU: &str = "nvidia-mqpu";
    /// Single-GPU statevector simulator.
    pub const CUSTATEVEC: &str = "custatevec";
    /// Tensor-network simulator (large qubit counts, shallow circuits).
    pub const TENSORNET: &str = "tensornet";
    /// Density-matrix simulator (noise simulation).
    pub const DM: &str = "density-matrix";
}

/// Cached job entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// NVIDIA CUDA-Q quantum backend.
///
/// Connects to CUDA-Q cloud services for GPU-accelerated quantum simulation
/// and hardware execution. Uses `OpenQASM` 3.0 as the circuit interchange format.
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_cudaq::CudaqBackend;
/// use arvak_hal::Backend;
/// use arvak_ir::{Circuit, QubitId};
///
/// let backend = CudaqBackend::new()?;
///
/// let mut circuit = Circuit::with_size("bell", 2, 2);
/// circuit.h(QubitId(0))?;
/// circuit.cx(QubitId(0), QubitId(1))?;
/// circuit.measure_all()?;
///
/// let job_id = backend.submit(&circuit, 1000).await?;
/// let result = backend.wait(&job_id).await?;
/// println!("{:?}", result.counts);
/// ```
pub struct CudaqBackend {
    config: BackendConfig,
    client: CudaqClient,
    target: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    target_info: Arc<Mutex<Option<TargetInfo>>>,
}

impl CudaqBackend {
    /// Create a new CUDA-Q backend with default settings.
    ///
    /// Reads the API token from `CUDAQ_API_TOKEN` environment variable.
    pub fn new() -> CudaqResult<Self> {
        let token = std::env::var("CUDAQ_API_TOKEN").map_err(|_| CudaqError::MissingToken)?;

        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token(&token);

        Self::from_config_impl(config)
    }

    /// Create a backend targeting a specific CUDA-Q target.
    ///
    /// Available targets: `nvidia-mqpu`, `custatevec`, `tensornet`, `density-matrix`.
    pub fn with_target(target: impl Into<String>) -> CudaqResult<Self> {
        let token = std::env::var("CUDAQ_API_TOKEN").map_err(|_| CudaqError::MissingToken)?;

        let mut config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token(&token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target.into()));

        Self::from_config_impl(config)
    }

    /// Create a backend with explicit credentials.
    pub fn with_credentials(
        endpoint: impl Into<String>,
        token: impl Into<String>,
        target: impl Into<String>,
    ) -> CudaqResult<Self> {
        let mut config = BackendConfig::new("cudaq")
            .with_endpoint(endpoint)
            .with_token(token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target.into()));

        Self::from_config_impl(config)
    }

    fn from_config_impl(config: BackendConfig) -> CudaqResult<Self> {
        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);
        let token = config.token.as_ref().ok_or(CudaqError::MissingToken)?;

        let target = config
            .extra
            .get("target")
            .and_then(|v| v.as_str())
            .map_or_else(
                || DEFAULT_TARGET.to_string(),
                std::string::ToString::to_string,
            );

        let client = CudaqClient::new(endpoint, token)?;

        // Build default capabilities at construction (HAL Contract v2).
        let (num_qubits, is_simulator) = match target.as_str() {
            "nvidia-mqpu" => (40, true),
            "custatevec" => (32, true),
            "tensornet" => (100, true),
            "density-matrix" => (20, true),
            _ => (30, true),
        };
        let capabilities = Capabilities {
            name: target.clone(),
            num_qubits,
            gate_set: GateSet::universal(),
            topology: Topology::full(num_qubits),
            max_shots: 1_000_000,
            is_simulator,
            features: vec!["gpu-accelerated".into(), "qasm3".into()],
            noise_profile: None,
        };

        Ok(Self {
            config,
            client,
            target,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            target_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the target name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Fetch and cache target information.
    async fn fetch_target_info(&self) -> CudaqResult<TargetInfo> {
        {
            let cache = self
                .target_info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(info) = cache.as_ref() {
                return Ok(info.clone());
            }
        }

        let info = self.client.get_target(&self.target).await?;

        {
            let mut cache = self
                .target_info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *cache = Some(info.clone());
        }

        Ok(info)
    }

    /// Convert circuit to `OpenQASM` 3.0.
    fn circuit_to_qasm(&self, circuit: &Circuit) -> CudaqResult<String> {
        arvak_qasm3::emit(circuit).map_err(|e| CudaqError::QasmConversion(e.to_string()))
    }
}

#[async_trait]
impl Backend for CudaqBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.fetch_target_info().await {
            Ok(info) => {
                if info.is_online() {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: None,
                    })
                } else {
                    Ok(BackendAvailability::unavailable("target offline"))
                }
            }
            Err(e) => {
                debug!("Availability check failed: {}", e);
                Ok(BackendAvailability::unavailable(e.to_string()))
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but {} supports max {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            ));
        }

        if reasons.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid { reasons })
        }
    }

    #[instrument(skip(self, circuit))]
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        info!(
            "Submitting circuit to CUDA-Q {}: {} qubits, {} shots",
            self.target,
            circuit.num_qubits(),
            shots
        );

        let caps = self.capabilities();
        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but {} supports max {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            )));
        }

        if shots > caps.max_shots {
            return Err(HalError::InvalidShots(format!(
                "Requested {} shots but maximum is {}",
                shots, caps.max_shots
            )));
        }

        let qasm = self
            .circuit_to_qasm(circuit)
            .map_err(|e| HalError::Backend(e.to_string()))?;
        debug!("Generated QASM3:\n{}", qasm);

        let request = SubmitRequest::new(&self.target, qasm, ProgramFormat::Qasm3, shots)
            .with_num_qubits(circuit.num_qubits() as u32);

        let response = self
            .client
            .submit_job(&request)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.id);
        info!("Job submitted: {}", job_id);

        let job = Job::new(job_id.clone(), shots).with_backend(&self.target);
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            jobs.insert(job_id.0.clone(), CachedJob { job, result: None });
        }

        Ok(job_id)
    }

    #[instrument(skip(self))]
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let response = self
            .client
            .get_job_status(&job_id.0)
            .await
            .map_err(|e| match e {
                CudaqError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        let status = if response.is_completed() {
            JobStatus::Completed
        } else if response.is_failed() {
            JobStatus::Failed(response.message.unwrap_or_default())
        } else if response.is_cancelled() {
            JobStatus::Cancelled
        } else if response.status.to_lowercase() == "running"
            || response.status.to_lowercase() == "executing"
        {
            JobStatus::Running
        } else {
            JobStatus::Queued
        };

        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(status.clone());
            }
        }

        Ok(status)
    }

    #[instrument(skip(self))]
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        // Check cache
        {
            let jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = jobs.get(&job_id.0) {
                if let Some(ref result) = cached.result {
                    return Ok(result.clone());
                }
            }
        }

        let response = self
            .client
            .get_job_result(&job_id.0)
            .await
            .map_err(|e| match e {
                CudaqError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        if let Some(error) = response.error {
            return Err(HalError::JobFailed(error));
        }

        let counts = match response.counts {
            Some(ref api_counts) => {
                let mut counts = Counts::new();
                for (bitstring, count) in api_counts {
                    counts.insert(bitstring.clone(), *count);
                }
                counts
            }
            None => return Err(HalError::JobFailed("No measurement results".into())),
        };

        let shots = response
            .metadata
            .as_ref()
            .and_then(|m| m.shots)
            .unwrap_or(counts.total_shots() as u32);

        let mut result = ExecutionResult::new(counts, shots);

        if let Some(ref metadata) = response.metadata {
            if let Some(time_ms) = metadata.execution_time_ms {
                result = result.with_execution_time(time_ms);
            }

            result = result.with_metadata(serde_json::json!({
                "target": metadata.target,
                "num_gpus": metadata.num_gpus,
                "simulator_version": metadata.simulator_version,
            }));
        }

        // Cache result
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.result = Some(result.clone());
                cached.job = cached.job.clone().with_status(JobStatus::Completed);
            }
        }

        Ok(result)
    }

    #[instrument(skip(self))]
    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        self.client
            .cancel_job(&job_id.0)
            .await
            .map_err(|e| match e {
                CudaqError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(JobStatus::Cancelled);
            }
        }

        info!("Job cancelled: {}", job_id);
        Ok(())
    }
}

impl BackendFactory for CudaqBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        Self::from_config_impl(config).map_err(|e| HalError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config() {
        let config = BackendConfig::new("cudaq")
            .with_endpoint("https://test.api.nvidia.com/v1")
            .with_token("test-token")
            .with_extra("target", serde_json::json!("tensornet"));

        assert_eq!(config.name, "cudaq");
        assert_eq!(
            config.endpoint,
            Some("https://test.api.nvidia.com/v1".to_string())
        );
        assert_eq!(
            config.extra.get("target").and_then(|v| v.as_str()),
            Some("tensornet")
        );
    }

    #[test]
    fn test_from_config_impl() {
        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token("test-token")
            .with_extra("target", serde_json::json!("custatevec"));

        let backend = CudaqBackend::from_config_impl(config).unwrap();
        assert_eq!(backend.name(), "cudaq");
        assert_eq!(backend.target(), "custatevec");
    }

    #[test]
    fn test_from_config_default_target() {
        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token("test-token");

        let backend = CudaqBackend::from_config_impl(config).unwrap();
        assert_eq!(backend.target(), DEFAULT_TARGET);
    }

    #[test]
    fn test_capabilities() {
        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token("test-token");

        let backend = CudaqBackend::from_config_impl(config).unwrap();
        let caps = backend.capabilities();

        assert_eq!(caps.name, "nvidia-mqpu");
        assert_eq!(caps.num_qubits, 40);
        assert!(caps.is_simulator);
        assert_eq!(caps.max_shots, 1_000_000);
        assert!(caps.features.contains(&"gpu-accelerated".to_string()));
    }

    #[test]
    fn test_capabilities_tensornet() {
        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token("test-token")
            .with_extra("target", serde_json::json!("tensornet"));

        let backend = CudaqBackend::from_config_impl(config).unwrap();
        let caps = backend.capabilities();

        assert_eq!(caps.name, "tensornet");
        assert_eq!(caps.num_qubits, 100);
    }

    #[test]
    fn test_circuit_to_qasm() {
        use arvak_ir::QubitId;

        let config = BackendConfig::new("cudaq")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token("test-token");

        let backend = CudaqBackend::from_config_impl(config).unwrap();

        let mut circuit = Circuit::with_size("bell", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let _ = circuit.measure_all();

        let qasm = backend.circuit_to_qasm(&circuit).unwrap();
        assert!(qasm.contains("OPENQASM"));
        assert!(qasm.contains("h "));
        assert!(qasm.contains("cx "));
    }
}
