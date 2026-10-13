//! IBM Quantum backend implementation.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, Capabilities, Counts, ExecutionResult, GateSet,
    HalError, HalResult, JobId, JobStatus, Topology, TopologyKind, ValidationResult,
};
use arvak_ir::Circuit;
use arvak_qasm3::emit;
use async_trait::async_trait;

use crate::api::{BackendInfo, IbmClient, LEGACY_ENDPOINT};
use crate::error::{IbmError, IbmResult};

/// Default IBM Quantum backend (Heron processor, zero-queue).
const DEFAULT_BACKEND: &str = "ibm_torino";

/// Maximum number of job IDs to keep in the shots cache.
/// Terminal-state jobs are evicted first; oldest entry evicted as fallback.
const MAX_SHOTS_CACHE: usize = 10_000;

/// Eagle backends (127-qubit, ECR native).
const EAGLE_BACKENDS: &[&str] = &[
    "ibm_brussels",
    "ibm_strasbourg",
    "ibm_brisbane",
    "ibm_kyoto",
    "ibm_osaka",
    "ibm_sherbrooke",
    "ibm_nazca",
];

/// Return the correct gate set for a named IBM backend.
///
/// Uses the target name to distinguish Eagle (ECR) from Heron (CZ).
/// Falls back to Heron for unknown targets (safe: Heron is the current
/// generation and most new backends are Heron).
fn gate_set_for_target(target: &str) -> GateSet {
    if EAGLE_BACKENDS.contains(&target) {
        GateSet::ibm_eagle()
    } else {
        GateSet::ibm_heron()
    }
}

/// Build a `Capabilities` stub for a named IBM backend.
///
/// The topology is a placeholder (`linear`) — callers that need accurate
/// topology must use `IbmBackend::connect()` which fetches it from the API.
fn capabilities_stub(target: &str, num_qubits: u32) -> Capabilities {
    Capabilities {
        name: target.to_string(),
        num_qubits,
        gate_set: gate_set_for_target(target),
        topology: Topology::linear(num_qubits), // placeholder
        max_shots: 100_000,
        max_circuit_ops: None,
        is_simulator: false,
        features: vec!["dynamic_circuits".into()],
        noise_profile: None,
    }
}

/// Processor families laid out on IBM's heavy-hex lattice.
const HEAVY_HEX_FAMILIES: &[&str] = &["Falcon", "Hummingbird", "Eagle", "Heron"];

/// Return the gate set for a backend whose configuration was fetched from the API.
///
/// The backend's own `basis_gates` decide: ECR → Eagle, CZ → Heron-style CZ set
/// (also Nighthawk). Only if the API reports neither do we fall back to the
/// name list. Qubit count is deliberately NOT used — Nighthawk has 120 qubits
/// but is CZ-native, so a `<= 127 → Eagle` rule misclassifies it.
fn gate_set_for_backend(target: &str, basis_gates: &[String]) -> GateSet {
    if basis_gates.iter().any(|g| g == "ecr") {
        GateSet::ibm_eagle()
    } else if basis_gates.iter().any(|g| g == "cz") {
        GateSet::ibm_heron()
    } else {
        gate_set_for_target(target)
    }
}

/// Build the topology from the API coupling map.
///
/// Only known heavy-hex families are labelled `HeavyHex`; anything else
/// (e.g. Nighthawk's square lattice) is `Custom`. Routing uses the edge list
/// either way, so the label never changes compilation — it must not lie, though.
fn topology_for_backend(info: &BackendInfo) -> Topology {
    let edges = info
        .coupling_map
        .iter()
        .map(|&[a, b]| (u32::try_from(a).unwrap_or(0), u32::try_from(b).unwrap_or(0)))
        .collect();
    let is_heavy_hex = info
        .processor_type
        .as_ref()
        .is_some_and(|p| HEAVY_HEX_FAMILIES.contains(&p.family.as_str()));
    Topology {
        kind: if is_heavy_hex {
            TopologyKind::HeavyHex
        } else {
            TopologyKind::Custom
        },
        edges,
    }
}

/// IBM runs native `rzz` (fractional gate) only for `0 < θ ≤ π/2` and fails the
/// whole job otherwise. Arvak's Heron translation folds angles into that range;
/// this catches uncompiled or symbolic `rzz` before submission.
fn rzz_angle_violation(gate: &arvak_ir::Gate) -> Option<String> {
    let arvak_ir::GateKind::Standard(arvak_ir::StandardGate::RZZ(theta)) = &gate.kind else {
        return None;
    };
    match theta.as_f64() {
        Some(t) if t > 0.0 && t <= std::f64::consts::FRAC_PI_2 => None,
        Some(t) => Some(format!(
            "rzz({t}) outside IBM's native range (0, π/2]; compile for the Heron basis to fold it"
        )),
        None => Some(
            "rzz with unbound parameter: IBM requires 0 < θ ≤ π/2, bind parameters and compile first"
                .to_string(),
        ),
    }
}

/// Return a `Capabilities` built from the backend configuration fetched from the API.
fn capabilities_from_real_info(target: &str, info: &BackendInfo) -> Capabilities {
    let num_qubits = u32::try_from(info.num_qubits).unwrap_or(133);
    Capabilities {
        name: target.to_string(),
        num_qubits,
        gate_set: gate_set_for_backend(target, &info.basis_gates),
        topology: topology_for_backend(info),
        max_shots: 100_000,
        max_circuit_ops: None,
        is_simulator: false,
        features: vec!["dynamic_circuits".into()],
        noise_profile: None,
    }
}

/// How long to cache backend info before refreshing from the API.
const BACKEND_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// IBM Quantum backend adapter.
pub struct IbmBackend {
    /// API client.
    client: Arc<IbmClient>,
    /// Target backend name.
    target: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Cached backend info with fetch timestamp for TTL-based refresh.
    backend_info: Arc<RwLock<Option<(BackendInfo, Instant)>>>,
    /// Whether to tell IBM to skip its own transpilation.
    skip_transpilation: bool,
    /// Submitted shot counts keyed by job ID.
    /// Used to correctly convert quasi-probability distributions to counts.
    shots_cache: Arc<Mutex<HashMap<String, u32>>>,
}

impl IbmBackend {
    /// Create a new IBM backend with default settings (legacy token mode).
    ///
    /// Reads the API token from the `IBM_QUANTUM_TOKEN` environment variable.
    /// For the new IBM Cloud API, use [`IbmBackend::connect`] instead.
    pub fn new() -> IbmResult<Self> {
        let token = std::env::var("IBM_QUANTUM_TOKEN").map_err(|_| IbmError::MissingToken)?;

        let client = IbmClient::new(LEGACY_ENDPOINT, &token)?;
        let target = DEFAULT_BACKEND.to_string();

        Ok(Self {
            client: Arc::new(client),
            capabilities: capabilities_stub(&target, 127),
            target,
            backend_info: Arc::new(RwLock::new(None)),
            skip_transpilation: false,
            shots_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Create a backend targeting a specific IBM Quantum device (legacy token mode).
    ///
    /// For the new IBM Cloud API, use [`IbmBackend::connect`] instead.
    pub fn with_target(target: impl Into<String>) -> IbmResult<Self> {
        let token = std::env::var("IBM_QUANTUM_TOKEN").map_err(|_| IbmError::MissingToken)?;

        let client = IbmClient::new(LEGACY_ENDPOINT, &token)?;
        let target = target.into();

        Ok(Self {
            client: Arc::new(client),
            capabilities: capabilities_stub(&target, 127),
            target,
            backend_info: Arc::new(RwLock::new(None)),
            skip_transpilation: false,
            shots_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Connect to an IBM Quantum backend using the new Cloud API.
    ///
    /// Reads `IBM_API_KEY` and `IBM_SERVICE_CRN` from environment. If `IBM_API_KEY`
    /// is not set, falls back to `IBM_QUANTUM_TOKEN` with the legacy endpoint.
    pub async fn connect(target: impl Into<String>) -> IbmResult<Self> {
        let target = target.into();

        // Try new Cloud API first (IBM_API_KEY + IBM_SERVICE_CRN)
        if let Ok(api_key) = std::env::var("IBM_API_KEY") {
            let service_crn =
                std::env::var("IBM_SERVICE_CRN").map_err(|_| IbmError::MissingServiceCrn)?;

            tracing::info!("connecting to IBM Cloud API (IAM key exchange)");
            let client = IbmClient::connect(&api_key, &service_crn).await?;

            // Eagerly fetch backend info for real topology
            let info = client.get_backend(&target).await?;
            let capabilities = capabilities_from_real_info(&target, &info);

            // Pre-cache the backend info
            let backend_info = Arc::new(RwLock::new(Some((info, Instant::now()))));

            return Ok(Self {
                client: Arc::new(client),
                capabilities,
                target,
                backend_info,
                skip_transpilation: false,
                shots_cache: Arc::new(Mutex::new(HashMap::new())),
            });
        }

        // Fall back to legacy direct-token mode
        if let Ok(token) = std::env::var("IBM_QUANTUM_TOKEN") {
            tracing::info!("falling back to legacy IBM Quantum token");
            let client = IbmClient::new(LEGACY_ENDPOINT, &token)?;

            return Ok(Self {
                client: Arc::new(client),
                capabilities: capabilities_stub(&target, 127),
                target,
                backend_info: Arc::new(RwLock::new(None)),
                skip_transpilation: false,
                shots_cache: Arc::new(Mutex::new(HashMap::new())),
            });
        }

        Err(IbmError::MissingToken)
    }

    /// Create a backend with explicit configuration.
    pub fn with_config(config: BackendConfig) -> IbmResult<Self> {
        let endpoint = config.endpoint.as_deref().unwrap_or(LEGACY_ENDPOINT);

        let token = config.token.as_ref().ok_or(IbmError::MissingToken)?;

        let target = config
            .extra
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_BACKEND);

        let mut client = IbmClient::new(endpoint, token)?;

        // Set instance if provided
        if let Some(instance) = config.extra.get("instance").and_then(|v| v.as_str()) {
            client = client.with_instance(instance);
        }

        Ok(Self {
            client: Arc::new(client),
            capabilities: capabilities_stub(target, 127),
            target: target.to_string(),
            backend_info: Arc::new(RwLock::new(None)),
            skip_transpilation: false,
            shots_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Tell IBM to skip its own transpilation when submitting jobs.
    ///
    /// Use this when the circuit has already been compiled to the
    /// native basis (e.g. by Arvak's compiler).
    pub fn set_skip_transpilation(&mut self, skip: bool) {
        self.skip_transpilation = skip;
    }

    /// Get the target backend name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Get backend information, fetching from API if not cached or stale.
    async fn get_backend_info(&self) -> IbmResult<BackendInfo> {
        // Check cache first; refresh if older than TTL.
        {
            let cached = self.backend_info.read().await;
            if let Some((ref info, fetched_at)) = *cached {
                if fetched_at.elapsed() < BACKEND_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        // Fetch from API
        let info = self.client.get_backend(&self.target).await?;

        // Cache it with current timestamp
        {
            let mut cached = self.backend_info.write().await;
            *cached = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Convert circuit to `OpenQASM` 3.0 string.
    ///
    /// The emitter already writes `include "stdgates.inc";` and a definition for
    /// every non-stdgates gate it uses (e.g. `rzz`), matching what Qiskit's
    /// `qasm3.dumps` sends in IBM's REST guide. Adding either again makes the
    /// program unparseable (`'p' is already defined`), so it is passed through.
    fn circuit_to_qasm(circuit: &Circuit) -> IbmResult<String> {
        emit(circuit).map_err(|e| IbmError::CircuitError(e.to_string()))
    }

    /// Convert measurement results to counts.
    ///
    /// `num_qubits` is used to pad bitstrings to the correct width when the
    /// result format doesn't provide enough context to infer it.
    ///
    /// `submitted_shots` is the number of shots requested at submission time.
    /// It is used as the denominator when converting quasi-probability
    /// distributions to counts, taking priority over metadata or the 1024 fallback.
    fn results_to_counts(
        results: &crate::api::JobResultResponse,
        num_qubits: usize,
        submitted_shots: Option<u32>,
    ) -> Counts {
        let mut counts = Counts::new();

        // Handle sampler results
        if let Some(result) = results.results.first() {
            // V2 Sampler: raw samples in `data.<register>.samples`
            if let Some(data) = &result.data {
                // Collect samples from all classical registers.
                // For a Bell state with 2 classical bits, the register "c"
                // will contain samples like ["0x0", "0x3", "0x0", ...].
                // Note: we do NOT use `num_qubits` here because that is the
                // device qubit count (e.g. 133) whereas the samples only
                // represent the circuit's measured classical bits.
                for register_data in data.values() {
                    let bit_width = infer_bit_width(&register_data.samples);

                    let mut sample_counts: HashMap<String, u64> = HashMap::new();
                    for sample in &register_data.samples {
                        let binary = hex_to_binary(sample, bit_width);
                        *sample_counts.entry(binary).or_insert(0) += 1;
                    }

                    for (bitstring, count) in sample_counts {
                        counts.insert(bitstring, count);
                    }
                }
                return counts;
            }

            // V1: Try pre-aggregated counts first (more accurate)
            if let Some(raw_counts) = &result.counts {
                for (bitstring, &count) in raw_counts {
                    let binary = hex_to_binary(bitstring, num_qubits);
                    counts.insert(binary, count);
                }
            }
            // V1: Fall back to quasi-distributions
            else if let Some(quasi_dists) = &result.quasi_dists {
                // Prefer shots from the original submission, fall back to
                // IBM metadata, then to the IBM default of 1024.
                let effective_shots = submitted_shots
                    .map(f64::from)
                    .or_else(|| {
                        result
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("shots"))
                            .and_then(serde_json::Value::as_u64)
                            .map(|s| s as f64)
                    })
                    .unwrap_or(1024.0_f64);

                if let Some(dist) = quasi_dists.first() {
                    for (bitstring, &prob) in dist {
                        let binary = hex_to_binary(bitstring, num_qubits);
                        let count = (prob * effective_shots).max(0.0).round() as u64;
                        if count > 0 {
                            counts.insert(binary, count);
                        }
                    }
                }
            }
        }

        counts
    }
}

/// Infer the classical register bit width from the V2 hex samples.
///
/// Finds the maximum value across all samples and uses its bit length.
/// For example, if samples contain "0x3" the max is 3, which needs 2 bits.
/// Falls back to 1 if all samples are zero.
fn infer_bit_width(samples: &[String]) -> usize {
    let max_val = samples
        .iter()
        .filter_map(|s| {
            let hex = s.strip_prefix("0x").unwrap_or(s);
            u64::from_str_radix(hex, 16).ok()
        })
        .max()
        .unwrap_or(0);

    if max_val == 0 {
        // All zeros — need at least 1 bit to display "0"
        1
    } else {
        // Minimum bits needed to represent max_val
        64 - max_val.leading_zeros() as usize
    }
}

/// Convert hex string to binary string, padded to `num_qubits` width.
///
/// If `num_qubits` is 0 the width falls back to 4 bits per hex digit.
fn hex_to_binary(hex: &str, num_qubits: usize) -> String {
    // Handle 0x prefix
    let hex = hex.strip_prefix("0x").unwrap_or(hex);

    // Parse as integer and format as binary, padded to the circuit qubit count
    // so that leading zeros are preserved.
    if let Ok(value) = u64::from_str_radix(hex, 16) {
        let width = if num_qubits > 0 {
            num_qubits
        } else {
            hex.len() * 4
        };
        format!("{value:0>width$b}", value = value, width = width)
    } else {
        // If not hex, assume it's already binary
        hex.to_string()
    }
}

#[async_trait]
impl Backend for IbmBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "ibm"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.get_backend_info().await {
            Ok(info) => {
                if info.status.operational {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: info.status.status_msg,
                    })
                } else {
                    Ok(BackendAvailability::unavailable(
                        info.status
                            .status_msg
                            .unwrap_or_else(|| "backend offline".to_string()),
                    ))
                }
            }
            Err(e) => {
                tracing::warn!("IBM backend availability check failed: {e}");
                Ok(BackendAvailability::unavailable("failed to query backend"))
            }
        }
    }

    async fn validate(&self, circuit: &Circuit, shots: u32) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit requires {} qubits but backend only has {}",
                circuit.num_qubits(),
                caps.num_qubits
            ));
        }

        if shots > caps.max_shots {
            reasons.push(format!(
                "Requested {} shots but backend allows at most {}",
                shots, caps.max_shots
            ));
        }

        if let Some(max_ops) = caps.max_circuit_ops {
            let num_ops = circuit.dag().num_ops();
            if num_ops > max_ops as usize {
                reasons.push(format!(
                    "Circuit has {num_ops} operations but backend allows at most {max_ops}"
                ));
            }
        }

        if !reasons.is_empty() {
            return Ok(ValidationResult::Invalid { reasons });
        }

        // Check gate set support; detect gates that require transpilation (DEBT-14).
        let gate_set = &caps.gate_set;
        let is_eagle = gate_set.two_qubit.iter().any(|g| g == "ecr");
        let mut needs_transpilation = false;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if let Some(reason) = rzz_angle_violation(gate) {
                    reasons.push(reason);
                    break;
                }
                if !gate_set.contains(name) {
                    // On Eagle, CZ/CX are not native but can be synthesised to ECR.
                    if is_eagle && (name == "cz" || name == "cx") {
                        needs_transpilation = true;
                    } else {
                        reasons.push(format!("Unsupported gate: {name}"));
                        break;
                    }
                }
            }
        }

        if !reasons.is_empty() {
            return Ok(ValidationResult::Invalid { reasons });
        }

        if needs_transpilation {
            return Ok(ValidationResult::RequiresTranspilation {
                details: "Eagle backend requires ECR; circuits containing CX/CZ will be \
                          retranslated to ECR + single-qubit gates"
                    .to_string(),
            });
        }

        Ok(ValidationResult::Valid)
    }

    async fn submit(
        &self,
        circuit: &Circuit,
        shots: u32,
        parameters: Option<&std::collections::HashMap<String, f64>>,
    ) -> HalResult<JobId> {
        // DEBT-25: reject non-empty parameter bindings (not yet supported).
        if parameters.is_some_and(|p| !p.is_empty()) {
            return Err(HalError::Unsupported(
                "IBM backend does not support runtime parameter binding".into(),
            ));
        }

        // Pre-submission validation (DEBT-01): catch unsupported gates, qubit-count
        // and shot-count violations before burning queue time or quantum credits.
        if let ValidationResult::Invalid { reasons } = self.validate(circuit, shots).await? {
            return Err(HalError::InvalidCircuit(reasons.join("; ")));
        }

        // Check if backend is operational
        let info = self
            .get_backend_info()
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        if !info.status.operational {
            return Err(HalError::BackendUnavailable(
                info.status
                    .status_msg
                    .unwrap_or_else(|| "Backend offline".to_string()),
            ));
        }

        // Convert circuit to QASM
        let qasm =
            Self::circuit_to_qasm(circuit).map_err(|e| HalError::InvalidCircuit(e.to_string()))?;

        // Submit job
        let response = self
            .client
            .submit_sampler_job(&self.target, vec![qasm], shots, self.skip_transpilation)
            .await
            .map_err(|e| HalError::SubmissionFailed(e.to_string()))?;

        // Cache submitted shot count for accurate quasi-distribution conversion.
        // Evict oldest entry if the cache is full to bound memory growth.
        {
            let mut cache = self.shots_cache.lock().await;
            if cache.len() >= MAX_SHOTS_CACHE {
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
            cache.insert(response.id.clone(), shots);
        }

        Ok(JobId(response.id))
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let status = self
            .client
            .get_job_status(&job_id.0)
            .await
            .map_err(|e| match e {
                IbmError::JobNotFound(id) => HalError::JobNotFound(id),
                other => HalError::Backend(other.to_string()),
            })?;

        let job_status = match status.status.to_uppercase().as_str() {
            "QUEUED" => JobStatus::Queued,
            "VALIDATING" | "RUNNING" => JobStatus::Running,
            "COMPLETED" => JobStatus::Completed,
            "FAILED" | "ERROR" => {
                let msg = status
                    .error_message()
                    .unwrap_or_else(|| "Unknown error".to_string());
                JobStatus::Failed(msg)
            }
            "CANCELLED" => JobStatus::Cancelled,
            _ => JobStatus::Running, // Treat unknown as running
        };

        Ok(job_status)
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        // First check job status
        let status = self
            .client
            .get_job_status(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        if !status.is_completed() {
            if status.is_failed() {
                let msg = status
                    .error_message()
                    .unwrap_or_else(|| "Job failed".to_string());
                return Err(HalError::JobFailed(msg));
            }
            if status.is_cancelled() {
                return Err(HalError::JobCancelled);
            }
            return Err(HalError::Backend(format!(
                "Job {} not yet completed",
                job_id.0
            )));
        }

        // Get results
        let results = self
            .client
            .get_job_results(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        // Use device qubit count for bitstring padding; fall back to hex heuristic
        // if backend info is unavailable.
        let num_qubits = self
            .get_backend_info()
            .await
            .map_or(0, |info| info.num_qubits);

        let submitted_shots = {
            let cache = self.shots_cache.lock().await;
            cache.get(&job_id.0).copied()
        };

        let counts = Self::results_to_counts(&results, num_qubits, submitted_shots);
        let total_shots = counts.total_shots() as u32;

        Ok(ExecutionResult::new(counts, total_shots))
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        self.client
            .cancel_job(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{BackendStatus, ProcessorType};

    fn backend_info(
        name: &str,
        num_qubits: usize,
        family: Option<&str>,
        basis_gates: &[&str],
        coupling_map: Vec<[usize; 2]>,
    ) -> BackendInfo {
        BackendInfo {
            name: name.into(),
            num_qubits,
            status: BackendStatus {
                operational: true,
                status_msg: None,
                pending_jobs: None,
            },
            processor_type: family.map(|f| ProcessorType {
                family: f.into(),
                revision: None,
            }),
            basis_gates: basis_gates.iter().map(|g| (*g).to_string()).collect(),
            coupling_map,
            simulator: false,
            max_shots: None,
            max_circuits: None,
        }
    }

    /// Bidirectional nearest-neighbour edges of a `rows × cols` square lattice.
    fn square_lattice(rows: usize, cols: usize) -> Vec<[usize; 2]> {
        let mut edges = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let q = r * cols + c;
                if c + 1 < cols {
                    edges.extend([[q, q + 1], [q + 1, q]]);
                }
                if r + 1 < rows {
                    edges.extend([[q, q + cols], [q + cols, q]]);
                }
            }
        }
        edges
    }

    fn max_degree(topology: &Topology) -> usize {
        let mut degree: HashMap<u32, usize> = HashMap::new();
        for &(a, _) in &topology.edges {
            *degree.entry(a).or_default() += 1;
        }
        degree.into_values().max().unwrap_or(0)
    }

    #[test]
    fn real_heron_config_yields_cz_gate_set_and_heavy_hex() {
        // Live ibm_fez configuration (trimmed), fetched 2026-10-13.
        let raw: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/ibm_fez_configuration.json"))
                .unwrap();
        let basis: Vec<&str> = raw["basis_gates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g.as_str().unwrap())
            .collect();
        let coupling: Vec<[usize; 2]> =
            serde_json::from_value(raw["coupling_map"].clone()).unwrap();
        let info = backend_info(
            "ibm_fez",
            raw["n_qubits"].as_u64().unwrap() as usize,
            raw["processor_type"]["family"].as_str(),
            &basis,
            coupling,
        );

        let caps = capabilities_from_real_info("ibm_fez", &info);

        assert_eq!(caps.num_qubits, 156);
        assert!(caps.gate_set.contains("cz"));
        assert!(!caps.gate_set.contains("ecr"));
        assert_eq!(caps.topology.kind, TopologyKind::HeavyHex);
        assert_eq!(caps.topology.edges.len(), 352);
        assert_eq!(max_degree(&caps.topology), 3);
    }

    /// Regression: 120-qubit Nighthawk was classified as Eagle (ECR) by the old
    /// `num_qubits <= 127` rule. The config here is SYNTHETIC (10×12 square
    /// lattice, CZ basis) — no live Nighthawk config was available to us.
    #[test]
    fn nighthawk_like_config_is_cz_native_and_not_heavy_hex() {
        let info = backend_info(
            "ibm_miami",
            120,
            Some("Nighthawk"),
            &["cz", "id", "rx", "rz", "rzz", "sx", "x"],
            square_lattice(10, 12),
        );

        let caps = capabilities_from_real_info("ibm_miami", &info);

        assert!(caps.gate_set.contains("cz"));
        assert!(!caps.gate_set.contains("ecr"));
        assert_eq!(caps.topology.kind, TopologyKind::Custom);
        assert_eq!(max_degree(&caps.topology), 4);
    }

    #[test]
    fn ecr_basis_selects_eagle_gate_set() {
        let info = backend_info(
            "ibm_brisbane",
            127,
            Some("Eagle"),
            &["ecr", "rz", "sx", "x"],
            vec![],
        );
        let caps = capabilities_from_real_info("ibm_brisbane", &info);
        assert!(caps.gate_set.contains("ecr"));
        assert_eq!(caps.topology.kind, TopologyKind::HeavyHex);
    }

    #[test]
    fn missing_basis_gates_falls_back_to_name_not_qubit_count() {
        let unknown = backend_info("ibm_newbird", 120, None, &[], vec![]);
        assert!(
            !capabilities_from_real_info("ibm_newbird", &unknown)
                .gate_set
                .contains("ecr")
        );
        assert_eq!(
            capabilities_from_real_info("ibm_newbird", &unknown)
                .topology
                .kind,
            TopologyKind::Custom
        );

        let eagle = backend_info("ibm_kyoto", 127, None, &[], vec![]);
        assert!(
            capabilities_from_real_info("ibm_kyoto", &eagle)
                .gate_set
                .contains("ecr")
        );
    }

    #[test]
    fn test_hex_to_binary() {
        // With num_qubits=0, falls back to hex-digit heuristic (4 bits per digit)
        assert_eq!(hex_to_binary("0x0", 0), "0000");
        assert_eq!(hex_to_binary("0x1", 0), "0001");
        assert_eq!(hex_to_binary("0x3", 0), "0011");
        assert_eq!(hex_to_binary("0xf", 0), "1111");
        assert_eq!(hex_to_binary("0xff", 0), "11111111");
        assert_eq!(hex_to_binary("3", 0), "0011");

        // With explicit num_qubits, pads to correct width
        assert_eq!(hex_to_binary("0x0", 4), "0000");
        assert_eq!(hex_to_binary("0x1", 5), "00001");
        assert_eq!(hex_to_binary("0x3", 8), "00000011");
    }

    #[test]
    fn test_backend_config() {
        // Just test that config parsing works (without token)
        let config = BackendConfig::new("ibm")
            .with_endpoint("https://test.example.com")
            .with_token("test-token");

        assert_eq!(config.name, "ibm");
        assert_eq!(
            config.endpoint,
            Some("https://test.example.com".to_string())
        );
    }

    #[test]
    fn test_results_to_counts_v1() {
        use crate::api::{JobResultResponse, SamplerResult};
        use std::collections::HashMap;

        let mut raw_counts = HashMap::new();
        raw_counts.insert("0x0".to_string(), 500u64);
        raw_counts.insert("0x3".to_string(), 500u64);

        let results = JobResultResponse {
            id: Some("test".to_string()),
            results: vec![SamplerResult {
                data: None,
                quasi_dists: None,
                counts: Some(raw_counts),
                metadata: None,
            }],
        };

        let counts = IbmBackend::results_to_counts(&results, 4, Some(1000));
        assert_eq!(counts.get("0000"), 500);
        assert_eq!(counts.get("0011"), 500);
        assert_eq!(counts.total_shots(), 1000);
    }

    #[test]
    fn test_results_to_counts_v2_samples() {
        use crate::api::{ClassicalRegisterData, JobResultResponse, SamplerResult};
        use std::collections::HashMap;

        // Simulate V2 Sampler results: 10 shots of a Bell state
        // Outcomes: 6x "00" (0x0) and 4x "11" (0x3)
        let samples = vec![
            "0x0".into(),
            "0x3".into(),
            "0x0".into(),
            "0x3".into(),
            "0x0".into(),
            "0x0".into(),
            "0x3".into(),
            "0x0".into(),
            "0x3".into(),
            "0x0".into(),
        ];

        let mut data = HashMap::new();
        data.insert("c".to_string(), ClassicalRegisterData { samples });

        let results = JobResultResponse {
            id: None,
            results: vec![SamplerResult {
                data: Some(data),
                quasi_dists: None,
                counts: None,
                metadata: Some(serde_json::json!({"version": 2})),
            }],
        };

        // num_qubits=133 should NOT affect V2 bitstring width
        let counts = IbmBackend::results_to_counts(&results, 133, Some(10));
        assert_eq!(counts.get("00"), 6);
        assert_eq!(counts.get("11"), 4);
        assert_eq!(counts.total_shots(), 10);
    }

    #[test]
    fn test_results_to_counts_v2_all_zeros() {
        use crate::api::{ClassicalRegisterData, JobResultResponse, SamplerResult};
        use std::collections::HashMap;

        // All shots measured 0
        let samples = vec!["0x0".into(), "0x0".into(), "0x0".into()];

        let mut data = HashMap::new();
        data.insert("c".to_string(), ClassicalRegisterData { samples });

        let results = JobResultResponse {
            id: None,
            results: vec![SamplerResult {
                data: Some(data),
                quasi_dists: None,
                counts: None,
                metadata: None,
            }],
        };

        let counts = IbmBackend::results_to_counts(&results, 133, None);
        assert_eq!(counts.get("0"), 3);
        assert_eq!(counts.total_shots(), 3);
    }

    #[test]
    fn test_infer_bit_width() {
        // Bell state: max value 3 → 2 bits
        let samples = vec!["0x0".into(), "0x3".into(), "0x0".into()];
        assert_eq!(infer_bit_width(&samples), 2);

        // GHZ on 3 qubits: max value 7 → 3 bits
        let samples = vec!["0x0".into(), "0x7".into()];
        assert_eq!(infer_bit_width(&samples), 3);

        // All zeros → 1 bit
        let samples = vec!["0x0".into(), "0x0".into()];
        assert_eq!(infer_bit_width(&samples), 1);

        // Single qubit: max 1 → 1 bit
        let samples = vec!["0x0".into(), "0x1".into()];
        assert_eq!(infer_bit_width(&samples), 1);
    }

    #[tokio::test]
    async fn test_validate_rejects_excess_shots() {
        let config = BackendConfig::new("ibm").with_token("test-token");
        let backend = IbmBackend::with_config(config).unwrap();
        let circuit = Circuit::with_size("test", 2, 2);

        let max_shots = backend.capabilities().max_shots;
        let result = backend.validate(&circuit, max_shots + 1).await.unwrap();
        match result {
            ValidationResult::Invalid { reasons } => {
                assert!(reasons.iter().any(|r| r.contains("shots")));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    /// Regression: the adapter re-inserted `include "stdgates.inc"` and an `rzz`
    /// definition on top of the emitter's own, which Qiskit's QASM3 importer
    /// rejects (`'p' is already defined`) — for every circuit.
    #[test]
    fn qasm_has_single_include_and_rzz_definition() {
        let mut circuit = Circuit::with_size("t", 2, 2);
        circuit.h(arvak_ir::QubitId(0)).unwrap();
        circuit
            .rzz(0.4, arvak_ir::QubitId(0), arvak_ir::QubitId(1))
            .unwrap();
        circuit
            .measure(arvak_ir::QubitId(0), arvak_ir::ClbitId(0))
            .unwrap();
        let qasm = IbmBackend::circuit_to_qasm(&circuit).unwrap();
        if let Ok(path) = std::env::var("ARVAK_DUMP_IBM_QASM") {
            std::fs::write(path, &qasm).unwrap();
        }
        assert_eq!(
            qasm.matches("include \"stdgates.inc\"").count(),
            1,
            "{qasm}"
        );
        assert_eq!(qasm.matches("gate rzz(").count(), 1, "{qasm}");

        let plain = Circuit::with_size("bell", 2, 0);
        let qasm = IbmBackend::circuit_to_qasm(&plain).unwrap();
        assert_eq!(
            qasm.matches("include \"stdgates.inc\"").count(),
            1,
            "{qasm}"
        );
        assert!(!qasm.contains("gate rzz("), "{qasm}");
    }

    #[tokio::test]
    async fn validate_rejects_rzz_outside_native_range() {
        let config = BackendConfig::new("ibm").with_token("test-token");
        let backend = IbmBackend::with_config(config).unwrap();

        for (theta, ok) in [
            (0.4, true),
            (std::f64::consts::FRAC_PI_2, true),
            (2.0, false),
            (-0.4, false),
        ] {
            let mut circuit = Circuit::with_size("t", 2, 0);
            circuit
                .rzz(theta, arvak_ir::QubitId(0), arvak_ir::QubitId(1))
                .unwrap();
            let result = backend.validate(&circuit, 100).await.unwrap();
            match (ok, result) {
                (true, ValidationResult::Valid) => {}
                (false, ValidationResult::Invalid { reasons }) => {
                    assert!(reasons.iter().any(|r| r.contains("rzz")), "{reasons:?}");
                }
                (_, other) => panic!("rzz({theta}): unexpected {other:?}"),
            }
        }

        let mut symbolic = Circuit::with_size("t", 2, 0);
        symbolic
            .rzz(
                arvak_ir::ParameterExpression::symbol("g"),
                arvak_ir::QubitId(0),
                arvak_ir::QubitId(1),
            )
            .unwrap();
        assert!(matches!(
            backend.validate(&symbolic, 100).await.unwrap(),
            ValidationResult::Invalid { .. }
        ));
    }

    #[test]
    fn test_v2_results_deserialization() {
        // Test that the actual V2 JSON structure can be deserialized
        let json = r#"{
            "results": [{
                "data": {
                    "c": {
                        "samples": ["0x0", "0x3", "0x0", "0x3"]
                    }
                },
                "metadata": {
                    "version": 2,
                    "execution": {"execution_spans": []}
                }
            }]
        }"#;

        let response: crate::api::JobResultResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.results.len(), 1);
        let result = &response.results[0];
        assert!(result.data.is_some());
        let data = result.data.as_ref().unwrap();
        assert!(data.contains_key("c"));
        assert_eq!(data["c"].samples.len(), 4);
    }
}
