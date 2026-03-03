//! DDSIM backend implementation.
//!
//! Uses MQT DDSIM via a Python subprocess. Circuits are serialized to
//! OpenQASM 2.0, passed to `mqt.ddsim` via a small inline Python script,
//! and the resulting measurement counts are parsed from JSON on stdout.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;

use crate::error::{DdsimError, DdsimResult};

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// Default maximum qubit count for DDSIM.
///
/// Decision-diagram simulation can handle structured circuits at higher qubit
/// counts than statevector, but pathological circuits still blow up.
const DEFAULT_MAX_QUBITS: u32 = 128;

/// Job data for the DDSIM backend.
struct DdsimJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// MQT DDSIM backend adapter.
///
/// Runs quantum circuits through the MQT Decision Diagram Simulator by
/// spawning a Python subprocess. This requires `python3` and the `mqt.ddsim`
/// package to be installed (`pip install mqt.ddsim`).
///
/// # Advantages over statevector simulation
///
/// - **Memory efficiency**: DD-based simulation can represent structured states
///   (GHZ, graph states, stabilizer-like circuits) in polynomial space.
/// - **Cross-validation**: Independent simulator for verifying Arvak's own
///   statevector engine.
/// - **MQT ecosystem**: Part of the Munich Quantum Toolkit, widely used in
///   academic quantum computing research.
pub struct DdsimBackend {
    config: BackendConfig,
    capabilities: Capabilities,
    jobs: Arc<Mutex<FxHashMap<String, DdsimJob>>>,
    max_qubits: u32,
}

impl DdsimBackend {
    /// Create a new DDSIM backend with default settings (128 qubits max).
    pub fn new() -> Self {
        Self::with_max_qubits(DEFAULT_MAX_QUBITS)
    }

    /// Create a DDSIM backend with a custom qubit limit.
    pub fn with_max_qubits(max_qubits: u32) -> Self {
        let mut caps = Capabilities::simulator(max_qubits);
        caps.name = "ddsim".to_string();
        caps.features.push("decision-diagram".to_string());

        Self {
            config: BackendConfig::new("ddsim"),
            capabilities: caps,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
        }
    }

    /// Check whether `python3` and `mqt.ddsim` are available.
    async fn check_ddsim_available() -> DdsimResult<()> {
        let output = tokio::process::Command::new("python3")
            .args(["-c", "import mqt.ddsim; print(mqt.ddsim.__version__)"])
            .output()
            .await
            .map_err(|e| DdsimError::NotAvailable(format!("python3 not found: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DdsimError::NotAvailable(format!(
                "mqt.ddsim not installed: {stderr}"
            )));
        }

        let version = String::from_utf8_lossy(&output.stdout);
        debug!("DDSIM version: {}", version.trim());
        Ok(())
    }

    /// Run a circuit through DDSIM and return measurement counts.
    ///
    /// Uses the `mqt.ddsim.CircuitSimulator` API directly via `mqt.core` IR,
    /// avoiding a Qiskit dependency.  The same QASM2 format is used by the
    /// native QDMI integration (see `crates/arvak-qdmi/tests/ddsim_integration.rs`).
    async fn run_ddsim(qasm: &str, shots: u32) -> DdsimResult<(Counts, u64)> {
        let start = Instant::now();

        // Inline Python script:
        //  1. Write QASM from stdin to a temp file (mqt.core.load needs a path)
        //  2. Load via mqt.core IR (no Qiskit required)
        //  3. Simulate with CircuitSimulator (DD-based weak/sampling mode)
        //  4. Output JSON counts to stdout
        let python_script = format!(
            r#"
import sys, json, tempfile, os
from mqt.ddsim import CircuitSimulator
from mqt.core import load as mqt_load

qasm = sys.stdin.read()
fd, path = tempfile.mkstemp(suffix=".qasm")
try:
    with os.fdopen(fd, 'w') as f:
        f.write(qasm)
    qc = mqt_load(path)
    sim = CircuitSimulator(qc, seed=-1)
    result = sim.simulate({shots})
    # result is dict[str, int] of bitstring -> count
    json.dump(result, sys.stdout)
finally:
    os.unlink(path)
"#
        );

        let mut child = tokio::process::Command::new("python3")
            .args(["-c", &python_script])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| DdsimError::NotAvailable(format!("failed to spawn python3: {e}")))?;

        // Write QASM to stdin
        {
            use tokio::io::AsyncWriteExt;
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                DdsimError::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "failed to open stdin",
                ))
            })?;
            stdin.write_all(qasm.as_bytes()).await?;
            stdin.shutdown().await?;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(DdsimError::ExecutionFailed {
                code: output.status.code(),
                stderr,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let raw_counts: FxHashMap<String, u64> = serde_json::from_str(&stdout)
            .map_err(|e| DdsimError::OutputParse(format!("{e}: {stdout}")))?;

        let mut counts = Counts::new();
        for (bitstring, count) in raw_counts {
            counts.insert(bitstring, count);
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        Ok((counts, elapsed_ms))
    }
}

impl Default for DdsimBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Backend for DdsimBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        match Self::check_ddsim_available().await {
            Ok(()) => Ok(BackendAvailability::always_available()),
            Err(e) => {
                warn!("DDSIM not available: {e}");
                Ok(BackendAvailability {
                    is_available: false,
                    queue_depth: None,
                    estimated_wait: None,
                    status_message: Some(format!("{e}")),
                })
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let mut reasons = Vec::new();

        if circuit.num_qubits() > self.max_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but DDSIM adapter is configured for max {}",
                circuit.num_qubits(),
                self.max_qubits
            ));
        }

        // Verify we can serialize to QASM
        if let Err(e) = arvak_qasm3::emit_qasm2(circuit) {
            reasons.push(format!("Cannot serialize to QASM2: {e}"));
        }

        if reasons.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid { reasons })
        }
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
                "DDSIM backend does not support runtime parameter binding".into(),
            ));
        }

        // Validate qubit count
        if circuit.num_qubits() > self.max_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but DDSIM adapter supports max {}",
                circuit.num_qubits(),
                self.max_qubits
            )));
        }

        // Serialize to QASM2
        let qasm =
            arvak_qasm3::emit_qasm2(circuit).map_err(|e| DdsimError::QasmEmit(e.to_string()))?;

        debug!(
            "Submitting to DDSIM: {} qubits, {} shots, {} bytes QASM",
            circuit.num_qubits(),
            shots,
            qasm.len()
        );

        // Generate job ID
        let job_id = JobId::new(Uuid::new_v4().to_string());
        let job = Job::new(job_id.clone(), shots).with_backend("ddsim");

        // Store pending job with eviction
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.job.status.is_terminal());
            }
            jobs.insert(job_id.0.clone(), DdsimJob { job, result: None });
        }

        // Run DDSIM on a spawned task (already async via subprocess)
        let (counts, elapsed_ms) = Self::run_ddsim(&qasm, shots).await.map_err(|e| {
            // Mark job as failed
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ddsim_job) = jobs.get_mut(&job_id.0) {
                ddsim_job.job = ddsim_job
                    .job
                    .clone()
                    .with_status(JobStatus::Failed(e.to_string()));
            }
            HalError::from(e)
        })?;

        let result = ExecutionResult::new(counts, shots).with_execution_time(elapsed_ms);

        // Update job with result
        {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(ddsim_job) = jobs.get_mut(&job_id.0) {
                ddsim_job.result = Some(result);
                ddsim_job.job = ddsim_job.job.clone().with_status(JobStatus::Completed);
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
        if let Some(ddsim_job) = jobs.get_mut(&job_id.0) {
            ddsim_job.job = ddsim_job.job.clone().with_status(JobStatus::Cancelled);
            Ok(())
        } else {
            Err(HalError::JobNotFound(job_id.0.clone()))
        }
    }
}

impl BackendFactory for DdsimBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let max_qubits = config
            .extra
            .get("max_qubits")
            .and_then(serde_json::value::Value::as_u64)
            .map_or(DEFAULT_MAX_QUBITS, |v| {
                u32::try_from(v).unwrap_or(DEFAULT_MAX_QUBITS)
            });

        if max_qubits == 0 {
            return Err(HalError::Backend(
                "max_qubits must be greater than 0".to_string(),
            ));
        }

        let mut caps = Capabilities::simulator(max_qubits);
        caps.name = "ddsim".to_string();
        caps.features.push("decision-diagram".to_string());

        Ok(Self {
            capabilities: caps,
            config,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            max_qubits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ddsim_capabilities() {
        let backend = DdsimBackend::new();
        let caps = backend.capabilities();

        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, DEFAULT_MAX_QUBITS);
        assert!(caps.features.contains(&"decision-diagram".to_string()));
    }

    #[test]
    fn test_ddsim_custom_qubits() {
        let backend = DdsimBackend::with_max_qubits(64);
        assert_eq!(backend.capabilities().num_qubits, 64);
    }

    #[test]
    fn test_ddsim_from_config() {
        let config = BackendConfig::new("ddsim");
        let backend = DdsimBackend::from_config(config).unwrap();
        assert_eq!(backend.capabilities().num_qubits, DEFAULT_MAX_QUBITS);
    }

    #[test]
    fn test_ddsim_from_config_custom() {
        let mut config = BackendConfig::new("ddsim");
        config
            .extra
            .insert("max_qubits".to_string(), serde_json::json!(32));
        let backend = DdsimBackend::from_config(config).unwrap();
        assert_eq!(backend.capabilities().num_qubits, 32);
    }

    #[test]
    fn test_ddsim_from_config_zero_qubits() {
        let mut config = BackendConfig::new("ddsim");
        config
            .extra
            .insert("max_qubits".to_string(), serde_json::json!(0));
        assert!(DdsimBackend::from_config(config).is_err());
    }

    #[tokio::test]
    async fn test_ddsim_validate_too_many_qubits() {
        let backend = DdsimBackend::with_max_qubits(5);
        let circuit = Circuit::with_size("test", 10, 0);
        let result = backend.validate(&circuit).await.unwrap();
        assert!(matches!(result, ValidationResult::Invalid { .. }));
    }

    #[tokio::test]
    async fn test_ddsim_submit_too_many_qubits() {
        let backend = DdsimBackend::with_max_qubits(5);
        let circuit = Circuit::with_size("test", 10, 0);
        let result = backend.submit(&circuit, 100, None).await;
        assert!(matches!(result, Err(HalError::CircuitTooLarge(_))));
    }

    /// Integration test: runs only when DDSIM is installed.
    ///
    /// Run with: `cargo test --package arvak-adapter-ddsim -- --ignored`
    #[tokio::test]
    #[ignore = "requires python3 + mqt.ddsim installed"]
    async fn test_ddsim_bell_state() {
        let backend = DdsimBackend::new();

        // Check availability first
        let avail = backend.availability().await.unwrap();
        if !avail.is_available {
            eprintln!("Skipping: DDSIM not available");
            return;
        }

        let circuit = Circuit::bell().unwrap();
        let job_id = backend.submit(&circuit, 1000, None).await.unwrap();

        let status = backend.status(&job_id).await.unwrap();
        assert!(status.is_success());

        let result = backend.result(&job_id).await.unwrap();
        assert_eq!(result.shots, 1000);

        // Bell state should produce only 00 and 11
        let counts = &result.counts;
        assert!(
            counts.get("00") + counts.get("11") == 1000,
            "Unexpected counts: {counts:?}"
        );
        assert!(counts.get("01") + counts.get("10") == 0);
    }

    /// Integration test: GHZ state.
    #[tokio::test]
    #[ignore = "requires python3 + mqt.ddsim installed"]
    async fn test_ddsim_ghz_state() {
        let backend = DdsimBackend::new();

        let avail = backend.availability().await.unwrap();
        if !avail.is_available {
            eprintln!("Skipping: DDSIM not available");
            return;
        }

        let circuit = Circuit::ghz(5).unwrap();
        let job_id = backend.submit(&circuit, 1000, None).await.unwrap();

        let result = backend.result(&job_id).await.unwrap();

        // GHZ(5) should produce only 00000 and 11111
        let counts = &result.counts;
        assert!(
            counts.get("00000") + counts.get("11111") == 1000,
            "Unexpected counts: {counts:?}"
        );
    }

    /// Cross-validation test: compare DDSIM against Arvak's own simulator.
    #[tokio::test]
    #[ignore = "requires python3 + mqt.ddsim installed"]
    async fn test_ddsim_cross_validate_bell() {
        let ddsim = DdsimBackend::new();

        let avail = ddsim.availability().await.unwrap();
        if !avail.is_available {
            eprintln!("Skipping: DDSIM not available");
            return;
        }

        let circuit = Circuit::bell().unwrap();

        // Run on DDSIM
        let ddsim_job = ddsim.submit(&circuit, 10_000, None).await.unwrap();
        let ddsim_result = ddsim.result(&ddsim_job).await.unwrap();

        // Both simulators should produce only 00 and 11 for Bell state
        let d_counts = &ddsim_result.counts;
        assert!(d_counts.get("00") + d_counts.get("11") == 10_000);

        // Probabilities should be close to 50/50
        let p00 = d_counts.get("00") as f64 / 10_000.0;
        assert!((p00 - 0.5).abs() < 0.05, "P(00) = {p00}, expected ~0.5");
    }
}
