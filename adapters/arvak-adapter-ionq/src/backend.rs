//! IonQ backend implementation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument};

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, ValidationResult,
};
use arvak_ir::{
    Circuit,
    gate::{GateKind, StandardGate},
    instruction::InstructionKind,
};

use crate::api::{BackendInfo, IonQClient, IonQGate, JobInput, JobRequest};
use crate::error::{IonQError, IonQResult};

/// IonQ simulator backend name.
pub const SIMULATOR: &str = "simulator";

/// Simulator qubit limit.
const SIMULATOR_MAX_QUBITS: u32 = 29;

/// QPU qubit limit (Aria/Forte — 25 algorithmic qubits).
const QPU_MAX_QUBITS: u32 = 25;

/// Maximum number of cached job entries before evicting terminal-state entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// TTL for cached backend info before re-fetching from the API.
const BACKEND_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// Cached job entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// IonQ trapped-ion quantum computer backend.
///
/// Supports IonQ's cloud simulator (29 qubits, free) and QPU hardware
/// (Aria 25q, Forte 36q).  All IonQ devices have all-to-all qubit
/// connectivity; no routing is required.
///
/// Circuits are submitted using the QIS gateset (standard gates like
/// H, CX, RX, RY, RZ).  IonQ compiles these to native gates (GPI, GPI2, MS)
/// on the server side.
///
/// # Authentication
///
/// Set `IONQ_API_KEY` in the environment.  Get a free API key at
/// <https://cloud.ionq.com>.
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_ionq::IonQBackend;
///
/// // Free simulator — requires IONQ_API_KEY
/// let backend = IonQBackend::new()?;
///
/// // QPU hardware
/// let backend = IonQBackend::with_backend("qpu.aria-1")?;
/// ```
pub struct IonQBackend {
    /// REST API client.
    client: IonQClient,
    /// Target backend name (e.g., "simulator", "qpu.aria-1").
    backend_name: String,
    /// Instance-unique name.
    name: String,
    /// Cached HAL capabilities.
    capabilities: Capabilities,
    /// Cached job metadata and results.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached backend info with TTL.
    backend_info: Arc<Mutex<Option<(BackendInfo, Instant)>>>,
}

impl IonQBackend {
    /// Create a backend targeting the IonQ simulator (29 qubits, free tier).
    ///
    /// Reads `IONQ_API_KEY` from the environment.
    pub fn new() -> IonQResult<Self> {
        Self::with_backend(SIMULATOR)
    }

    /// Create a backend targeting a specific IonQ backend.
    ///
    /// Reads `IONQ_API_KEY` from the environment.
    /// Reads `IONQ_API_URL` to override the default base URL.
    pub fn with_backend(backend_name: impl Into<String>) -> IonQResult<Self> {
        let api_key = std::env::var("IONQ_API_KEY").map_err(|_| IonQError::MissingApiKey)?;
        let base_url =
            std::env::var("IONQ_API_URL").unwrap_or_else(|_| crate::api::BASE_URL.to_string());

        let client = IonQClient::with_base_url(base_url, api_key)?;
        let backend_name = backend_name.into();
        let name = format!("ionq_{}", backend_name.replace('.', "_"));

        let max_qubits = if backend_name == SIMULATOR {
            SIMULATOR_MAX_QUBITS
        } else {
            QPU_MAX_QUBITS
        };
        let capabilities = build_capabilities(&backend_name, max_qubits);

        Ok(Self {
            client,
            backend_name,
            name,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            backend_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a backend with an explicit API key (useful for testing).
    pub fn with_api_key(
        backend_name: impl Into<String>,
        api_key: impl Into<String>,
    ) -> IonQResult<Self> {
        let client = IonQClient::new(api_key)?;
        let backend_name = backend_name.into();
        let name = format!("ionq_{}", backend_name.replace('.', "_"));

        let max_qubits = if backend_name == SIMULATOR {
            SIMULATOR_MAX_QUBITS
        } else {
            QPU_MAX_QUBITS
        };
        let capabilities = build_capabilities(&backend_name, max_qubits);

        Ok(Self {
            client,
            backend_name,
            name,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            backend_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Return the IonQ backend name (e.g., "simulator", "qpu.aria-1").
    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }

    /// Fetch backend info from the API, using the cache if not stale.
    async fn fetch_backend_info(&self) -> IonQResult<BackendInfo> {
        {
            let cache = self.backend_info.lock().await;
            if let Some((ref info, fetched_at)) = *cache {
                if fetched_at.elapsed() < BACKEND_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        let info = self.client.get_backend(&self.backend_name).await?;

        {
            let mut cache = self.backend_info.lock().await;
            *cache = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Serialize a circuit to IonQ's QIS JSON gate format.
    ///
    /// IonQ QIS gateset accepts standard gates (h, cx, rx, ry, rz, etc.)
    /// with rotation angles in radians.
    fn serialize_circuit(circuit: &Circuit) -> IonQResult<Vec<IonQGate>> {
        let mut gates: Vec<IonQGate> = Vec::new();

        for (_, inst) in circuit.dag().topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let ionq_gate = Self::gate_to_ionq(gate, &inst.qubits)?;
                    gates.push(ionq_gate);
                }
                // IonQ measures all qubits automatically at the end — skip explicit measures.
                InstructionKind::Measure => {}
                InstructionKind::Barrier => {}
                _other => {
                    return Err(IonQError::UnsupportedGate(format!(
                        "{} (unsupported instruction kind)",
                        inst.name()
                    )));
                }
            }
        }

        Ok(gates)
    }

    /// Convert a single Arvak IR gate to an IonQ QIS gate.
    fn gate_to_ionq(
        gate: &arvak_ir::gate::Gate,
        qubits: &[arvak_ir::qubit::QubitId],
    ) -> IonQResult<IonQGate> {
        let q0 = qubits.first().map_or(0, |q| q.0);

        match &gate.kind {
            GateKind::Standard(sg) => Self::standard_gate_to_ionq(sg, qubits, q0),
            GateKind::Custom(cg) => Err(IonQError::UnsupportedGate(cg.name.clone())),
        }
    }

    fn standard_gate_to_ionq(
        sg: &StandardGate,
        qubits: &[arvak_ir::qubit::QubitId],
        q0: u32,
    ) -> IonQResult<IonQGate> {
        match sg {
            // Single-qubit gates without parameters
            StandardGate::H => Ok(single("h", q0)),
            StandardGate::X => Ok(single("x", q0)),
            StandardGate::Y => Ok(single("y", q0)),
            StandardGate::Z => Ok(single("z", q0)),
            StandardGate::S => Ok(single("s", q0)),
            StandardGate::Sdg => Ok(single("si", q0)),
            StandardGate::T => Ok(single("t", q0)),
            StandardGate::Tdg => Ok(single("ti", q0)),
            StandardGate::SX => Ok(single("v", q0)),

            // Single-qubit rotation gates (angles in radians)
            StandardGate::Rx(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("Rx".into()))?;
                Ok(single_rotation("rx", q0, rad))
            }
            StandardGate::Ry(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("Ry".into()))?;
                Ok(single_rotation("ry", q0, rad))
            }
            StandardGate::Rz(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("Rz".into()))?;
                Ok(single_rotation("rz", q0, rad))
            }

            // Two-qubit gates
            StandardGate::CX => {
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(controlled("cx", q0, q1))
            }
            StandardGate::CZ => {
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(controlled("cz", q0, q1))
            }
            StandardGate::Swap => {
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(two_target("swap", q0, q1))
            }
            StandardGate::RXX(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("RXX".into()))?;
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(IonQGate {
                    gate: "xx".into(),
                    target: None,
                    targets: Some(vec![q0, q1]),
                    control: None,
                    controls: None,
                    rotation: Some(rad),
                })
            }
            StandardGate::RYY(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("RYY".into()))?;
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(IonQGate {
                    gate: "yy".into(),
                    target: None,
                    targets: Some(vec![q0, q1]),
                    control: None,
                    controls: None,
                    rotation: Some(rad),
                })
            }
            StandardGate::RZZ(angle) => {
                let rad = angle
                    .as_f64()
                    .ok_or_else(|| IonQError::SymbolicParameter("RZZ".into()))?;
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                Ok(IonQGate {
                    gate: "zz".into(),
                    target: None,
                    targets: Some(vec![q0, q1]),
                    control: None,
                    controls: None,
                    rotation: Some(rad),
                })
            }

            // Three-qubit gates
            StandardGate::CCX => {
                let q1 = qubits.get(1).map_or(1, |q| q.0);
                let q2 = qubits.get(2).map_or(2, |q| q.0);
                Ok(IonQGate {
                    gate: "cx".into(),
                    target: None,
                    targets: Some(vec![q2]),
                    control: None,
                    controls: Some(vec![q0, q1]),
                    rotation: None,
                })
            }

            _ => Err(IonQError::UnsupportedGate(gate_name_str(sg))),
        }
    }

    /// Convert IonQ probability distribution to `Counts`.
    ///
    /// IonQ returns probabilities keyed by decimal state index (e.g., "0", "3").
    /// We convert each key to a binary bitstring and scale by shot count.
    fn probabilities_to_counts(
        probabilities: &std::collections::HashMap<String, f64>,
        n_qubits: u32,
        shots: u32,
    ) -> Counts {
        let mut counts = Counts::new();

        for (state_str, &prob) in probabilities {
            let state_idx: u64 = state_str.parse().unwrap_or(0);
            let bitstring = format!("{:0>width$b}", state_idx, width = n_qubits as usize);
            let count = (prob * f64::from(shots)).round() as u64;
            if count > 0 {
                counts.insert(bitstring, count);
            }
        }

        counts
    }

    /// Convert IonQ histogram (sampled counts as fractions) to `Counts`.
    fn histogram_to_counts(
        histogram: &std::collections::HashMap<String, f64>,
        n_qubits: u32,
        shots: u32,
    ) -> Counts {
        let mut counts = Counts::new();

        for (state_str, &frac) in histogram {
            let state_idx: u64 = state_str.parse().unwrap_or(0);
            let bitstring = format!("{:0>width$b}", state_idx, width = n_qubits as usize);
            let count = (frac * f64::from(shots)).round() as u64;
            if count > 0 {
                counts.insert(bitstring, count);
            }
        }

        counts
    }
}

/// Build `Capabilities` for an IonQ backend.
fn build_capabilities(backend_name: &str, num_qubits: u32) -> Capabilities {
    let mut caps = Capabilities::ionq(backend_name, num_qubits);
    if backend_name == SIMULATOR {
        caps.is_simulator = true;
    }
    caps
}

// ---------------------------------------------------------------------------
// Gate construction helpers
// ---------------------------------------------------------------------------

fn single(name: &str, target: u32) -> IonQGate {
    IonQGate {
        gate: name.into(),
        target: Some(target),
        targets: None,
        control: None,
        controls: None,
        rotation: None,
    }
}

fn single_rotation(name: &str, target: u32, rotation: f64) -> IonQGate {
    IonQGate {
        gate: name.into(),
        target: Some(target),
        targets: None,
        control: None,
        controls: None,
        rotation: Some(rotation),
    }
}

fn controlled(name: &str, control: u32, target: u32) -> IonQGate {
    IonQGate {
        gate: name.into(),
        target: Some(target),
        targets: None,
        control: Some(control),
        controls: None,
        rotation: None,
    }
}

fn two_target(name: &str, q0: u32, q1: u32) -> IonQGate {
    IonQGate {
        gate: name.into(),
        target: None,
        targets: Some(vec![q0, q1]),
        control: None,
        controls: None,
        rotation: None,
    }
}

fn gate_name_str(sg: &StandardGate) -> String {
    format!("{sg:?}")
}

#[async_trait]
impl Backend for IonQBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.fetch_backend_info().await {
            Ok(info) => {
                if info.is_available() {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: Some(format!("status: {}", info.status)),
                    })
                } else {
                    Ok(BackendAvailability::unavailable(format!(
                        "IonQ {} is {}",
                        self.backend_name, info.status
                    )))
                }
            }
            Err(e) => {
                debug!("IonQ availability check failed: {}", e);
                Ok(BackendAvailability::unavailable(e.to_string()))
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        // Check qubit count.
        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but IonQ {} supports at most {}",
                circuit.num_qubits(),
                self.backend_name,
                caps.num_qubits
            ));
        }

        // Check gate set.
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let name = gate.name();
                    if !gate_set.contains(name) {
                        reasons.push(format!("Unsupported gate: {name}"));
                        break;
                    }
                    // Check for unbound symbolic parameters.
                    if let GateKind::Standard(sg) = &gate.kind {
                        for param in sg.parameters() {
                            if param.is_symbolic() {
                                reasons
                                    .push(format!("Gate '{name}' has unbound symbolic parameter"));
                                break;
                            }
                        }
                    }
                }
                InstructionKind::Measure | InstructionKind::Barrier => {}
                _ => {
                    reasons.push(format!("Unsupported instruction: {}", inst.name()));
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
                "IonQ backend does not support runtime parameter binding".into(),
            ));
        }

        info!(
            "Submitting circuit to IonQ {}: {} qubits, {} shots",
            self.backend_name,
            circuit.num_qubits(),
            shots
        );

        let caps = self.capabilities();

        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but IonQ {} supports at most {}",
                circuit.num_qubits(),
                self.backend_name,
                caps.num_qubits
            )));
        }

        if shots == 0 {
            return Err(HalError::InvalidShots(
                "Shot count must be at least 1".into(),
            ));
        }

        let n_qubits = u32::try_from(circuit.num_qubits()).unwrap_or(caps.num_qubits);

        let gates =
            Self::serialize_circuit(circuit).map_err(|e| HalError::Backend(e.to_string()))?;

        let req = JobRequest {
            job_type: "ionq.circuit.v1".into(),
            backend: self.backend_name.clone(),
            shots,
            name: Some(format!("arvak-{}", &self.backend_name)),
            input: JobInput {
                gateset: "qis".into(),
                qubits: n_qubits,
                circuit: gates,
            },
        };

        let response = self
            .client
            .submit_job(&req)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.id);
        info!("IonQ job submitted: {}", job_id);

        let job = Job::new(job_id.clone(), shots).with_backend(&self.backend_name);
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
            IonQError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        let status = match response.status.as_str() {
            "completed" => JobStatus::Completed,
            "failed" => {
                let msg = response
                    .error
                    .and_then(|e| e.message)
                    .unwrap_or_else(|| "unknown error".into());
                JobStatus::Failed(msg)
            }
            "canceled" => JobStatus::Cancelled,
            "running" | "ready" => JobStatus::Running,
            "submitted" => JobStatus::Queued,
            _ => JobStatus::Queued,
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
            IonQError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        if response.status == "failed" {
            let msg = response
                .error
                .and_then(|e| e.message)
                .unwrap_or_else(|| "unknown error".into());
            return Err(HalError::JobFailed(msg));
        }

        if response.status != "completed" {
            return Err(HalError::Backend(format!(
                "Job {} is not yet completed (status: {})",
                job_id, response.status
            )));
        }

        let results = response
            .results
            .ok_or_else(|| HalError::JobFailed("Completed job returned no results".into()))?;

        let n_qubits = response.qubits.unwrap_or(1);
        let shots = response.shots.unwrap_or(100);

        // IonQ returns either probabilities or histogram depending on backend.
        let counts = if let Some(ref histogram) = results.histogram {
            Self::histogram_to_counts(histogram, n_qubits, shots)
        } else if let Some(ref probabilities) = results.probabilities {
            Self::probabilities_to_counts(probabilities, n_qubits, shots)
        } else {
            return Err(HalError::JobFailed(
                "Completed job has no probabilities or histogram".into(),
            ));
        };

        let actual_shots = u32::try_from(counts.total_shots()).unwrap_or(shots);
        let result = ExecutionResult::new(counts, actual_shots);

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
                IonQError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(JobStatus::Cancelled);
            }
        }

        info!("IonQ job cancelled: {}", job_id);
        Ok(())
    }
}

impl BackendFactory for IonQBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let backend_name = config
            .extra
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or(SIMULATOR)
            .to_string();

        Self::with_backend(backend_name).map_err(|e| HalError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_probabilities_to_counts_bell_state() {
        let mut probs = HashMap::new();
        probs.insert("0".to_string(), 0.5);
        probs.insert("3".to_string(), 0.5);

        let counts = IonQBackend::probabilities_to_counts(&probs, 2, 1000);
        let sorted = counts.sorted();
        let map: HashMap<String, u64> = sorted.into_iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(map["00"], 500);
        assert_eq!(map["11"], 500);
    }

    #[test]
    fn test_probabilities_to_counts_single_qubit() {
        let mut probs = HashMap::new();
        probs.insert("1".to_string(), 1.0);

        let counts = IonQBackend::probabilities_to_counts(&probs, 1, 100);
        let sorted = counts.sorted();
        let map: HashMap<String, u64> = sorted.into_iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(map["1"], 100);
    }

    #[test]
    fn test_probabilities_to_counts_empty() {
        let probs = HashMap::new();
        let counts = IonQBackend::probabilities_to_counts(&probs, 2, 100);
        assert_eq!(counts.total_shots(), 0);
    }

    #[test]
    fn test_histogram_to_counts() {
        let mut hist = HashMap::new();
        hist.insert("0".to_string(), 0.48);
        hist.insert("3".to_string(), 0.52);

        let counts = IonQBackend::histogram_to_counts(&hist, 2, 100);
        let sorted = counts.sorted();
        let map: HashMap<String, u64> = sorted.into_iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(map["00"], 48);
        assert_eq!(map["11"], 52);
    }

    #[test]
    fn test_gate_to_ionq_h() {
        use arvak_ir::{gate::Gate, qubit::QubitId};

        let gate = Gate::standard(StandardGate::H);
        let qubits = vec![QubitId(0)];
        let ionq = IonQBackend::gate_to_ionq(&gate, &qubits).unwrap();
        assert_eq!(ionq.gate, "h");
        assert_eq!(ionq.target, Some(0));
        assert!(ionq.rotation.is_none());
    }

    #[test]
    fn test_gate_to_ionq_cx() {
        use arvak_ir::{gate::Gate, qubit::QubitId};

        let gate = Gate::standard(StandardGate::CX);
        let qubits = vec![QubitId(0), QubitId(1)];
        let ionq = IonQBackend::gate_to_ionq(&gate, &qubits).unwrap();
        assert_eq!(ionq.gate, "cx");
        assert_eq!(ionq.control, Some(0));
        assert_eq!(ionq.target, Some(1));
    }

    #[test]
    fn test_gate_to_ionq_rx() {
        use arvak_ir::{gate::Gate, parameter::ParameterExpression, qubit::QubitId};

        let angle = ParameterExpression::Constant(std::f64::consts::FRAC_PI_2);
        let gate = Gate::standard(StandardGate::Rx(angle));
        let qubits = vec![QubitId(0)];
        let ionq = IonQBackend::gate_to_ionq(&gate, &qubits).unwrap();
        assert_eq!(ionq.gate, "rx");
        assert_eq!(ionq.target, Some(0));
        assert!((ionq.rotation.unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
    }

    #[test]
    fn test_gate_to_ionq_swap() {
        use arvak_ir::{gate::Gate, qubit::QubitId};

        let gate = Gate::standard(StandardGate::Swap);
        let qubits = vec![QubitId(0), QubitId(1)];
        let ionq = IonQBackend::gate_to_ionq(&gate, &qubits).unwrap();
        assert_eq!(ionq.gate, "swap");
        assert_eq!(ionq.targets, Some(vec![0, 1]));
        assert!(ionq.target.is_none());
    }

    #[test]
    fn test_gate_to_ionq_ccx() {
        use arvak_ir::{gate::Gate, qubit::QubitId};

        let gate = Gate::standard(StandardGate::CCX);
        let qubits = vec![QubitId(0), QubitId(1), QubitId(2)];
        let ionq = IonQBackend::gate_to_ionq(&gate, &qubits).unwrap();
        assert_eq!(ionq.gate, "cx");
        assert_eq!(ionq.controls, Some(vec![0, 1]));
        assert_eq!(ionq.targets, Some(vec![2]));
    }

    #[test]
    fn test_gate_to_ionq_unsupported() {
        use arvak_ir::{gate::Gate, parameter::ParameterExpression, qubit::QubitId};

        // PRX is not in IonQ QIS gateset
        let gate = Gate::standard(StandardGate::PRX(
            ParameterExpression::Constant(1.0),
            ParameterExpression::Constant(0.0),
        ));
        let qubits = vec![QubitId(0)];
        let result = IonQBackend::gate_to_ionq(&gate, &qubits);
        assert!(matches!(result, Err(IonQError::UnsupportedGate(_))));
    }

    #[test]
    fn test_build_capabilities_simulator() {
        let caps = build_capabilities("simulator", 29);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 29);
    }

    #[test]
    fn test_build_capabilities_qpu() {
        let caps = build_capabilities("qpu.aria-1", 25);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 25);
    }
}
