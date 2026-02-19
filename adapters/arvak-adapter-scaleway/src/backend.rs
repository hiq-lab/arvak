//! Scaleway QaaS backend implementation.
//!
//! Implements the HAL Contract v2 `Backend` trait for Scaleway's
//! Quantum-as-a-Service platform, which provides access to IQM Garnet
//! and other quantum hardware via a session-based execution model.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, info, instrument, warn};

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;

use crate::api::{
    CreateJobRequest, JobResponse, ScalewayClient, build_computation_model, compress_qasm,
};
use crate::error::{ScalewayError, ScalewayResult};

/// Default Scaleway API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.scaleway.com";

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// How long to wait between job status polls (seconds).
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Maximum time to wait for a job to complete (seconds).
pub(crate) const MAX_WAIT_TIME: Duration = Duration::from_secs(300);

/// Job cache entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// Scaleway QaaS quantum computing backend.
///
/// This backend connects to quantum hardware (IQM Garnet, Pasqal) through
/// Scaleway's Quantum-as-a-Service platform. Jobs execute within sessions
/// that are created via the Scaleway console or API.
///
/// # Authentication
///
/// Set environment variables:
/// ```bash
/// export SCALEWAY_SECRET_KEY="your-secret-key"
/// export SCALEWAY_PROJECT_ID="your-project-id"
/// export SCALEWAY_SESSION_ID="your-session-id"
/// ```
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_scaleway::ScalewayBackend;
/// use arvak_hal::Backend;
///
/// let backend = ScalewayBackend::new()?;
/// let caps = backend.capabilities();
/// println!("Qubits: {}", caps.num_qubits);
/// ```
pub struct ScalewayBackend {
    /// Backend configuration.
    config: BackendConfig,
    /// API client.
    client: ScalewayClient,
    /// Active session ID.
    session_id: String,
    /// Platform identifier (e.g., "QPU-GARNET-20PQ").
    platform: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Cached job information.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
}

impl ScalewayBackend {
    /// Create a new Scaleway backend from environment variables.
    ///
    /// Reads:
    /// - `SCALEWAY_SECRET_KEY` — API authentication
    /// - `SCALEWAY_PROJECT_ID` — Project scope
    /// - `SCALEWAY_SESSION_ID` — Active session for job submission
    /// - `SCALEWAY_PLATFORM` — Platform ID (default: "QPU-GARNET-20PQ")
    pub fn new() -> ScalewayResult<Self> {
        let secret_key =
            std::env::var("SCALEWAY_SECRET_KEY").map_err(|_| ScalewayError::MissingToken)?;
        let project_id =
            std::env::var("SCALEWAY_PROJECT_ID").map_err(|_| ScalewayError::MissingProjectId)?;
        let session_id =
            std::env::var("SCALEWAY_SESSION_ID").map_err(|_| ScalewayError::MissingSession)?;
        let platform =
            std::env::var("SCALEWAY_PLATFORM").unwrap_or_else(|_| "QPU-GARNET-20PQ".into());

        let config = BackendConfig::new("scaleway")
            .with_endpoint(DEFAULT_BASE_URL)
            .with_token(&secret_key);

        let client = ScalewayClient::new(&secret_key, &project_id)?;

        let capabilities = Self::capabilities_for_platform(&platform)
            .map_err(|e| ScalewayError::CircuitValidation(e.to_string()))?;

        Ok(Self {
            config,
            client,
            session_id,
            platform,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
        })
    }

    /// Create a backend with explicit credentials.
    pub fn with_credentials(
        secret_key: impl Into<String>,
        project_id: impl Into<String>,
        session_id: impl Into<String>,
        platform: impl Into<String>,
    ) -> ScalewayResult<Self> {
        let secret_key = secret_key.into();
        let project_id = project_id.into();
        let session_id = session_id.into();
        let platform = platform.into();

        let config = BackendConfig::new("scaleway")
            .with_endpoint(DEFAULT_BASE_URL)
            .with_token(&secret_key);

        let client = ScalewayClient::new(&secret_key, &project_id)?;

        let capabilities = Self::capabilities_for_platform(&platform)
            .map_err(|e| ScalewayError::CircuitValidation(e.to_string()))?;

        Ok(Self {
            config,
            client,
            session_id,
            platform,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
        })
    }

    fn from_config_impl(config: BackendConfig) -> ScalewayResult<Self> {
        let secret_key = config.token.as_ref().ok_or(ScalewayError::MissingToken)?;

        let project_id = config
            .extra
            .get("project_id")
            .and_then(|v| v.as_str())
            .ok_or(ScalewayError::MissingProjectId)?
            .to_string();

        let session_id = config
            .extra
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or(ScalewayError::MissingSession)?
            .to_string();

        let platform = config
            .extra
            .get("platform")
            .and_then(|v| v.as_str())
            .map_or_else(|| "QPU-GARNET-20PQ".to_string(), str::to_string);

        let client = ScalewayClient::new(secret_key, &project_id)?;

        let capabilities = Self::capabilities_for_platform(&platform)
            .map_err(|e| ScalewayError::CircuitValidation(e.to_string()))?;

        Ok(Self {
            config,
            client,
            session_id,
            platform,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
        })
    }

    /// Build capabilities for a known Scaleway platform.
    ///
    /// # Errors
    ///
    /// Returns `Err` for unknown platform strings (DEBT-16: silently defaulting
    /// to star topology would produce wrong routing for new Scaleway platforms).
    fn capabilities_for_platform(platform: &str) -> Result<Capabilities, HalError> {
        match platform {
            // IQM Sirius 16-qubit — star-24 topology.
            "QPU-SIRIUS-24PQ" => Ok(Capabilities::iqm("scaleway-sirius-16", 16)),
            // IQM Garnet 20-qubit — crystal-20 topology.
            "QPU-GARNET-20PQ" => Ok(Capabilities::iqm("scaleway-garnet-20", 20)),
            // IQM Emerald 54-qubit — crystal-54 topology.
            "QPU-EMERALD-54PQ" => Ok(Capabilities::iqm("scaleway-emerald-54", 54)),
            // DEBT-16: error on unknown platform instead of silently defaulting.
            _ => Err(HalError::Backend(format!(
                "Unknown Scaleway platform: {platform}. \
                 Known platforms: QPU-SIRIUS-24PQ, QPU-GARNET-20PQ, QPU-EMERALD-54PQ"
            ))),
        }
    }

    /// Get the active session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the platform identifier.
    pub fn platform(&self) -> &str {
        &self.platform
    }

    /// Convert an Arvak IR circuit to QASM3 string.
    ///
    /// Scaleway's backend expects `include "stdgates.inc";` in the QASM3
    /// header (matching Qiskit's `qasm3.dumps()` output). Arvak's emitter
    /// omits it, so we inject it after the version line.
    fn circuit_to_qasm(&self, circuit: &Circuit) -> ScalewayResult<String> {
        let qasm =
            arvak_qasm3::emit(circuit).map_err(|e| ScalewayError::QasmError(e.to_string()))?;
        // Insert `include "stdgates.inc";` after the OPENQASM 3.0; line
        let qasm = qasm.replacen(
            "OPENQASM 3.0;",
            "OPENQASM 3.0;\ninclude \"stdgates.inc\";",
            1,
        );
        Ok(qasm)
    }

    /// Parse result_distribution JSON into Counts.
    fn parse_result_distribution(&self, value: &serde_json::Value) -> Counts {
        let mut counts = Counts::new();

        if let Some(map) = value.as_object() {
            for (bitstring, count) in map {
                if let Some(n) = count.as_u64() {
                    counts.insert(bitstring.clone(), n);
                }
            }
        }

        counts
    }

    /// Parse job results from the results endpoint into Counts.
    fn parse_job_results(&self, results: &[crate::api::JobResultEntry]) -> Counts {
        let mut counts = Counts::new();

        for entry in results {
            if let Some(ref result) = entry.result {
                // Result can be a distribution map directly
                if let Some(map) = result.as_object() {
                    for (bitstring, count) in map {
                        if let Some(n) = count.as_u64() {
                            counts.insert(bitstring.clone(), n);
                        }
                    }
                }
            }
        }

        counts
    }

    /// Poll a job until it reaches a terminal state.
    pub async fn wait_for_job(&self, job_id: &str) -> ScalewayResult<JobResponse> {
        let start = std::time::Instant::now();

        loop {
            let response = self.client.get_job(job_id).await?;

            if response.is_completed() || response.is_failed() || response.is_cancelled() {
                return Ok(response);
            }

            if start.elapsed() > MAX_WAIT_TIME {
                return Err(ScalewayError::Timeout(job_id.to_string()));
            }

            debug!(
                "Job {} status: {} — waiting {}s",
                job_id,
                response.status,
                POLL_INTERVAL.as_secs()
            );
            sleep(POLL_INTERVAL).await;
        }
    }
}

#[async_trait]
impl Backend for ScalewayBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.client.get_session(&self.session_id).await {
            Ok(session) => {
                if session.is_running() {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: session.waiting_job_count,
                        estimated_wait: None,
                        status_message: session.progress_message,
                    })
                } else if session.is_starting() {
                    Ok(BackendAvailability {
                        is_available: false,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: Some(format!(
                            "Session starting ({})",
                            session.progress_message.unwrap_or_default()
                        )),
                    })
                } else {
                    Ok(BackendAvailability::unavailable(format!(
                        "Session status: {}",
                        session.status
                    )))
                }
            }
            Err(e) => {
                warn!("Session availability check failed: {}", e);
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
                self.platform,
                caps.num_qubits
            ));
        }

        // Check gate set support
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if !gate_set.contains(name) {
                    reasons.push(format!("Unsupported gate: {name}"));
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
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        info!(
            "Submitting circuit to Scaleway {} (session {}): {} qubits, {} shots",
            self.platform,
            self.session_id,
            circuit.num_qubits(),
            shots
        );

        // Pre-submission shot-count validation (DEBT-17).
        // IQM/Scaleway limits: 1 ≤ shots ≤ 100,000.
        if shots == 0 {
            return Err(HalError::InvalidShots("shots must be ≥ 1".to_string()));
        }
        if shots > 100_000 {
            return Err(HalError::InvalidShots(format!(
                "shots {shots} exceeds IQM/Scaleway maximum of 100,000"
            )));
        }

        // Pre-submission circuit validation (gate set + qubit count).
        if let ValidationResult::Invalid { reasons } = self.validate(circuit).await? {
            return Err(HalError::InvalidCircuit(reasons.join("; ")));
        }

        // Validate circuit size
        let caps = self.capabilities();
        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but {} only supports {}",
                circuit.num_qubits(),
                self.platform,
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
        debug!("Generated QASM ({} bytes)", qasm.len());

        // Step 1: Compress QASM3 and build computation model
        let compressed = compress_qasm(&qasm).map_err(|e| HalError::Backend(e.to_string()))?;
        let model_json = build_computation_model(&compressed, &self.platform);
        let model_payload =
            serde_json::to_string(&model_json).map_err(|e| HalError::Backend(e.to_string()))?;

        // Step 2: Upload model to Scaleway
        let model = self
            .client
            .create_model(&model_payload)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;
        debug!("Model uploaded: {}", model.id);

        // Step 3: Create job referencing the model
        let request = CreateJobRequest::new(&self.session_id, &model.id, shots);

        let response = self
            .client
            .create_job(&request)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.id);
        info!("Job submitted: {} (status: {})", job_id, response.status);

        // Cache job info
        let job = Job::new(job_id.clone(), shots).with_backend(&self.platform);
        {
            let mut jobs = self.jobs.lock().await;
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.job.status.is_terminal());
            }
            jobs.insert(job_id.0.clone(), CachedJob { job, result: None });
        }

        Ok(job_id)
    }

    #[instrument(skip(self))]
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let response = self.client.get_job(&job_id.0).await.map_err(|e| match e {
            ScalewayError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        let status = if response.is_completed() {
            JobStatus::Completed
        } else if response.is_failed() {
            JobStatus::Failed(
                response
                    .progress_message
                    .unwrap_or_else(|| "Job failed".into()),
            )
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

        // First try: check if result_distribution is inline on the job itself
        let job_response = self.client.get_job(&job_id.0).await.map_err(|e| match e {
            ScalewayError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        if job_response.is_failed() {
            return Err(HalError::JobFailed(
                job_response
                    .progress_message
                    .unwrap_or_else(|| "Job failed".into()),
            ));
        }

        // Try inline result_distribution first
        let counts = if let Some(ref dist) = job_response.result_distribution {
            let c = self.parse_result_distribution(dist);
            if c.total_shots() > 0 {
                c
            } else {
                // Fallback: fetch from results endpoint
                let results = self
                    .client
                    .list_job_results(&job_id.0)
                    .await
                    .map_err(|e| HalError::Backend(e.to_string()))?;

                let c = self.parse_job_results(&results.job_results);
                if c.total_shots() == 0 {
                    return Err(HalError::JobFailed("No measurement results".into()));
                }
                c
            }
        } else {
            // No inline results — fetch from results endpoint
            let results = self
                .client
                .list_job_results(&job_id.0)
                .await
                .map_err(|e| HalError::Backend(e.to_string()))?;

            let c = self.parse_job_results(&results.job_results);
            if c.total_shots() == 0 {
                return Err(HalError::JobFailed("No measurement results".into()));
            }
            c
        };

        let shots = counts.total_shots() as u32;
        let mut result = ExecutionResult::new(counts, shots);

        // Add execution metadata
        if let Some(ref duration) = job_response.job_duration {
            // Parse duration string like "2.5s" → milliseconds
            if let Some(secs_str) = duration.strip_suffix('s') {
                if let Ok(secs) = secs_str.parse::<f64>() {
                    result = result.with_execution_time((secs * 1000.0) as u64);
                }
            }
        }

        result = result.with_metadata(serde_json::json!({
            "platform": self.platform,
            "session_id": self.session_id,
            "provider": "scaleway",
        }));

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
                ScalewayError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
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

impl BackendFactory for ScalewayBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        Self::from_config_impl(config).map_err(|e| HalError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config() {
        let config = BackendConfig::new("scaleway")
            .with_endpoint(DEFAULT_BASE_URL)
            .with_token("test-token")
            .with_extra("project_id", serde_json::json!("proj-123"))
            .with_extra("session_id", serde_json::json!("sess-456"))
            .with_extra("platform", serde_json::json!("QPU-GARNET-20PQ"));

        assert_eq!(config.name, "scaleway");
        assert!(config.extra.contains_key("project_id"));
        assert!(config.extra.contains_key("session_id"));
    }

    #[test]
    fn test_capabilities_garnet() {
        let caps = ScalewayBackend::capabilities_for_platform("QPU-GARNET-20PQ").unwrap();
        assert_eq!(caps.num_qubits, 20);
        assert!(!caps.is_simulator);
    }

    #[test]
    fn test_capabilities_emerald() {
        let caps = ScalewayBackend::capabilities_for_platform("QPU-EMERALD-54PQ").unwrap();
        assert_eq!(caps.num_qubits, 54);
    }

    #[test]
    fn test_capabilities_unknown_platform() {
        // DEBT-16 fix: unknown platforms must return an error, not silently default.
        let result = ScalewayBackend::capabilities_for_platform("QPU-UNKNOWN-100PQ");
        assert!(result.is_err(), "Unknown platform should return an error");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Unknown Scaleway platform"),
            "Error should mention platform name"
        );
    }

    #[test]
    fn test_parse_result_distribution() {
        // Create a minimal backend for testing parse methods.
        // We can't easily instantiate ScalewayBackend without credentials,
        // so test the parsing logic directly.
        let dist = serde_json::json!({"00": 2048, "11": 1952});
        let mut counts = Counts::new();
        if let Some(map) = dist.as_object() {
            for (bitstring, count) in map {
                if let Some(n) = count.as_u64() {
                    counts.insert(bitstring.clone(), n);
                }
            }
        }
        assert_eq!(counts.total_shots(), 4000);
    }

    #[test]
    fn test_parse_duration_string() {
        // Test the duration parsing logic
        let duration = "2.5s";
        if let Some(secs_str) = duration.strip_suffix('s') {
            let secs: f64 = secs_str.parse().unwrap();
            assert_eq!((secs * 1000.0) as u64, 2500);
        }
    }
}
