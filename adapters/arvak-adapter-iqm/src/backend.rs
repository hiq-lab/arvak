//! IQM Resonance backend (HAL-thin).
//!
//! Translates an already-IQM-native [`arvak_ir::Circuit`] into the
//! Resonance v1 wire format, submits it, and surfaces the
//! `measurement_counts` artifact back through [`ExecutionResult`].
//!
//! # Layering
//!
//! Per the HAL contract, this backend does *not* transpile. It
//! advertises its native gate set (`prx`, `cz` plus `measure`) via
//! [`Capabilities`], rejects non-native instructions in `validate()`,
//! and is otherwise a thin wire translator. Lowering arbitrary user
//! circuits into PRX/CZ is `arvak-compile`'s responsibility — the
//! same pattern the IBM Heron, AQT, Quantinuum and Quandela adapters
//! follow.

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, instrument};

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, Counts,
    ExecutionResult, HalError, HalResult, Job, JobId, JobStatus, OidcAuth, OidcConfig,
    ValidationResult,
};
use arvak_ir::Circuit;
use arvak_ir::gate::{GateKind, StandardGate};
use arvak_ir::instruction::InstructionKind;

use crate::api::{
    IqmCircuit, IqmClient, IqmInstruction, JobStatusValue, MeasurementCounts, SubmitRequest,
};
use crate::error::{IqmError, IqmResult};

/// Default IQM Resonance v1 API endpoint.
pub const DEFAULT_ENDPOINT: &str = "https://resonance.iqm.tech/api/v1";

/// Default target backend (Garnet — IQM's 20-qubit Resonance device).
pub const DEFAULT_BACKEND: &str = "garnet";

/// Maximum number of cached jobs before evicting completed entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// How long to cache QC health data before re-querying.
const HEALTH_TTL: Duration = Duration::from_secs(5 * 60);

/// Job cache entry.
struct CachedJob {
    job: Job,
    result: Option<ExecutionResult>,
}

/// IQM Resonance backend.
///
/// One [`IqmBackend`] instance targets a single quantum computer
/// alias (`garnet`, `emerald`, etc.). Multiple instances may coexist
/// in the same process — `Backend::name()` returns `"iqm_<target>"`
/// so callers can tell them apart.
pub struct IqmBackend {
    /// Backend configuration.
    config: BackendConfig,
    /// API client.
    client: IqmClient,
    /// Target quantum computer alias or UUID.
    target: String,
    /// Cached capabilities (HAL contract v2: sync introspection).
    capabilities: Capabilities,
    /// Cached job information.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached health snapshot for `availability()`.
    health: Arc<Mutex<Option<(bool, Instant)>>>,
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
    ///
    /// `Backend::name()` will return `"iqm_<target>"` so multiple IQM
    /// instances in the same process remain distinguishable.
    pub fn with_target(target: impl Into<String>) -> IqmResult<Self> {
        let token = std::env::var("IQM_TOKEN").map_err(|_| IqmError::MissingToken)?;
        let target_str: String = target.into();

        let mut config = BackendConfig::new(format!("iqm_{target_str}"))
            .with_endpoint(DEFAULT_ENDPOINT)
            .with_token(&token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target_str));

        Self::from_config_impl(config)
    }

    /// Create a backend with explicit endpoint and token.
    pub fn with_credentials(
        endpoint: impl Into<String>,
        token: impl Into<String>,
        target: impl Into<String>,
    ) -> IqmResult<Self> {
        let target_str: String = target.into();
        let mut config = BackendConfig::new(format!("iqm_{target_str}"))
            .with_endpoint(endpoint)
            .with_token(token);

        config
            .extra
            .insert("target".into(), serde_json::json!(target_str));

        Self::from_config_impl(config)
    }

    /// Create a backend authenticated via OIDC (LUMI Helmi / LRZ).
    ///
    /// Reads a cached OIDC token (from a prior interactive
    /// `arvak auth login` session) and uses it as the Bearer for
    /// Resonance REST calls. Returns an authentication error if no
    /// valid cached token is available, pointing the user at the
    /// login command.
    ///
    /// # Limitation
    ///
    /// The bearer token is fetched once at construction. For very
    /// long-running jobs (≫ 1 hour) that exceed the OIDC access token
    /// lifetime, the in-flight HTTP call may fail with 401 and the
    /// caller must reconstruct the backend (which triggers
    /// auto-refresh from the cached refresh token via [`OidcAuth`]).
    pub fn with_oidc(
        config: OidcConfig,
        target: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> IqmResult<Self> {
        let auth = OidcAuth::new(config)
            .map_err(|e| IqmError::AuthFailed(format!("OIDC handler init failed: {e}")))?;

        let rt = tokio::runtime::Handle::try_current().ok();
        let token = if let Some(handle) = rt {
            tokio::task::block_in_place(|| handle.block_on(auth.get_token()))
        } else {
            let private_rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| IqmError::AuthFailed(format!("runtime build failed: {e}")))?;
            private_rt.block_on(auth.get_token())
        }
        .map_err(|e| IqmError::AuthFailed(format!("OIDC token fetch failed: {e}")))?;

        Self::with_credentials(endpoint, token, target)
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

        // Per-target qubit counts for known Resonance machines (as of
        // 2026-06-25). The first successful availability() call may
        // refresh these via the live API; the defaults here cover the
        // construction-time `capabilities()` call without requiring
        // a network round-trip.
        let num_qubits = match target.to_ascii_lowercase().as_str() {
            "sirius" => 16,
            "emerald" | "crystal" => 54,
            _ => 20,
        };
        let capabilities = Capabilities::iqm(format!("iqm_{target}"), num_qubits);

        Ok(Self {
            config,
            client,
            target,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            health: Arc::new(Mutex::new(None)),
        })
    }

    /// Get the target backend name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Fetch and cache health, refreshing if stale.
    async fn fetch_health(&self) -> IqmResult<bool> {
        {
            let cache = self.health.lock().await;
            if let Some((healthy, fetched_at)) = *cache {
                if fetched_at.elapsed() < HEALTH_TTL {
                    return Ok(healthy);
                }
            }
        }

        let info = self.client.get_qc_health(&self.target).await?;

        {
            let mut cache = self.health.lock().await;
            *cache = Some((info.healthy, Instant::now()));
        }

        Ok(info.healthy)
    }

    /// Translate an Arvak IR circuit into the IQM Resonance v1 wire
    /// format.
    ///
    /// The circuit must already be in IQM-native form — only `PRX(angle,
    /// phase)`, `CZ`, and `Measure` instructions are accepted; anything
    /// else raises `InvalidCircuit`. Use `arvak-compile` with an IQM
    /// target to decompose arbitrary user circuits before submission.
    ///
    /// Qubit labels follow the Resonance convention: `QubitId(i)` maps
    /// to `"QB{i+1}"`. Measurement results are keyed `c_{circuit_idx}_
    /// {classical_register_idx}_{bit_idx}` to match the format we
    /// observed in real Sirius / Garnet jobs.
    fn circuit_to_iqm(&self, circuit: &Circuit) -> Result<IqmCircuit, HalError> {
        let mut instructions = Vec::new();
        let mut measure_idx: usize = 0;

        for (_, inst) in circuit.dag().topological_ops() {
            match &inst.kind {
                InstructionKind::Gate(gate) => {
                    let name = gate.name();
                    let qubit_labels: Vec<String> = inst
                        .qubits
                        .iter()
                        .map(|q| format!("QB{}", q.0 + 1))
                        .collect();

                    match &gate.kind {
                        GateKind::Standard(StandardGate::PRX(theta, phi)) => {
                            let angle = theta.as_f64().ok_or_else(|| {
                                HalError::InvalidCircuit(
                                    "PRX angle parameter not resolvable to a constant; \
                                     IQM Resonance requires concrete angles at submission. \
                                     Bind symbols via arvak.compile() before submission."
                                        .into(),
                                )
                            })?;
                            let phase = phi.as_f64().ok_or_else(|| {
                                HalError::InvalidCircuit(
                                    "PRX phase parameter not resolvable to a constant; \
                                     IQM Resonance requires concrete angles at submission. \
                                     Bind symbols via arvak.compile() before submission."
                                        .into(),
                                )
                            })?;
                            instructions.push(IqmInstruction::prx(&qubit_labels[0], angle, phase));
                        }
                        GateKind::Standard(StandardGate::CZ) => {
                            if qubit_labels.len() != 2 {
                                return Err(HalError::InvalidCircuit(format!(
                                    "CZ requires 2 qubits, got {}",
                                    qubit_labels.len()
                                )));
                            }
                            instructions
                                .push(IqmInstruction::cz(&qubit_labels[0], &qubit_labels[1]));
                        }
                        _ => {
                            return Err(HalError::InvalidCircuit(format!(
                                "{} is not in the IQM native gate set ({{prx, cz}}). \
                                 Compile with arvak.compile(target=\"iqm_{}\") to decompose \
                                 before submission.",
                                name, self.target
                            )));
                        }
                    }
                }
                InstructionKind::Measure => {
                    for q in &inst.qubits {
                        let qubit_label = format!("QB{}", q.0 + 1);
                        // Mirror the Resonance UI key shape we observed in
                        // real jobs: c_{circuit_idx}_0_{measure_idx}.
                        let key = format!("c_0_0_{measure_idx}");
                        instructions.push(IqmInstruction::measure(&qubit_label, &key));
                        measure_idx += 1;
                    }
                }
                InstructionKind::Barrier => {
                    // Barriers are scheduling hints, not native ops on
                    // Resonance — drop them silently.
                }
                InstructionKind::Reset => {
                    return Err(HalError::InvalidCircuit(
                        "Reset is not supported by IQM Resonance circuit jobs.".into(),
                    ));
                }
                InstructionKind::Delay { .. } => {
                    return Err(HalError::InvalidCircuit(
                        "Delay instructions are not yet supported by this adapter.".into(),
                    ));
                }
                InstructionKind::Shuttle { .. } => {
                    return Err(HalError::InvalidCircuit(
                        "Shuttle instructions are not part of the IQM Resonance \
                         instruction set."
                            .into(),
                    ));
                }
                InstructionKind::NoiseChannel { .. } => {
                    return Err(HalError::InvalidCircuit(
                        "Noise channels are not submittable to live IQM hardware.".into(),
                    ));
                }
            }
        }

        if instructions.is_empty() {
            return Err(HalError::InvalidCircuit(
                "Empty circuit (no IQM-native instructions found).".into(),
            ));
        }

        Ok(IqmCircuit {
            name: format!("arvak-{}", uuid_short()),
            instructions,
        })
    }

    /// Convert Resonance measurement_counts → HAL `Counts`.
    ///
    /// Resonance returns a per-circuit list of `{measurement_keys, counts}`
    /// blocks where the count keys are bitstrings concatenated in
    /// `measurement_keys` order. The HAL contract puts qubit 0 in the
    /// RIGHTMOST character of the count key (OpenQASM 3 / Qiskit
    /// convention), so we re-key by reversing the string.
    fn measurement_counts_to_counts(blocks: &[MeasurementCounts]) -> Counts {
        let mut counts = Counts::new();

        // Single-circuit submission ⇒ first (and only) block.
        if let Some(block) = blocks.first() {
            for (bitstring, n) in &block.counts {
                // Resonance bitstring is in measurement_keys order, which
                // is the order measure-instructions appear in the circuit.
                // We submitted them in c_0_0_0, c_0_0_1, ... matching
                // qubit-id order, so reversal lands qubit 0 on the right.
                let normalized: String = bitstring.chars().rev().collect();
                counts.insert(normalized, *n);
            }
        }

        counts
    }

    /// Map a Resonance status enum to the HAL `JobStatus` enum.
    fn map_status(value: JobStatusValue, errors: Option<&Vec<crate::api::JobError>>) -> JobStatus {
        match value {
            JobStatusValue::Waiting => JobStatus::Queued,
            JobStatusValue::Processing => JobStatus::Running,
            JobStatusValue::Completed => JobStatus::Completed,
            JobStatusValue::Cancelled => JobStatus::Cancelled,
            JobStatusValue::Failed => {
                let message = errors
                    .and_then(|es| {
                        es.iter()
                            .find_map(|e| e.message.as_deref())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "IQM Resonance reported failed".into());
                JobStatus::Failed(message)
            }
        }
    }
}

/// Short, unique-enough circuit-name suffix for the Resonance UI.
///
/// Avoids a `uuid` dependency for what is purely a display label —
/// the API does not key off it.
fn uuid_short() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("{nanos:x}-{n:x}")
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
        match self.fetch_health().await {
            Ok(true) => Ok(BackendAvailability {
                is_available: true,
                queue_depth: None,
                estimated_wait: None,
                status_message: None,
            }),
            Ok(false) => Ok(BackendAvailability::unavailable(
                "Resonance reports QC unhealthy",
            )),
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

        // Native-gate enforcement: only `prx`, `cz`, and `measure` are
        // accepted on the wire. The HAL contract puts the responsibility
        // for lowering arbitrary gates on `arvak-compile`, not on the
        // adapter.
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if !gate_set.contains(name) {
                    reasons.push(format!(
                        "Unsupported gate: {name} (IQM Resonance native set is \
                         {{prx, cz}}; compile via arvak.compile(target=\"iqm_{}\") \
                         first)",
                        self.target
                    ));
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
        if parameters.is_some_and(|p| !p.is_empty()) {
            return Err(HalError::Unsupported(
                "IQM backend does not support runtime parameter binding".into(),
            ));
        }

        info!(
            "Submitting circuit to IQM {}: {} qubits, {} shots",
            self.target,
            circuit.num_qubits(),
            shots
        );

        let caps = self.capabilities();
        if circuit.num_qubits() > caps.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit has {} qubits but {} only supports {}",
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

        let iqm_circuit = self.circuit_to_iqm(circuit)?;
        debug!(
            "Translated to IQM wire format ({} instructions)",
            iqm_circuit.instructions.len()
        );
        let request = SubmitRequest::single(iqm_circuit, shots);

        let response = self
            .client
            .submit_circuit(&self.target, &request)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let job_id = JobId::new(&response.id);
        info!("Job submitted: {}", job_id);

        let job = Job::new(job_id.clone(), shots).with_backend(&self.target);
        {
            let mut jobs = self.jobs.lock().await;
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.job.status.is_terminal());
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
        let response = self.client.get_job(&job_id.0).await.map_err(|e| match e {
            IqmError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        let status = Self::map_status(response.status, response.errors.as_ref());

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
        {
            let jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get(&job_id.0) {
                if let Some(ref result) = cached.result {
                    return Ok(result.clone());
                }
            }
        }

        // Confirm the job is actually terminal before fetching the
        // counts artifact — otherwise the artifact may not yet exist.
        let job = self.client.get_job(&job_id.0).await.map_err(|e| match e {
            IqmError::JobNotFound(_) => HalError::JobNotFound(job_id.0.clone()),
            _ => HalError::Backend(e.to_string()),
        })?;

        match job.status {
            JobStatusValue::Completed => {}
            JobStatusValue::Failed => {
                let msg = job
                    .errors
                    .as_ref()
                    .and_then(|es| {
                        es.iter()
                            .find_map(|e| e.message.as_deref())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "IQM Resonance reported failed".into());
                return Err(HalError::JobFailed(msg));
            }
            JobStatusValue::Cancelled => {
                return Err(HalError::JobFailed("job was cancelled".into()));
            }
            JobStatusValue::Waiting | JobStatusValue::Processing => {
                return Err(HalError::JobFailed(format!(
                    "job {} is not yet terminal (status: {:?})",
                    job_id.0, job.status
                )));
            }
        }

        let blocks = self
            .client
            .get_measurement_counts(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        let counts = Self::measurement_counts_to_counts(&blocks);
        let total = counts.total_shots() as u32;
        if total == 0 {
            return Err(HalError::JobFailed(
                "No measurement counts in artifact (empty result)".into(),
            ));
        }

        let mut result = ExecutionResult::new(counts, total);
        if let Some(rt_ms) = job.runtime_ms {
            result = result.with_execution_time(rt_ms);
        }
        result = result.with_metadata(serde_json::json!({
            "backend": format!("iqm_{}", self.target),
            "iqm_job_id": job_id.0.clone(),
            "runtime_ms": job.runtime_ms,
        }));

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
    use arvak_ir::QubitId;
    use arvak_ir::parameter::ParameterExpression;

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
    fn capabilities_advertise_iqm_native_set() {
        let backend =
            IqmBackend::with_credentials("https://example/api/v1", "test-token", "garnet")
                .expect("constructs");

        let caps = backend.capabilities();
        assert!(caps.gate_set.contains("prx"));
        assert!(caps.gate_set.contains("cz"));
        assert!(!caps.gate_set.contains("h"));
        assert!(!caps.gate_set.contains("cx"));
    }

    #[tokio::test]
    async fn validate_rejects_non_native_gate_with_helpful_message() {
        let backend =
            IqmBackend::with_credentials("https://example/api/v1", "test-token", "garnet").unwrap();

        // Build a circuit with H (not native) — must be rejected.
        let mut c = Circuit::with_size("h-test", 1, 1);
        c.h(QubitId(0)).unwrap();
        let vr = backend.validate(&c).await.unwrap();
        match vr {
            ValidationResult::Invalid { reasons } => {
                assert!(
                    reasons
                        .iter()
                        .any(|r| r.contains("Unsupported gate: h") && r.contains("arvak.compile")),
                    "expected helpful reject reason, got {reasons:?}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn validate_accepts_native_prx_cz_circuit() {
        let backend =
            IqmBackend::with_credentials("https://example/api/v1", "test-token", "garnet").unwrap();

        let mut c = Circuit::with_size("native-bell", 2, 2);
        // PRX(π/2, π) on q0 — IQM-native form of Hadamard.
        c.prx(
            ParameterExpression::Constant(std::f64::consts::FRAC_PI_2),
            ParameterExpression::Constant(std::f64::consts::PI),
            QubitId(0),
        )
        .unwrap();
        c.cz(QubitId(0), QubitId(1)).unwrap();
        c.measure(QubitId(0), arvak_ir::ClbitId(0)).unwrap();
        c.measure(QubitId(1), arvak_ir::ClbitId(1)).unwrap();

        let vr = backend.validate(&c).await.unwrap();
        assert!(matches!(vr, ValidationResult::Valid), "got {vr:?}");
    }

    #[test]
    fn circuit_to_iqm_emits_resonance_wire_format() {
        let backend =
            IqmBackend::with_credentials("https://example/api/v1", "test-token", "garnet").unwrap();

        let mut c = Circuit::with_size("native-bell", 2, 2);
        c.prx(
            ParameterExpression::Constant(std::f64::consts::FRAC_PI_2),
            ParameterExpression::Constant(std::f64::consts::PI),
            QubitId(0),
        )
        .unwrap();
        c.cz(QubitId(0), QubitId(1)).unwrap();
        c.measure(QubitId(0), arvak_ir::ClbitId(0)).unwrap();
        c.measure(QubitId(1), arvak_ir::ClbitId(1)).unwrap();

        let translated = backend.circuit_to_iqm(&c).expect("translates");
        assert_eq!(translated.instructions.len(), 4);

        // prx on QB1
        assert_eq!(translated.instructions[0].name, "prx");
        assert_eq!(translated.instructions[0].locus, vec!["QB1".to_string()]);

        // cz QB1, QB2
        assert_eq!(translated.instructions[1].name, "cz");
        assert_eq!(
            translated.instructions[1].locus,
            vec!["QB1".to_string(), "QB2".to_string()]
        );

        // measure QB1 c_0_0_0
        assert_eq!(translated.instructions[2].name, "measure");
        assert_eq!(translated.instructions[2].args["key"], "c_0_0_0");

        // measure QB2 c_0_0_1
        assert_eq!(translated.instructions[3].name, "measure");
        assert_eq!(translated.instructions[3].args["key"], "c_0_0_1");
    }

    #[test]
    fn measurement_counts_bit_order_matches_hal_contract() {
        // q0 measured first → "c_0_0_0", q1 second → "c_0_0_1".
        // Resonance keys are "<q0_bit><q1_bit>" (measurement_keys order).
        // HAL contract puts q0 on the right; we reverse.
        let mut counts_in = std::collections::HashMap::new();
        counts_in.insert("10".to_string(), 7u64); // q0=1, q1=0
        counts_in.insert("01".to_string(), 3u64); // q0=0, q1=1
        let blocks = vec![MeasurementCounts {
            measurement_keys: vec!["c_0_0_0".into(), "c_0_0_1".into()],
            counts: counts_in,
        }];
        let out = IqmBackend::measurement_counts_to_counts(&blocks);
        assert_eq!(out.get("01"), 7);
        assert_eq!(out.get("10"), 3);
    }

    #[test]
    fn map_status_covers_all_resonance_states() {
        assert!(matches!(
            IqmBackend::map_status(JobStatusValue::Waiting, None),
            JobStatus::Queued
        ));
        assert!(matches!(
            IqmBackend::map_status(JobStatusValue::Processing, None),
            JobStatus::Running
        ));
        assert!(matches!(
            IqmBackend::map_status(JobStatusValue::Completed, None),
            JobStatus::Completed
        ));
        assert!(matches!(
            IqmBackend::map_status(JobStatusValue::Cancelled, None),
            JobStatus::Cancelled
        ));
        let failed = IqmBackend::map_status(
            JobStatusValue::Failed,
            Some(&vec![crate::api::JobError {
                message: Some("calibration drift".into()),
                code: None,
            }]),
        );
        match failed {
            JobStatus::Failed(msg) => assert!(msg.contains("calibration drift")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
