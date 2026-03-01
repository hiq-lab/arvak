//! IQM backend implementation.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, instrument};

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;

use crate::api::{BackendInfo, IqmClient, SubmitRequest};
use crate::error::{IqmError, IqmResult};

/// Default IQM Resonance API endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://cocos.resonance.meetiqm.com/api/v1";

/// Default target backend (Garnet - IQM's 20-qubit device).
pub const DEFAULT_BACKEND: &str = "garnet";

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// How long to cache backend info before refreshing from the API.
const BACKEND_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// Job cache entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// IQM quantum computer backend.
///
/// This backend connects to IQM quantum computers via the Resonance cloud API.
/// It supports IQM's native gate set (PRX, CZ) and star topology.
pub struct IqmBackend {
    /// Backend configuration.
    config: BackendConfig,
    /// API client.
    client: IqmClient,
    /// Target quantum computer name.
    target: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Cached job information.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached backend info with fetch timestamp for TTL-based refresh.
    backend_info: Arc<Mutex<Option<(BackendInfo, Instant)>>>,
}

impl IqmBackend {
    /// Create a new IQM backend with default settings.
    ///
    /// Reads the API token from the `IQM_TOKEN` environment variable.
    pub fn new() -> IqmResult<Self> {
        let token = std::env::var("IQM_TOKEN").map_err(|_| IqmError::MissingToken)?;

        let config = BackendConfig::new("iqm")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token(&token);

        Self::from_config_impl(config)
    }

    /// Create a backend targeting a specific IQM device.
    pub fn with_target(target: impl Into<String>) -> IqmResult<Self> {
        let token = std::env::var("IQM_TOKEN").map_err(|_| IqmError::MissingToken)?;

        let mut config = BackendConfig::new("iqm")
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token(&token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target.into()));

        Self::from_config_impl(config)
    }

    /// Create a backend with explicit endpoint and token.
    pub fn with_credentials(
        endpoint: impl Into<String>,
        token: impl Into<String>,
        target: impl Into<String>,
    ) -> IqmResult<Self> {
        let mut config = BackendConfig::new("iqm")
            .with_endpoint(endpoint)
            .with_token(token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target.into()));

        Self::from_config_impl(config)
    }

    fn from_config_impl(config: BackendConfig) -> IqmResult<Self> {
        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);

        let token = config.token.as_ref().ok_or(IqmError::MissingToken)?;

        let target = config
            .extra
            .get("target")
            .and_then(|v| v.as_str())
            .map_or_else(
                || DEFAULT_BACKEND.to_string(),
                std::string::ToString::to_string,
            );

        let client = IqmClient::new(endpoint, token)?;

        let capabilities = Capabilities::iqm(&target, 20);

        Ok(Self {
            config,
            client,
            target,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            backend_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the target backend name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Fetch and cache backend information, refreshing if stale.
    async fn fetch_backend_info(&self) -> IqmResult<BackendInfo> {
        // Check cache first; refresh if older than TTL.
        {
            let cache = self.backend_info.lock().await;
            if let Some((ref info, fetched_at)) = *cache {
                if fetched_at.elapsed() < BACKEND_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        // Fetch from API
        let info = self.client.get_backend_info(&self.target).await?;

        // Cache it with current timestamp
        {
            let mut cache = self.backend_info.lock().await;
            *cache = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Convert circuit to QASM3 string.
    fn circuit_to_qasm(&self, circuit: &Circuit) -> IqmResult<String> {
        arvak_qasm3::emit(circuit).map_err(|e| IqmError::QasmError(e.to_string()))
    }

    /// Convert IQM measurement results to Counts.
    fn measurements_to_counts(&self, measurements: &[crate::api::MeasurementResult]) -> Counts {
        let mut counts = Counts::new();

        // Find the classical register (usually "c" or "meas")
        if let Some(result) = measurements.first() {
            for shot in &result.values {
                // Convert bit array to bitstring (MSB first)
                let bitstring: String = shot
                    .iter()
                    .map(|&b| if b != 0 { '1' } else { '0' })
                    .collect();
                // Counts::insert accumulates: repeated bitstrings correctly increment the count.
                counts.insert(bitstring, 1);
            }
        }

        counts
    }

    /// Convert pre-aggregated counts from API response.
    fn api_counts_to_counts(&self, api_counts: &std::collections::HashMap<String, u64>) -> Counts {
        let mut counts = Counts::new();
        for (bitstring, count) in api_counts {
            counts.insert(bitstring.clone(), *count);
        }
        counts
    }
}

#[async_trait]
impl Backend for IqmBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.fetch_backend_info().await {
            Ok(info) => {
                if info.is_online() {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: None,
                    })
                } else {
                    Ok(BackendAvailability::unavailable("backend offline"))
                }
            }
            Err(e) => {
                debug!("Backend availability check failed: {}", e);
                Ok(BackendAvailability::unavailable(e.to_string()))
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but {} only supports {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            ));
        }

        // Check gate set support
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if !gate_set.contains(name) {
                    reasons.push(format!("Unsupported gate: {}", name));
                    break;
                }
            }
        }

        if reasons.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid { reasons })
        }
    }

    #[instrument(skip(self, circuit))]
    async fn submit(
        &self,
        circuit: &Circuit,
        shots: u32,
        _parameters: Option<&std::collections::HashMap<String, f64>>,
    ) -> HalResult<JobId> {
        info!(
            "Submitting circuit to IQM {}: {} qubits, {} shots",
            self.target,
            circuit.num_qubits(),
            shots
        );

        // Validate circuit size
        let caps = self.capabilities();
        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but {} only supports {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            )));
        }

        // Validate shots
        if shots > caps.max_shots {
            return Err(HalError::InvalidShots(format!(
                "Requested {} shots but maximum is {}",
                shots, caps.max_shots
            )));
        }

        // Convert circuit to QASM3
        let qasm = self
            .circuit_to_qasm(circuit)
            .map_err(|e| HalError::Backend(e.to_string()))?;
        debug!("Generated QASM:\n{}", qasm);

        // Create submit request
        let request = SubmitRequest::new(&self.target, qasm, shots);

        // Submit to API
        let response = self
            .client
            .submit_job(&request)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.id);
        info!("Job submitted: {}", job_id);

        // Cache job info, evicting completed entries if the cache is full.
        // If no terminal entries exist, evict an arbitrary active entry to
        // prevent unbounded memory growth.
        let job = Job::new(job_id.clone(), shots).with_backend(&self.target);
        {
            let mut jobs = self.jobs.lock().await;
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.job.status.is_terminal());
                // If retain did not free any space, evict any entry to bound memory.
                if jobs.len() >= MAX_CACHED_JOBS {
                    tracing::warn!(
                        capacity = MAX_CACHED_JOBS,
                        "IQM job cache at capacity with no terminal entries; \
                         evicting an active entry to prevent unbounded growth"
                    );
                    if let Some(key) = jobs.keys().next().cloned() {
                        jobs.remove(&key);
                    }
                }
            }
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
                IqmError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        let status = if response.is_completed() {
            JobStatus::Completed
        } else if response.is_failed() {
            JobStatus::Failed(response.message.unwrap_or_default())
        } else if response.is_cancelled() {
            JobStatus::Cancelled
        } else if response.status.to_lowercase() == "running" {
            JobStatus::Running
        } else {
            JobStatus::Queued
        };

        // Update cache
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(status.clone());
            }
        }

        Ok(status)
    }

    #[instrument(skip(self))]
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        // Check cache first
        {
            let jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get(&job_id.0) {
                if let Some(ref result) = cached.result {
                    return Ok(result.clone());
                }
            }
        }

        // Fetch from API
        let response = self
            .client
            .get_job_result(&job_id.0)
            .await
            .map_err(|e| match e {
                IqmError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        // Check for errors
        if let Some(error) = response.error {
            return Err(HalError::JobFailed(error));
        }

        // Convert results
        let counts = if let Some(ref api_counts) = response.counts {
            self.api_counts_to_counts(api_counts)
        } else if let Some(ref measurements) = response.measurements {
            self.measurements_to_counts(measurements)
        } else {
            return Err(HalError::JobFailed("No measurement results".into()));
        };

        let shots = response
            .metadata
            .as_ref()
            .and_then(|m| m.shots)
            .unwrap_or(counts.total_shots() as u32);

        let mut result = ExecutionResult::new(counts, shots);

        // Add execution time if available
        if let Some(ref metadata) = response.metadata {
            if let Some(time_ms) = metadata.execution_time_ms {
                result = result.with_execution_time(time_ms);
            }

            // Add metadata
            result = result.with_metadata(serde_json::json!({
                "backend": metadata.backend,
                "queue_time_s": metadata.queue_time_s,
                "calibration_timestamp": metadata.calibration_timestamp,
            }));
        }

        // Cache result
        {
            let mut jobs = self.jobs.lock().await;
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
                IqmError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        // Update cache
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(JobStatus::Cancelled);
            }
        }

        info!("Job cancelled: {}", job_id);
        Ok(())
    }
}

impl BackendFactory for IqmBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        Self::from_config_impl(config).map_err(|e| HalError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config() {
        let config = BackendConfig::new("iqm")
            .with_endpoint("https://test.api.com")
            .with_token("test-token")
            .with_extra("target", serde_json::json!("garnet"));

        assert_eq!(config.name, "iqm");
        assert_eq!(config.endpoint, Some("https://test.api.com".to_string()));
        assert!(config.extra.contains_key("target"));
    }

    #[test]
    fn test_measurements_to_counts() {
        // TODO: Implement with mock IqmBackend
        // This would require IqmBackend instance, so we'll skip for now
        // Real tests would mock the API client
    }
}
