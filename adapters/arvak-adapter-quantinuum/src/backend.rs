//! Quantinuum backend implementation.

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

use crate::api::{JobRequest, MachineInfo, QuantinuumClient};
use crate::error::{QuantinuumError, QuantinuumResult};

/// Default target machine (noiseless H2 emulator — free to use).
pub const DEFAULT_MACHINE: &str = "H2-1LE";

/// H2 emulator: 32 qubits, all-to-all.
const DEFAULT_NUM_QUBITS: u32 = 32;

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// How long to cache machine info before refreshing from the API.
const MACHINE_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// Cached job entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// Quantinuum ion-trap quantum computer backend.
///
/// Supports H1 (20 qubits) and H2 (32 qubits) hardware machines, plus their
/// noiseless emulators (`H1-1E`, `H2-1E`, `H2-1LE`).  All devices have
/// all-to-all qubit connectivity, so no routing is required.
///
/// # Authentication
///
/// Set `QUANTINUUM_EMAIL` and `QUANTINUUM_PASSWORD` environment variables.
/// The backend exchanges them for a JWT on first use and refreshes the token
/// automatically on expiry.
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_quantinuum::QuantinuumBackend;
///
/// let backend = QuantinuumBackend::new()?;           // H2-1LE (noiseless emulator)
/// let backend = QuantinuumBackend::with_target("H2-1")?;  // real H2 hardware
/// ```
pub struct QuantinuumBackend {
    /// REST API client.
    client: QuantinuumClient,
    /// Target machine name.
    target: String,
    /// Cached HAL capabilities (synchronous introspection).
    capabilities: Capabilities,
    /// Cached job metadata and results.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached machine info with TTL.
    machine_info: Arc<Mutex<Option<(MachineInfo, Instant)>>>,
}

impl QuantinuumBackend {
    /// Create a backend targeting the default noiseless emulator (`H2-1LE`).
    ///
    /// Reads `QUANTINUUM_EMAIL` and `QUANTINUUM_PASSWORD` from the environment.
    pub fn new() -> QuantinuumResult<Self> {
        Self::with_target(DEFAULT_MACHINE)
    }

    /// Create a backend targeting a specific Quantinuum machine.
    ///
    /// Reads `QUANTINUUM_EMAIL` and `QUANTINUUM_PASSWORD` from the environment.
    pub fn with_target(target: impl Into<String>) -> QuantinuumResult<Self> {
        let email = std::env::var("QUANTINUUM_EMAIL").map_err(|_| QuantinuumError::MissingEmail)?;
        let password =
            std::env::var("QUANTINUUM_PASSWORD").map_err(|_| QuantinuumError::MissingPassword)?;

        let target = target.into();
        let client = QuantinuumClient::new(email, password)?;
        let capabilities = build_capabilities(&target, DEFAULT_NUM_QUBITS);

        Ok(Self {
            client,
            target,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            machine_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a backend with explicit credentials (useful for testing).
    pub fn with_credentials(
        target: impl Into<String>,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> QuantinuumResult<Self> {
        let target = target.into();
        let client = QuantinuumClient::new(email, password)?;
        let capabilities = build_capabilities(&target, DEFAULT_NUM_QUBITS);

        Ok(Self {
            client,
            target,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            machine_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Return the target machine name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Fetch machine info from API, using the cache if not stale.
    async fn fetch_machine_info(&self) -> QuantinuumResult<MachineInfo> {
        {
            let cache = self.machine_info.lock().await;
            if let Some((ref info, fetched_at)) = *cache {
                if fetched_at.elapsed() < MACHINE_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        let info = self.client.get_machine(&self.target).await?;

        {
            let mut cache = self.machine_info.lock().await;
            *cache = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Convert a circuit to QASM 2.0 string for submission.
    fn circuit_to_qasm2(circuit: &Circuit) -> QuantinuumResult<String> {
        arvak_qasm3::emit_qasm2(circuit).map_err(|e| QuantinuumError::QasmError(e.to_string()))
    }

    /// Convert Quantinuum per-register bit arrays to a `Counts` histogram.
    ///
    /// The API returns `results` as `{register_name: [bit_shot_0, bit_shot_1, ...]}`.
    /// We sort register names and concatenate bits per shot to form bitstrings.
    fn parse_results(results: &std::collections::HashMap<String, Vec<u8>>) -> Counts {
        let mut counts = Counts::new();

        if results.is_empty() {
            return counts;
        }

        // Number of shots is the length of any register's array.
        let n_shots = results.values().next().map(|v| v.len()).unwrap_or(0);
        if n_shots == 0 {
            return counts;
        }

        // Sort register names so the bit ordering is deterministic.
        let mut reg_names: Vec<&String> = results.keys().collect();
        reg_names.sort();

        for shot in 0..n_shots {
            let bitstring: String = reg_names
                .iter()
                .map(|reg| {
                    let bit = results[*reg].get(shot).copied().unwrap_or(0);
                    if bit != 0 { '1' } else { '0' }
                })
                .collect();
            // Counts::insert accumulates: repeated bitstrings correctly increment.
            counts.insert(bitstring, 1);
        }

        counts
    }
}

/// Build `Capabilities` for a Quantinuum machine from its name.
fn build_capabilities(target: &str, num_qubits: u32) -> Capabilities {
    let is_simulator = target.ends_with('E') || target.ends_with("LE");
    Capabilities::quantinuum(target, num_qubits).with_simulator(is_simulator)
}

#[async_trait]
impl Backend for QuantinuumBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "quantinuum"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.fetch_machine_info().await {
            Ok(info) => {
                if info.is_online() {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: None,
                    })
                } else {
                    let msg = info.status.as_deref().unwrap_or("offline").to_string();
                    Ok(BackendAvailability::unavailable(msg))
                }
            }
            Err(e) => {
                debug!("Quantinuum availability check failed: {}", e);
                Ok(BackendAvailability::unavailable(e.to_string()))
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but {} supports at most {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            ));
        }

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
            "Submitting circuit to Quantinuum {}: {} qubits, {} shots",
            self.target,
            circuit.num_qubits(),
            shots
        );

        let caps = self.capabilities();
        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but {} supports at most {}",
                circuit.num_qubits(),
                self.target,
                caps.num_qubits
            )));
        }

        if shots > caps.max_shots {
            return Err(HalError::InvalidShots(format!(
                "Requested {shots} shots but maximum is {}",
                caps.max_shots
            )));
        }

        let qasm2 =
            Self::circuit_to_qasm2(circuit).map_err(|e| HalError::Backend(e.to_string()))?;
        debug!("Generated QASM2 ({} chars)", qasm2.len());

        let req = JobRequest::new(&self.target, qasm2, shots);

        let response = self
            .client
            .submit_job(&req)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.job);
        info!("Job submitted: {}", job_id);

        let job = Job::new(job_id.clone(), shots).with_backend(&self.target);
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
            QuantinuumError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        let status = if response.is_completed() {
            JobStatus::Completed
        } else if response.is_failed() {
            let msg = response.error.unwrap_or_default();
            JobStatus::Failed(msg)
        } else if response.is_cancelled() {
            JobStatus::Cancelled
        } else if response.status.to_lowercase() == "running" {
            JobStatus::Running
        } else {
            JobStatus::Queued
        };

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
        // Return from cache if available.
        {
            let jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get(&job_id.0) {
                if let Some(ref result) = cached.result {
                    return Ok(result.clone());
                }
            }
        }

        let response = self.client.get_job(&job_id.0).await.map_err(|e| match e {
            QuantinuumError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        if let Some(error) = response.error {
            return Err(HalError::JobFailed(error));
        }

        if !response.is_completed() {
            return Err(HalError::Backend(format!(
                "Job {} is not yet completed (status: {})",
                job_id, response.status
            )));
        }

        let api_results = response.results.ok_or_else(|| {
            HalError::JobFailed("Completed job returned no measurement results".into())
        })?;

        let counts = Self::parse_results(&api_results);
        let shots = u32::try_from(counts.total_shots()).unwrap_or(u32::MAX);
        let result = ExecutionResult::new(counts, shots);

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
                QuantinuumError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

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

impl BackendFactory for QuantinuumBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let target = config
            .extra
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MACHINE)
            .to_string();

        Self::with_target(target).map_err(|e| HalError::Backend(e.to_string()))
    }
}

/// Extension trait to allow `Capabilities::quantinuum()` to also set the
/// `is_simulator` flag.  Used only internally in this adapter.
trait CapabilitiesExt {
    fn with_simulator(self, is_simulator: bool) -> Self;
}

impl CapabilitiesExt for Capabilities {
    fn with_simulator(mut self, is_simulator: bool) -> Self {
        self.is_simulator = is_simulator;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_results_bell_state() {
        let mut results = std::collections::HashMap::new();
        // 4 shots: |00⟩, |11⟩, |00⟩, |11⟩
        results.insert("c_0".to_string(), vec![0, 1, 0, 1]);
        results.insert("c_1".to_string(), vec![0, 1, 0, 1]);

        let counts = QuantinuumBackend::parse_results(&results);
        let sorted = counts.sorted();
        assert_eq!(sorted.len(), 2);
        let map: std::collections::HashMap<String, u64> =
            sorted.into_iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(map["00"], 2);
        assert_eq!(map["11"], 2);
    }

    #[test]
    fn test_parse_results_empty() {
        let results = std::collections::HashMap::new();
        let counts = QuantinuumBackend::parse_results(&results);
        assert_eq!(counts.total_shots(), 0);
    }

    #[test]
    fn test_build_capabilities_emulator() {
        let caps = build_capabilities("H2-1LE", 32);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 32);
    }

    #[test]
    fn test_build_capabilities_hardware() {
        let caps = build_capabilities("H2-1", 32);
        assert!(!caps.is_simulator);
    }
}
