//! Quandela photonic QPU backend (Ascella, Belenos).
//!
//! Submits circuits to the Quandela Cloud via the Perceval Python bridge
//! (`perceval_bridge.py`).  Dual-rail encoding and serialisation are handled
//! entirely by the bridge; this crate calls it as a subprocess and marshals
//! the JSON output into HAL types.
//!
//! # Supported platforms
//!
//! | Platform name   | Qubits | Notes                               |
//! |-----------------|--------|-------------------------------------|
//! | `sim:ascella`   | 6      | Ascella simulator (free)            |
//! | `qpu:ascella`   | 6      | Ascella physical QPU                |
//! | `sim:belenos`   | 12     | Belenos simulator                   |
//! | `qpu:belenos`   | 12     | Belenos physical QPU (12q, 2025)    |
//! | `quandela_altair` | 5    | Legacy Altair (4K cryocooled)       |
//!
//! # Authentication
//!
//! Set `PCVL_CLOUD_TOKEN` (or place the token in
//! `~/.openclaw/credentials/quandela/cloud.key`).
//!
//! # Circuit constraints
//!
//! The Perceval bridge supports H, X, Y, Z, S, Sdg, T, Tdg, Rx, Ry, Rz, CX,
//! and CZ.  For two-qubit gates the bridge currently requires adjacent qubit
//! pairs (ctrl = i, data = i+1).  Non-adjacent pairs will fail at `submit()`
//! time with a clear error.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::instrument;

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, ExecutionResult,
    HalError, HalResult, JobId, JobStatus, ValidationResult,
    capability::{CompressorSpec, CompressorType, CoolingProfile},
};
use arvak_ir::{Circuit, instruction::InstructionKind};

use crate::api::{MAX_CACHED_JOBS, call_bridge, find_bridge_script, find_python};
use crate::error::{QuandelaError, QuandelaResult};

// Re-export HAL cooling types used in ingest methods.
use arvak_hal::capability::{PufEnrollment, QuietWindow, TransferFunctionSample};
use arvak_ir::gate::GateKind;

/// Alsvid enrollment record from the alsvid-lab `/signature` API.
///
/// Deserialises the JSON output of `POST /signature` from alsvid-lab.
/// Field names match the Python `AlsvidEnrollment` schema exactly —
/// this is a contract test boundary.
#[derive(Debug, serde::Deserialize)]
pub struct AlsvidEnrollment {
    /// Installation-unique identifier.
    pub installation_id: String,
    /// Compressor type as classified by alsvid spectral analysis.
    pub compressor_type: String,
    /// SHA-256 fingerprint hash (hex, 64 chars).
    pub fingerprint_hash: String,
    /// Unix timestamp of enrollment.
    pub enrolled_at: u64,
    /// Shots used per sample point during enrollment.
    pub enrollment_shots: u32,
    /// Maximum acceptable intra-distance for verification.
    pub intra_distance_threshold: Option<f64>,
    /// Dominant compressor vibration frequency in Hz.
    pub fundamental_hz: f64,
    /// HOM visibility per compressor phase (len = sample_count).
    pub hom_visibility_by_phase: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Per-job metadata cached at submit() time.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct CircuitMeta {
    n_qubits: usize,
    circuit_json: String,
}

// ---------------------------------------------------------------------------
// Platform helper
// ---------------------------------------------------------------------------

fn platform_num_qubits(platform: &str) -> u32 {
    match platform {
        "sim:ascella" | "qpu:ascella" => 6,
        "sim:belenos" | "qpu:belenos" => 12,
        _ => 5, // legacy Altair / unknown
    }
}

fn platform_is_simulator(platform: &str) -> bool {
    platform.starts_with("sim:")
}

fn build_capabilities(platform: &str) -> Capabilities {
    let n = platform_num_qubits(platform);
    let is_sim = platform_is_simulator(platform);

    // Altair is the only cryocooled variant.
    let cooling = if platform == "quandela_altair" {
        Some(CoolingProfile::new(CompressorSpec {
            model: "Quandela Altair 4K cryocooler".into(),
            cycle_frequency_hz: 1.0,
            stage_temperatures_k: vec![4.0],
            compressor_type: CompressorType::GiffordMcMahon,
        }))
    } else {
        None
    };

    Capabilities {
        name: platform.into(),
        num_qubits: n,
        gate_set: arvak_hal::GateSet::quandela(),
        topology: arvak_hal::Topology::full(n),
        max_shots: 100_000,
        max_circuit_ops: None,
        is_simulator: is_sim,
        features: vec!["photonic".into()],
        noise_profile: None,
        cooling_profile: cooling,
    }
}

// ---------------------------------------------------------------------------
// Circuit → JSON serialisation
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct CircuitJson<'a> {
    n_qubits: usize,
    gates: Vec<GateJson<'a>>,
}

#[derive(serde::Serialize)]
struct GateJson<'a> {
    name: &'a str,
    qubits: Vec<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    params: Vec<f64>,
}

fn circuit_to_json(circuit: &Circuit) -> QuandelaResult<String> {
    let n_qubits = circuit.num_qubits();
    let mut gates = Vec::new();

    for (_, inst) in circuit.dag().topological_ops() {
        match &inst.kind {
            InstructionKind::Measure | InstructionKind::Reset | InstructionKind::Barrier => {
                // Include measure so the bridge knows to skip it.
                let qubits = inst.qubits.iter().map(|q| q.0 as usize).collect();
                gates.push(GateJson {
                    name: inst.name(),
                    qubits,
                    params: vec![],
                });
            }
            InstructionKind::Gate(gate) => {
                let name = gate.name();
                let qubits = inst.qubits.iter().map(|q| q.0 as usize).collect();
                // Extract numeric parameter values (resolved constants only).
                let params: Vec<f64> = match &gate.kind {
                    GateKind::Standard(sg) => sg
                        .parameters()
                        .into_iter()
                        .filter_map(|p: &arvak_ir::parameter::ParameterExpression| p.as_f64())
                        .collect(),
                    GateKind::Custom(cg) => cg
                        .params
                        .iter()
                        .filter_map(arvak_ir::ParameterExpression::as_f64)
                        .collect(),
                };
                gates.push(GateJson {
                    name,
                    qubits,
                    params,
                });
            }
            _ => {}
        }
    }

    serde_json::to_string(&CircuitJson { n_qubits, gates }).map_err(QuandelaError::from)
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

async fn cache_insert(
    cache: &Mutex<HashMap<String, CircuitMeta>>,
    job_id: &str,
    meta: CircuitMeta,
) {
    let mut lock = cache.lock().await;
    if lock.len() >= MAX_CACHED_JOBS {
        // Evict one entry (FIFO approximation — remove the first key found).
        if let Some(oldest) = lock.keys().next().cloned() {
            lock.remove(&oldest);
        }
    }
    lock.insert(job_id.to_string(), meta);
}

async fn cache_get(
    cache: &Mutex<HashMap<String, CircuitMeta>>,
    job_id: &str,
) -> Option<CircuitMeta> {
    cache.lock().await.get(job_id).cloned()
}

// ---------------------------------------------------------------------------
// QuandelaBackend
// ---------------------------------------------------------------------------

/// Quandela photonic QPU backend.
///
/// Supports `sim:ascella`, `qpu:ascella`, `sim:belenos`, `qpu:belenos`, and
/// the legacy `quandela_altair` (Altair 4K cryocooled, 5q).
pub struct QuandelaBackend {
    config: BackendConfig,
    capabilities: Capabilities,
    /// Perceval Cloud platform identifier (e.g. `"sim:ascella"`).
    platform: String,
    /// Path to the Python interpreter.
    python: String,
    /// Path to `perceval_bridge.py`.
    bridge: PathBuf,
    /// Per-job circuit metadata (needed for result decoding).
    job_cache: Arc<Mutex<HashMap<String, CircuitMeta>>>,
}

impl std::fmt::Debug for QuandelaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuandelaBackend")
            .field("platform", &self.platform)
            .finish()
    }
}

impl QuandelaBackend {
    /// Create a backend for `sim:ascella` using default Python / bridge path.
    pub fn new() -> QuandelaResult<Self> {
        Self::for_platform("sim:ascella")
    }

    /// Create a backend for an explicit Quandela Cloud platform.
    ///
    /// Supported: `sim:ascella`, `qpu:ascella`, `sim:belenos`, `qpu:belenos`,
    /// `quandela_altair`.
    pub fn for_platform(platform: impl Into<String>) -> QuandelaResult<Self> {
        let platform = platform.into();
        let config = BackendConfig::new(platform.clone());
        let capabilities = build_capabilities(&platform);

        Ok(Self {
            config,
            capabilities,
            platform,
            python: find_python(),
            bridge: find_bridge_script(),
            job_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Create a backend using an explicit API key (kept for test compatibility).
    ///
    /// In the Perceval bridge architecture the token is read from the
    /// `PCVL_CLOUD_TOKEN` env var (or the key file).  This constructor stores
    /// the token in the backend and passes it to the bridge subprocess via the
    /// environment when making calls.
    pub fn with_key(api_key: impl Into<String>) -> QuandelaResult<Self> {
        let key = api_key.into();
        let mut backend = Self::for_platform("sim:ascella")?;
        if !key.is_empty() {
            backend.config.token = Some(key);
        }
        Ok(backend)
    }

    /// Override the Python interpreter path (mainly for testing).
    #[must_use]
    pub fn with_python(mut self, python: impl Into<String>) -> Self {
        self.python = python.into();
        self
    }

    /// Override the bridge script path (mainly for testing).
    #[must_use]
    pub fn with_bridge(mut self, bridge: impl Into<PathBuf>) -> Self {
        self.bridge = bridge.into();
        self
    }

    // ── Alsvid integration (Altair only) ────────────────────────────────────

    /// Ingest an alsvid-lab enrollment record into the `CoolingProfile`.
    ///
    /// Populates `cooling_profile.puf_enrollment` and stores per-phase
    /// `visibility_modulation` samples in the transfer function.
    pub fn ingest_alsvid_enrollment(&mut self, e: AlsvidEnrollment) {
        let puf = PufEnrollment {
            installation_id: e.installation_id,
            fingerprint_hash: e.fingerprint_hash,
            enrolled_at: e.enrolled_at,
            enrollment_shots: e.enrollment_shots,
            intra_distance_threshold: e.intra_distance_threshold,
        };

        let fundamental_hz = e.fundamental_hz;
        let n = e.hom_visibility_by_phase.len();
        let transfer_function: Vec<TransferFunctionSample> = e
            .hom_visibility_by_phase
            .into_iter()
            .enumerate()
            .map(|(i, vis)| {
                let phase_fraction = if n > 1 { i as f64 / n as f64 } else { 0.0 };
                TransferFunctionSample {
                    freq_hz: fundamental_hz * (1.0 + phase_fraction),
                    t1_modulation: 0.0,
                    visibility_modulation: Some(vis),
                }
            })
            .collect();

        if let Some(ref mut cp) = self.capabilities.cooling_profile {
            cp.puf_enrollment = Some(puf);
            cp.transfer_function = transfer_function;
        } else {
            let mut cp = CoolingProfile::new(CompressorSpec {
                model: "Quandela Altair 4K cryocooler".into(),
                cycle_frequency_hz: fundamental_hz,
                stage_temperatures_k: vec![4.0],
                compressor_type: CompressorType::GiffordMcMahon,
            });
            cp.puf_enrollment = Some(puf);
            cp.transfer_function = transfer_function;
            self.capabilities.cooling_profile = Some(cp);
        }
    }

    /// Ingest quiet windows from alsvid-lab scheduling output.
    pub fn ingest_alsvid_schedule(&mut self, windows: Vec<QuietWindow>) {
        if let Some(ref mut cp) = self.capabilities.cooling_profile {
            cp.quiet_windows = windows;
        } else {
            let mut cp = CoolingProfile::new(CompressorSpec {
                model: "Quandela Altair 4K cryocooler".into(),
                cycle_frequency_hz: 1.0,
                stage_temperatures_k: vec![4.0],
                compressor_type: CompressorType::GiffordMcMahon,
            });
            cp.quiet_windows = windows;
            self.capabilities.cooling_profile = Some(cp);
        }
    }

    // ── Bridge helpers ───────────────────────────────────────────────────────

    async fn bridge_call(&self, args: &[&str]) -> QuandelaResult<serde_json::Value> {
        let token = self.config.token.as_deref();
        call_bridge(&self.python, &self.bridge, args, token).await
    }

    fn from_config_impl(config: BackendConfig) -> QuandelaResult<Self> {
        let platform = if config.name.contains(':') {
            config.name.clone()
        } else {
            "sim:ascella".to_string()
        };

        let mut backend = Self::for_platform(platform)?;
        backend.config = config;
        Ok(backend)
    }
}

// ---------------------------------------------------------------------------
// HAL Backend trait
// ---------------------------------------------------------------------------

#[async_trait]
impl Backend for QuandelaBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        &self.platform
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        let resp = self
            .bridge_call(&["ping", &self.platform])
            .await
            .map_err(HalError::from)?;

        let status = resp
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        if status == "online" || status == "available" {
            Ok(BackendAvailability::always_available())
        } else {
            Ok(BackendAvailability::unavailable(format!(
                "platform status: {status}"
            )))
        }
    }

    #[instrument(skip(self, circuit))]
    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        // Qubit count.
        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "circuit has {} qubits; {} supports at most {}",
                circuit.num_qubits(),
                self.platform,
                caps.num_qubits,
            ));
        }

        // Gate set.
        for (_, inst) in circuit.dag().topological_ops() {
            if let InstructionKind::Gate(gate) = &inst.kind {
                let name = gate.name();
                if !caps.gate_set.contains(name) {
                    reasons.push(format!("unsupported gate: {name}"));
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
        let _ = parameters; // Quandela cloud doesn't support runtime parameter binding yet.
        let circuit_json =
            circuit_to_json(circuit).map_err(|e| HalError::Backend(e.to_string()))?;

        let shots_str = shots.to_string();
        let resp = self
            .bridge_call(&["submit", &self.platform, &shots_str, &circuit_json])
            .await
            .map_err(HalError::from)?;

        let job_id = resp
            .get("job_id")
            .and_then(|j| j.as_str())
            .ok_or_else(|| HalError::Backend("bridge submit: missing job_id".into()))?
            .to_string();

        // Cache circuit metadata for result decoding.
        cache_insert(
            &self.job_cache,
            &job_id,
            CircuitMeta {
                n_qubits: circuit.num_qubits(),
                circuit_json,
            },
        )
        .await;

        Ok(JobId::new(job_id))
    }

    #[instrument(skip(self))]
    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let resp = self
            .bridge_call(&["status", &self.platform, &job_id.0])
            .await
            .map_err(HalError::from)?;

        let status = resp
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");
        let msg = resp
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        Ok(match status {
            "queued" => JobStatus::Queued,
            "running" => JobStatus::Running,
            "done" => JobStatus::Completed,
            "cancelled" => JobStatus::Cancelled,
            "error" => JobStatus::Failed(msg),
            other => JobStatus::Failed(format!("unknown status: {other}")),
        })
    }

    #[instrument(skip(self))]
    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        let meta = cache_get(&self.job_cache, &job_id.0).await.ok_or_else(|| {
            HalError::Backend(format!(
                "no cached circuit metadata for job {}: \
                 result() must be called from the same process that called submit()",
                job_id.0
            ))
        })?;

        let n_qubits_str = meta.n_qubits.to_string();
        let resp = self
            .bridge_call(&[
                "result",
                &self.platform,
                &job_id.0,
                &n_qubits_str,
                &meta.circuit_json,
            ])
            .await
            .map_err(HalError::from)?;

        let counts_obj = resp
            .get("counts")
            .and_then(|c| c.as_object())
            .ok_or_else(|| HalError::Backend("bridge result: missing counts object".into()))?;

        let mut counts = arvak_hal::Counts::new();
        let mut total: u64 = 0;
        for (bitstring, count_val) in counts_obj {
            let count = count_val.as_u64().unwrap_or(0);
            counts.insert(bitstring.clone(), count);
            total = total.saturating_add(count);
        }

        let shots = u32::try_from(total).unwrap_or(u32::MAX);
        Ok(ExecutionResult::new(counts, shots))
    }

    #[instrument(skip(self))]
    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        self.bridge_call(&["cancel", &self.platform, &job_id.0])
            .await
            .map(|_| ())
            .map_err(HalError::from)
    }
}

impl BackendFactory for QuandelaBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        Self::from_config_impl(config).map_err(HalError::from)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn bell_circuit() -> Circuit {
        Circuit::bell().expect("bell circuit")
    }

    #[test]
    fn test_quandela_name_ascella() {
        let b = QuandelaBackend::for_platform("sim:ascella").unwrap();
        assert_eq!(b.name(), "sim:ascella");
    }

    #[test]
    fn test_quandela_name_belenos() {
        let b = QuandelaBackend::for_platform("sim:belenos").unwrap();
        assert_eq!(b.name(), "sim:belenos");
    }

    #[test]
    fn test_capabilities_ascella() {
        let b = QuandelaBackend::for_platform("sim:ascella").unwrap();
        let caps = b.capabilities();
        assert_eq!(caps.num_qubits, 6);
        assert!(caps.is_simulator);
        assert!(caps.features.contains(&"photonic".to_string()));
        assert!(caps.cooling_profile.is_none());
    }

    #[test]
    fn test_capabilities_belenos() {
        let b = QuandelaBackend::for_platform("qpu:belenos").unwrap();
        let caps = b.capabilities();
        assert_eq!(caps.num_qubits, 12);
        assert!(!caps.is_simulator);
    }

    #[test]
    fn test_capabilities_altair_has_cooling_profile() {
        let b = QuandelaBackend::for_platform("quandela_altair").unwrap();
        let caps = b.capabilities();
        assert_eq!(caps.num_qubits, 5);
        assert!(caps.cooling_profile.is_some());
    }

    #[tokio::test]
    async fn test_validate_bell_valid() {
        let b = QuandelaBackend::for_platform("sim:ascella").unwrap();
        let circuit = bell_circuit();
        let result = b.validate(&circuit).await.unwrap();
        assert!(
            matches!(result, ValidationResult::Valid),
            "expected Valid, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_validate_too_many_qubits() {
        let b = QuandelaBackend::for_platform("sim:ascella").unwrap();
        let circuit = Circuit::with_size("big", 7, 0);
        let result = b.validate(&circuit).await.unwrap();
        assert!(
            matches!(result, ValidationResult::Invalid { .. }),
            "expected Invalid for 7 qubits, got {result:?}"
        );
    }

    #[test]
    fn test_circuit_to_json_bell() {
        let circuit = bell_circuit();
        let json_str = circuit_to_json(&circuit).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["n_qubits"], 2);
        let gates = v["gates"].as_array().unwrap();
        // Should contain H and CX at minimum
        assert!(gates.iter().any(|g| g["name"] == "h"));
        assert!(gates.iter().any(|g| g["name"] == "cx"));
    }

    #[test]
    fn test_ingest_alsvid_enrollment_populates_cooling_profile() {
        let mut b = QuandelaBackend::for_platform("quandela_altair").unwrap();
        let enrollment = AlsvidEnrollment {
            installation_id: "altair-sn001-paris".into(),
            compressor_type: "bellows".into(),
            fingerprint_hash: "a".repeat(64),
            enrolled_at: 1_740_000_000,
            enrollment_shots: 1000,
            intra_distance_threshold: Some(0.08),
            fundamental_hz: 1.2,
            hom_visibility_by_phase: vec![0.95, 0.92, 0.88, 0.90, 0.93, 0.91, 0.89, 0.94],
        };
        b.ingest_alsvid_enrollment(enrollment);

        let cp = b.capabilities().cooling_profile.as_ref().unwrap();
        assert!(cp.is_enrolled());
        assert_eq!(cp.transfer_function.len(), 8);
        assert!(cp.transfer_function[0].visibility_modulation.is_some());
    }

    #[test]
    fn test_ingest_alsvid_schedule_populates_quiet_windows() {
        let mut b = QuandelaBackend::for_platform("quandela_altair").unwrap();
        let windows = vec![QuietWindow {
            cycle_offset: 0.1,
            cycle_fraction: 0.15,
            t1_improvement_factor: None,
        }];
        b.ingest_alsvid_schedule(windows);

        let cp = b.capabilities().cooling_profile.as_ref().unwrap();
        assert_eq!(cp.quiet_windows.len(), 1);
    }

    #[test]
    fn test_alsvid_enrollment_schema_matches_arvak_struct() {
        // Contract test: JSON produced by alsvid-lab /signature must deserialise
        // into AlsvidEnrollment without errors.
        let json = r#"{
            "installation_id": "altair-sn001",
            "compressor_type": "bellows",
            "fingerprint_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "enrolled_at": 1740000000,
            "enrollment_shots": 1000,
            "intra_distance_threshold": 0.08,
            "fundamental_hz": 1.2,
            "hom_visibility_by_phase": [0.95, 0.92]
        }"#;
        let enrollment: AlsvidEnrollment = serde_json::from_str(json).unwrap();
        assert_eq!(enrollment.installation_id, "altair-sn001");
        assert_eq!(enrollment.hom_visibility_by_phase.len(), 2);
    }
}
