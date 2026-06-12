//! Simulator backend implementation.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, instrument};
use uuid::Uuid;

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;

use crate::statevector::Statevector;

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// Job data for the simulator.
struct SimJob {
    job: Job,
    #[allow(dead_code)]
    circuit: Circuit,
    result: Option<ExecutionResult>,
}

/// Local simulator backend.
///
/// This backend simulates quantum circuits using a statevector simulation.
/// It supports circuits up to ~20 qubits (limited by memory).
pub struct SimulatorBackend {
    /// Backend configuration.
    config: BackendConfig,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Active jobs.
    jobs: Arc<Mutex<FxHashMap<String, SimJob>>>,
    /// Maximum number of qubits supported.
    max_qubits: u32,
    /// Optional RNG seed for reproducible sampling.
    seed: Option<u64>,
}

impl SimulatorBackend {
    /// Create a new simulator backend with default settings.
    pub fn new() -> Self {
        let max_qubits = 20;
        Self {
            config: BackendConfig::new("simulator"),
            capabilities: Capabilities::simulator(max_qubits),
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
            seed: None,
        }
    }

    /// Create a simulator with custom max qubits.
    pub fn with_max_qubits(max_qubits: u32) -> Self {
        Self {
            config: BackendConfig::new("simulator"),
            capabilities: Capabilities::simulator(max_qubits),
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
            seed: None,
        }
    }

    /// Set the RNG seed for reproducible measurement sampling.
    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Run simulation synchronously.
    ///
    /// This is the core simulation engine. Circuits without `Reset`
    /// instructions are deterministic up to the final measurement, so the
    /// statevector is evolved ONCE and the histogram is sampled from the
    /// final distribution — O(G·2^n + shots·n) instead of the previous
    /// O(shots·G·2^n) per-shot re-simulation. Circuits containing `Reset`
    /// collapse stochastically mid-circuit and are re-run per shot.
    ///
    /// Returns an error if a gate has unresolved symbolic parameters or if an
    /// unsupported gate type is encountered.
    ///
    /// The method is public so that the Python bindings can call it directly
    /// without going through the async [`Backend`] trait.
    #[instrument(skip(self, circuit))]
    pub fn run_simulation(&self, circuit: &Circuit, shots: u32) -> Result<ExecutionResult, String> {
        run_simulation_seeded(circuit, shots, self.seed)
    }
}

/// Free-standing simulation engine (does not need backend state).
///
/// `seed` makes runs reproducible; `None` seeds from OS entropy.
pub fn run_simulation_seeded(
    circuit: &Circuit,
    shots: u32,
    seed: Option<u64>,
) -> Result<ExecutionResult, String> {
    use rand::SeedableRng;

    let start = Instant::now();

    let num_qubits = circuit.num_qubits();
    debug!(
        "Starting simulation: {} qubits, {} shots",
        num_qubits, shots
    );

    let mut rng = match seed {
        Some(s) => rand::rngs::StdRng::seed_from_u64(s),
        None => rand::rngs::StdRng::from_entropy(),
    };

    // Collect instructions
    let instructions: Vec<_> = circuit
        .dag()
        .topological_ops()
        .map(|(_, inst)| inst.clone())
        .collect();

    debug!("Circuit has {} instructions", instructions.len());

    let has_reset = instructions
        .iter()
        .any(|inst| matches!(inst.kind, arvak_ir::InstructionKind::Reset));

    let mut counts = Counts::new();

    if has_reset {
        // Mid-circuit reset collapses stochastically: each shot is an
        // independent trajectory.
        for shot in 0..shots {
            let mut sv = Statevector::new(num_qubits);
            for inst in &instructions {
                sv.apply(inst, &mut rng)?;
            }
            let outcome = sv.sample(&mut rng);
            counts.insert(sv.outcome_to_bitstring(outcome), 1);

            if shot > 0 && shot % 1000 == 0 {
                debug!("Completed {} shots", shot);
            }
        }
    } else {
        // Deterministic evolution: simulate once, sample the distribution.
        let mut sv = Statevector::new(num_qubits);
        for inst in &instructions {
            sv.apply(inst, &mut rng)?;
        }
        for (outcome, count) in sv.sample_counts(shots, &mut rng) {
            counts.insert(sv.outcome_to_bitstring(outcome), count.into());
        }
    }

    let elapsed = start.elapsed();
    debug!("Simulation completed in {:?}", elapsed);

    Ok(ExecutionResult::new(counts, shots).with_execution_time(elapsed.as_millis() as u64))
}

impl Default for SimulatorBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for SimulatorBackend {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        Ok(BackendAvailability::always_available())
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        if circuit.num_qubits() > self.max_qubits as usize {
            return Ok(ValidationResult::Invalid {
                reasons: vec![format!(
                    "Circuit has {} qubits but simulator only supports {}",
                    circuit.num_qubits(),
                    self.max_qubits
                )],
            });
        }
        Ok(ValidationResult::Valid)
    }

    #[instrument(skip(self, circuit, parameters))]
    async fn submit(
        &self,
        circuit: &Circuit,
        shots: u32,
        parameters: Option<&std::collections::HashMap<String, f64>>,
    ) -> HalResult<JobId> {
        // DEBT-25: reject non-empty parameter bindings (not yet supported).
        if parameters.is_some_and(|p| !p.is_empty()) {
            return Err(HalError::Unsupported(
                "Simulator does not support runtime parameter binding".into(),
            ));
        }

        // Validate circuit size
        if circuit.num_qubits() > self.max_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but simulator only supports {}",
                circuit.num_qubits(),
                self.max_qubits
            )));
        }

        // Generate job ID
        let job_id = JobId::new(Uuid::new_v4().to_string());

        // Create job
        let job = Job::new(job_id.clone(), shots).with_backend("simulator");

        let sim_job = SimJob {
            job,
            circuit: circuit.clone(),
            result: None,
        };

        // Store job, evicting completed entries if the cache is full.
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.job.status.is_terminal());
            }
            jobs.insert(job_id.0.clone(), sim_job);
        }

        debug!("Submitted job: {}", job_id);

        // Run simulation on a blocking thread to avoid starving the async runtime.
        let circuit_clone = circuit.clone();
        let seed = self.seed;
        let result =
            tokio::task::spawn_blocking(move || run_simulation_seeded(&circuit_clone, shots, seed))
                .await
                .map_err(|e| HalError::Backend(format!("simulation task panicked: {e}")))?
                .map_err(|e| HalError::Backend(format!("simulation failed: {e}")))?;

        // Update job with result
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(sim_job) = jobs.get_mut(&job_id.0) {
                sim_job.result = Some(result);
                sim_job.job = sim_job.job.clone().with_status(JobStatus::Completed);
            }
        }

        Ok(job_id)
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        jobs.get(&job_id.0)
            .map(|j| j.job.status.clone())
            .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        let jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        jobs.get(&job_id.0)
            .and_then(|j| j.result.clone())
            .ok_or_else(|| HalError::JobNotFound(job_id.0.clone()))
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sim_job) = jobs.get_mut(&job_id.0) {
            sim_job.job = sim_job.job.clone().with_status(JobStatus::Cancelled);
            Ok(())
        } else {
            Err(HalError::JobNotFound(job_id.0.clone()))
        }
    }
}

impl BackendFactory for SimulatorBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let max_qubits = config
            .extra
            .get("max_qubits")
            .and_then(serde_json::value::Value::as_u64)
            .map_or(20, |v| u32::try_from(v).unwrap_or(20));

        if max_qubits == 0 {
            return Err(HalError::Backend(
                "max_qubits must be greater than 0".to_string(),
            ));
        }

        let seed = config
            .extra
            .get("seed")
            .and_then(serde_json::value::Value::as_u64);

        Ok(Self {
            capabilities: Capabilities::simulator(max_qubits),
            config,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
            seed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulator_capabilities() {
        let backend = SimulatorBackend::new();
        let caps = backend.capabilities();

        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 20);
    }

    #[tokio::test]
    async fn test_bitstring_convention_q0_rightmost() {
        // HAL Contract conformance: qubit 0 is the RIGHTMOST character
        // (OpenQASM 3 / Qiskit convention). X on q0 of a 2-qubit register
        // must yield "01", never "10".
        let backend = SimulatorBackend::new();

        let mut circuit = Circuit::with_size("conformance", 2, 2);
        circuit.x(arvak_ir::QubitId(0)).unwrap();
        circuit.measure_all().unwrap();

        let job_id = backend.submit(&circuit, 100, None).await.unwrap();
        let result = backend.result(&job_id).await.unwrap();
        assert_eq!(
            result.counts.get("01"),
            100,
            "X(q0) must produce \"01\" — q0 is the rightmost bit"
        );
        assert_eq!(result.counts.get("10"), 0);
    }

    #[tokio::test]
    async fn test_seeded_simulation_reproducible() {
        let backend = SimulatorBackend::new().with_seed(42);
        let circuit = Circuit::bell().unwrap();

        let job_a = backend.submit(&circuit, 500, None).await.unwrap();
        let counts_a = backend.result(&job_a).await.unwrap().counts;
        let job_b = backend.submit(&circuit, 500, None).await.unwrap();
        let counts_b = backend.result(&job_b).await.unwrap().counts;
        assert_eq!(counts_a.get("00"), counts_b.get("00"));
        assert_eq!(counts_a.get("11"), counts_b.get("11"));
    }

    #[tokio::test]
    async fn test_simulator_bell_state() {
        let backend = SimulatorBackend::new();

        let circuit = Circuit::bell().unwrap();
        let job_id = backend.submit(&circuit, 1000, None).await.unwrap();

        let status = backend.status(&job_id).await.unwrap();
        assert!(status.is_success());

        let result = backend.result(&job_id).await.unwrap();
        assert_eq!(result.shots, 1000);

        // Bell state should produce only 00 and 11
        let counts = &result.counts;
        assert!(counts.get("00") + counts.get("11") == 1000);
        assert!(counts.get("01") + counts.get("10") == 0);
    }

    #[tokio::test]
    async fn test_simulator_ghz_state() {
        let backend = SimulatorBackend::new();

        let circuit = Circuit::ghz(3).unwrap();
        let job_id = backend.submit(&circuit, 1000, None).await.unwrap();

        let result = backend.result(&job_id).await.unwrap();

        // GHZ state should produce only 000 and 111
        let counts = &result.counts;
        assert!(counts.get("000") + counts.get("111") == 1000);
    }

    #[tokio::test]
    async fn test_simulator_too_many_qubits() {
        let backend = SimulatorBackend::with_max_qubits(5);

        let circuit = Circuit::with_size("test", 10, 0);
        let result = backend.submit(&circuit, 100, None).await;

        assert!(matches!(result, Err(HalError::CircuitTooLarge(_))));
    }
}
