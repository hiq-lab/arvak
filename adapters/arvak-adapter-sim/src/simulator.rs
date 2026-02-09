//! Simulator backend implementation.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, instrument};
use uuid::Uuid;

use arvak_hal::{
    Backend, BackendConfig, BackendFactory, Capabilities, Counts, ExecutionResult, HalError,
    HalResult, Job, JobId, JobStatus,
};
use arvak_ir::Circuit;

use crate::statevector::Statevector;

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
    /// Active jobs.
    jobs: Arc<Mutex<FxHashMap<String, SimJob>>>,
    /// Maximum number of qubits supported.
    max_qubits: u32,
}

impl SimulatorBackend {
    /// Create a new simulator backend with default settings.
    pub fn new() -> Self {
        Self {
            config: BackendConfig::new("simulator"),
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits: 20,
        }
    }

    /// Create a simulator with custom max qubits.
    pub fn with_max_qubits(max_qubits: u32) -> Self {
        Self {
            config: BackendConfig::new("simulator"),
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
        }
    }

    /// Run simulation synchronously.
    #[instrument(skip(self, circuit))]
    fn run_simulation(&self, circuit: &Circuit, shots: u32) -> ExecutionResult {
        let start = Instant::now();

        let num_qubits = circuit.num_qubits();
        debug!(
            "Starting simulation: {} qubits, {} shots",
            num_qubits, shots
        );

        let mut counts = Counts::new();

        // Collect instructions
        let instructions: Vec<_> = circuit
            .dag()
            .topological_ops()
            .map(|(_, inst)| inst.clone())
            .collect();

        debug!("Circuit has {} instructions", instructions.len());

        // Run shots
        for shot in 0..shots {
            // Initialize statevector
            let mut sv = Statevector::new(num_qubits);

            // Apply all gates
            for inst in &instructions {
                sv.apply(inst);
            }

            // Sample and record result
            let outcome = sv.sample();
            let bitstring = sv.outcome_to_bitstring(outcome);
            counts.insert(bitstring, 1);

            if shot > 0 && shot % 1000 == 0 {
                debug!("Completed {} shots", shot);
            }
        }

        let elapsed = start.elapsed();
        debug!("Simulation completed in {:?}", elapsed);

        ExecutionResult::new(counts, shots).with_execution_time(elapsed.as_millis() as u64)
    }
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

    async fn capabilities(&self) -> HalResult<Capabilities> {
        Ok(Capabilities::simulator(self.max_qubits))
    }

    async fn is_available(&self) -> HalResult<bool> {
        Ok(true)
    }

    #[instrument(skip(self, circuit))]
    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
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

        // Store job
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            jobs.insert(job_id.0.clone(), sim_job);
        }

        debug!("Submitted job: {}", job_id);

        // Run simulation immediately (in a real implementation, this would be async)
        let result = self.run_simulation(circuit, shots);

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
            .map_or(20, |v| v as u32);

        Ok(Self {
            config,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simulator_capabilities() {
        let backend = SimulatorBackend::new();
        let caps = backend.capabilities().await.unwrap();

        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 20);
    }

    #[tokio::test]
    async fn test_simulator_bell_state() {
        let backend = SimulatorBackend::new();

        let circuit = Circuit::bell().unwrap();
        let job_id = backend.submit(&circuit, 1000).await.unwrap();

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
        let job_id = backend.submit(&circuit, 1000).await.unwrap();

        let result = backend.result(&job_id).await.unwrap();

        // GHZ state should produce only 000 and 111
        let counts = &result.counts;
        assert!(counts.get("000") + counts.get("111") == 1000);
    }

    #[tokio::test]
    async fn test_simulator_too_many_qubits() {
        let backend = SimulatorBackend::with_max_qubits(5);

        let circuit = Circuit::with_size("test", 10, 0);
        let result = backend.submit(&circuit, 100).await;

        assert!(matches!(result, Err(HalError::CircuitTooLarge(_))));
    }
}
