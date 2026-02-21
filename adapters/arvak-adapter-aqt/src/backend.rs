//! AQT backend implementation.

use std::f64::consts::PI;
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

use crate::api::{AqtClient, AqtOp, CircuitPayload, ResourceInfo, SubmitRequest};
use crate::error::{AqtError, AqtResult};

/// Default AQT workspace for offline simulators.
pub const DEFAULT_WORKSPACE: &str = "default";

/// Default AQT resource (offline noiseless simulator — any token works).
pub const DEFAULT_RESOURCE: &str = "offline_simulator_no_noise";

/// AQT API constraint: maximum qubits per circuit.
const AQT_MAX_QUBITS: u32 = 20;

/// AQT API constraint: maximum shots per circuit (1–2000).
const AQT_MAX_SHOTS: u32 = 2000;

/// AQT API constraint: maximum gate operations per circuit (1–2000).
const AQT_MAX_OPS: usize = 2000;

/// Maximum number of cached job entries before evicting terminal-state entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// TTL for cached resource info before re-fetching from the API.
const RESOURCE_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// Cached job entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// AQT ion-trap quantum computer backend.
///
/// Supports AQT's cloud-hosted and offline simulators as well as the IBEX Q1
/// hardware device.  All AQT resources have all-to-all qubit connectivity;
/// no routing is required.
///
/// # Authentication
///
/// Set `AQT_TOKEN` in the environment.  A real AQT account token is required
/// for all resources — the Arnica cloud API validates tokens even for offline
/// simulators.  Request an account at <https://arnica.aqt.eu>.
///
/// # Example
///
/// ```ignore
/// use arvak_adapter_aqt::AqtBackend;
///
/// // Offline noiseless simulator — no token needed
/// let backend = AqtBackend::new()?;
///
/// // AQT cloud simulator — requires AQT_TOKEN
/// let backend = AqtBackend::with_resource("aqt_simulators", "simulator_noise")?;
/// ```
pub struct AqtBackend {
    /// REST API client.
    client: AqtClient,
    /// AQT workspace identifier.
    workspace: String,
    /// AQT resource (backend) identifier.
    resource: String,
    /// Instance-unique name: `"{workspace}/{resource}"`.
    name: String,
    /// Cached HAL capabilities.
    capabilities: Capabilities,
    /// Cached job metadata and results.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached resource info with TTL.
    resource_info: Arc<Mutex<Option<(ResourceInfo, Instant)>>>,
}

impl AqtBackend {
    /// Create a backend targeting the default offline noiseless simulator.
    ///
    /// Reads `AQT_TOKEN` from the environment (may be empty for offline sims).
    /// Reads `AQT_PORTAL_URL` to override the default base URL.
    pub fn new() -> AqtResult<Self> {
        Self::with_resource(DEFAULT_WORKSPACE, DEFAULT_RESOURCE)
    }

    /// Create a backend targeting a specific AQT workspace and resource.
    ///
    /// Reads `AQT_TOKEN` from the environment (may be empty for offline sims).
    /// Reads `AQT_PORTAL_URL` to override the default base URL.
    pub fn with_resource(
        workspace: impl Into<String>,
        resource: impl Into<String>,
    ) -> AqtResult<Self> {
        // AQT_TOKEN may be empty for offline simulators.
        let token = std::env::var("AQT_TOKEN").unwrap_or_default();

        let base_url =
            std::env::var("AQT_PORTAL_URL").unwrap_or_else(|_| crate::api::BASE_URL.to_string());

        let client = AqtClient::with_base_url(base_url, token)?;

        let workspace = workspace.into();
        let resource = resource.into();
        let name = format!("{workspace}/{resource}");
        let capabilities = build_capabilities(&resource, AQT_MAX_QUBITS);

        Ok(Self {
            client,
            workspace,
            resource,
            name,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            resource_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Create a backend with an explicit token (useful for testing).
    pub fn with_token(
        workspace: impl Into<String>,
        resource: impl Into<String>,
        token: impl Into<String>,
    ) -> AqtResult<Self> {
        let workspace = workspace.into();
        let resource = resource.into();
        let name = format!("{workspace}/{resource}");
        let client = AqtClient::new(token)?;
        let capabilities = build_capabilities(&resource, AQT_MAX_QUBITS);

        Ok(Self {
            client,
            workspace,
            resource,
            name,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            resource_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Return the workspace name.
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// Return the resource (backend) name.
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Fetch resource info from the API, using the cache if not stale.
    async fn fetch_resource_info(&self) -> AqtResult<ResourceInfo> {
        {
            let cache = self.resource_info.lock().await;
            if let Some((ref info, fetched_at)) = *cache {
                if fetched_at.elapsed() < RESOURCE_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        let info = self.client.get_resource(&self.resource).await?;

        {
            let mut cache = self.resource_info.lock().await;
            *cache = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Serialize a circuit to the AQT JSON gate format.
    ///
    /// # Angle conversion
    ///
    /// All AQT angles are in units of π (the API parameter equals radians ÷ π).
    ///
    /// # Gate mapping
    ///
    /// | Arvak IR gate        | AQT operation |
    /// |----------------------|---------------|
    /// | `Rz(angle_rad)`      | `RZ` with `phi = angle_rad / π` |
    /// | `PRX(theta_rad, phi_rad)` | `R` with `theta = theta_rad / π`, `phi = phi_rad / π rem_euclid 2.0` |
    /// | `RXX(angle_rad)`     | `RXX` with `theta = angle_rad / π` |
    ///
    /// Note: Arvak `PRX(θ, φ) = RZ(φ)·RX(θ)·RZ(-φ)` maps to AQT
    /// `R(θ_api, φ_api) = RZ(-φ_api·π)·RX(θ_api·π)·RZ(φ_api·π)`.
    /// These are the same gate under the substitution `φ_api = -φ_rad / π`,
    /// normalized to `[0, 2)` via `rem_euclid(2.0)`.
    fn serialize_circuit(circuit: &Circuit) -> AqtResult<Vec<AqtOp>> {
        let mut ops: Vec<AqtOp> = Vec::new();
        let mut has_measure = false;

        for (_, inst) in circuit.dag().topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let op = Self::gate_to_aqt_op(gate, &inst.qubits)?;
                    ops.push(op);
                }
                InstructionKind::Measure => {
                    has_measure = true;
                    // AQT uses a single terminal MEASURE — defer until end.
                }
                // Barriers are informational; skip them.
                InstructionKind::Barrier => {}
                _other => {
                    return Err(AqtError::UnsupportedGate(format!(
                        "{} (unsupported instruction kind)",
                        inst.name()
                    )));
                }
            }
        }

        // AQT requires exactly one terminal MEASURE for all qubits.
        // Always append it (either the circuit had measurements, or we add one).
        let _ = has_measure;
        ops.push(AqtOp::Measure);

        Ok(ops)
    }

    /// Convert a single Arvak IR gate to an AQT JSON operation.
    fn gate_to_aqt_op(
        gate: &arvak_ir::gate::Gate,
        qubits: &[arvak_ir::qubit::QubitId],
    ) -> AqtResult<AqtOp> {
        let qubit = qubits.first().map_or(0, |q| q.0);

        match &gate.kind {
            GateKind::Standard(sg) => match sg {
                StandardGate::Rz(angle_expr) => {
                    let angle_rad = angle_expr
                        .as_f64()
                        .ok_or_else(|| AqtError::SymbolicParameter("Rz".into()))?;
                    Ok(AqtOp::Rz {
                        qubit,
                        phi: angle_rad / PI,
                    })
                }

                StandardGate::PRX(theta_expr, phi_expr) => {
                    let theta_rad = theta_expr
                        .as_f64()
                        .ok_or_else(|| AqtError::SymbolicParameter("PRX theta".into()))?;
                    let phi_rad = phi_expr
                        .as_f64()
                        .ok_or_else(|| AqtError::SymbolicParameter("PRX phi".into()))?;
                    // Arvak PRX(θ, φ) = RZ(φ)·RX(θ)·RZ(-φ)
                    // AQT  R(θ, φ)   = RZ(-φπ)·RX(θπ)·RZ(φπ)
                    // Mapping: phi_api = -phi_rad/π, normalised to [0, 2)
                    let phi_api = (-phi_rad / PI).rem_euclid(2.0);
                    Ok(AqtOp::R {
                        qubit,
                        theta: theta_rad / PI,
                        phi: phi_api,
                    })
                }

                StandardGate::RXX(angle_expr) => {
                    let angle_rad = angle_expr
                        .as_f64()
                        .ok_or_else(|| AqtError::SymbolicParameter("RXX".into()))?;
                    let q0 = qubits.first().map_or(0, |q| q.0);
                    let q1 = qubits.get(1).map_or(1, |q| q.0);
                    Ok(AqtOp::Rxx {
                        qubits: [q0, q1],
                        theta: angle_rad / PI,
                    })
                }

                _ => Err(AqtError::UnsupportedGate(gate.name().to_string())),
            },

            GateKind::Custom(cg) => Err(AqtError::UnsupportedGate(cg.name.clone())),
        }
    }

    /// Aggregate AQT raw measurement samples into a `Counts` histogram.
    ///
    /// AQT returns `result["0"]` as a `[shots × n_qubits]` array.
    /// Each inner array is one shot; values are 0 or 1 per qubit.
    /// We concatenate each shot's bits into a bitstring (qubit 0 first)
    /// and count occurrences.
    fn parse_results(result_map: &std::collections::HashMap<String, Vec<Vec<u8>>>) -> Counts {
        let mut counts = Counts::new();

        // We only handle single-circuit submissions — use circuit index "0".
        let Some(shots) = result_map.get("0") else {
            return counts;
        };

        for shot in shots {
            let bitstring: String = shot
                .iter()
                .map(|&b| if b != 0 { '1' } else { '0' })
                .collect();
            counts.insert(bitstring, 1);
        }

        counts
    }
}

/// Build `Capabilities` for an AQT resource.
fn build_capabilities(resource: &str, num_qubits: u32) -> Capabilities {
    let is_simulator = resource.contains("simulator");
    Capabilities::aqt(resource, num_qubits).with_aqt_simulator(is_simulator)
}

#[async_trait]
impl Backend for AqtBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.fetch_resource_info().await {
            Ok(info) => {
                // Offline simulators don't report a status field; treat missing status as online.
                let online = info.status.is_none() || info.is_online();
                if online {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: None,
                    })
                } else {
                    let msg = info.status.unwrap_or_else(|| "offline".into());
                    Ok(BackendAvailability::unavailable(msg))
                }
            }
            Err(e) => {
                debug!("AQT availability check failed: {}", e);
                // Offline simulators may not expose the /resources endpoint publicly;
                // treat the error as "probably available" for offline_simulator resources.
                if self.resource.contains("offline_simulator") {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: Some("offline simulator (status check unavailable)".into()),
                    })
                } else {
                    Ok(BackendAvailability::unavailable(e.to_string()))
                }
            }
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        // Check qubit count.
        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits but AQT {} supports at most {}",
                circuit.num_qubits(),
                self.resource,
                caps.num_qubits
            ));
        }

        // Check gate set.
        let gate_set = &caps.gate_set;
        let mut op_count = 0usize;

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
                    op_count = op_count.saturating_add(1);
                }
                InstructionKind::Measure | InstructionKind::Barrier => {}
                _ => {
                    reasons.push(format!("Unsupported instruction: {}", inst.name()));
                    break;
                }
            }
        }

        // Check operation count (AQT limit: 2000 ops per circuit, excluding MEASURE).
        if op_count > AQT_MAX_OPS {
            reasons.push(format!(
                "Circuit has {op_count} gates but AQT allows at most {AQT_MAX_OPS}"
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
            "Submitting circuit to AQT {}/{}: {} qubits, {} shots",
            self.workspace,
            self.resource,
            circuit.num_qubits(),
            shots
        );

        let caps = self.capabilities();

        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but AQT {} supports at most {}",
                circuit.num_qubits(),
                self.resource,
                caps.num_qubits
            )));
        }

        if shots > AQT_MAX_SHOTS {
            return Err(HalError::InvalidShots(format!(
                "Requested {shots} shots but AQT maximum is {AQT_MAX_SHOTS}"
            )));
        }

        if shots == 0 {
            return Err(HalError::InvalidShots(
                "Shot count must be at least 1".into(),
            ));
        }

        let n_qubits = u32::try_from(circuit.num_qubits()).unwrap_or(AQT_MAX_QUBITS);

        let ops = Self::serialize_circuit(circuit).map_err(|e| HalError::Backend(e.to_string()))?;

        if ops.len() > AQT_MAX_OPS + 1 {
            // +1 for the terminal MEASURE
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} operations but AQT allows at most {}",
                ops.len() - 1,
                AQT_MAX_OPS
            )));
        }

        let circuit_payload = CircuitPayload {
            repetitions: shots,
            number_of_qubits: n_qubits,
            quantum_circuit: ops,
        };
        let req = SubmitRequest::new(vec![circuit_payload]);

        let response = self
            .client
            .submit_circuit(&self.workspace, &self.resource, &req)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.job.job_id);
        info!("AQT job submitted: {}", job_id);

        let job = Job::new(job_id.clone(), shots).with_backend(&self.resource);
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
        let response = self
            .client
            .get_result(&job_id.0)
            .await
            .map_err(|e| match e {
                AqtError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        let status = if response.response.is_finished() {
            JobStatus::Completed
        } else if response.response.is_error() {
            let msg = response.response.message.unwrap_or_default();
            JobStatus::Failed(msg)
        } else if response.response.is_cancelled() {
            JobStatus::Cancelled
        } else if response.response.status.to_lowercase() == "ongoing" {
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

        let response = self
            .client
            .get_result(&job_id.0)
            .await
            .map_err(|e| match e {
                AqtError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        if let Some(ref error_msg) = response.response.message {
            if response.response.is_error() {
                return Err(HalError::JobFailed(error_msg.clone()));
            }
        }

        if !response.response.is_finished() {
            return Err(HalError::Backend(format!(
                "Job {} is not yet completed (status: {})",
                job_id, response.response.status
            )));
        }

        let api_results = response.response.result.ok_or_else(|| {
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
                AqtError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
                _ => HalError::Backend(e.to_string()),
            })?;

        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.job = cached.job.clone().with_status(JobStatus::Cancelled);
            }
        }

        info!("AQT job cancelled: {}", job_id);
        Ok(())
    }
}

impl BackendFactory for AqtBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        let workspace = config
            .extra
            .get("workspace")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_WORKSPACE)
            .to_string();

        let resource = config
            .extra
            .get("resource")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_RESOURCE)
            .to_string();

        Self::with_resource(workspace, resource).map_err(|e| HalError::Backend(e.to_string()))
    }
}

/// Extension trait to set the `is_simulator` flag on an AQT `Capabilities`.
trait CapabilitiesAqtExt {
    fn with_aqt_simulator(self, is_simulator: bool) -> Self;
}

impl CapabilitiesAqtExt for Capabilities {
    fn with_aqt_simulator(mut self, is_simulator: bool) -> Self {
        self.is_simulator = is_simulator;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_parse_results_bell_state() {
        let mut result_map = HashMap::new();
        // 4 shots of a 2-qubit Bell state: |00⟩, |11⟩, |00⟩, |11⟩
        result_map.insert(
            "0".to_string(),
            vec![vec![0, 0], vec![1, 1], vec![0, 0], vec![1, 1]],
        );
        let counts = AqtBackend::parse_results(&result_map);
        let sorted = counts.sorted();
        let map: HashMap<String, u64> = sorted.into_iter().map(|(k, v)| (k.clone(), *v)).collect();
        assert_eq!(map["00"], 2);
        assert_eq!(map["11"], 2);
    }

    #[test]
    fn test_parse_results_empty() {
        let result_map = HashMap::new();
        let counts = AqtBackend::parse_results(&result_map);
        assert_eq!(counts.total_shots(), 0);
    }

    #[test]
    fn test_parse_results_single_qubit() {
        let mut result_map = HashMap::new();
        // 3 shots: |0⟩, |1⟩, |0⟩
        result_map.insert("0".to_string(), vec![vec![0], vec![1], vec![0]]);
        let counts = AqtBackend::parse_results(&result_map);
        assert_eq!(counts.total_shots(), 3);
    }

    #[test]
    fn test_gate_to_aqt_op_rz() {
        use arvak_ir::{gate::Gate, parameter::ParameterExpression, qubit::QubitId};

        let angle = ParameterExpression::Constant(PI / 2.0);
        let gate = Gate::standard(StandardGate::Rz(angle));
        let qubits = vec![QubitId(0)];

        let op = AqtBackend::gate_to_aqt_op(&gate, &qubits).unwrap();
        match op {
            AqtOp::Rz { qubit, phi } => {
                assert_eq!(qubit, 0);
                assert!((phi - 0.5).abs() < 1e-10);
            }
            _ => panic!("Expected Rz op"),
        }
    }

    #[test]
    fn test_gate_to_aqt_op_rxx() {
        use arvak_ir::{gate::Gate, parameter::ParameterExpression, qubit::QubitId};

        let angle = ParameterExpression::Constant(PI / 4.0);
        let gate = Gate::standard(StandardGate::RXX(angle));
        let qubits = vec![QubitId(0), QubitId(1)];

        let op = AqtBackend::gate_to_aqt_op(&gate, &qubits).unwrap();
        match op {
            AqtOp::Rxx {
                qubits: [q0, q1],
                theta,
            } => {
                assert_eq!(q0, 0);
                assert_eq!(q1, 1);
                assert!((theta - 0.25).abs() < 1e-10);
            }
            _ => panic!("Expected Rxx op"),
        }
    }

    #[test]
    fn test_gate_to_aqt_op_unsupported() {
        use arvak_ir::{gate::Gate, qubit::QubitId};

        let gate = Gate::standard(StandardGate::H);
        let qubits = vec![QubitId(0)];

        let result = AqtBackend::gate_to_aqt_op(&gate, &qubits);
        assert!(matches!(result, Err(AqtError::UnsupportedGate(_))));
    }

    #[test]
    fn test_build_capabilities_offline_simulator() {
        let caps = build_capabilities("offline_simulator_no_noise", 20);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 20);
    }

    #[test]
    fn test_build_capabilities_hardware() {
        let caps = build_capabilities("ibex", 12);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 12);
    }
}
