//! Quandela Altair photonic QPU backend.

use async_trait::async_trait;
use tracing::instrument;

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, BackendFactory, Capabilities, ExecutionResult,
    HalError, HalResult, JobId, JobStatus, ValidationResult,
};
use arvak_ir::{Circuit, instruction::InstructionKind};

use crate::api::QuandelaClient;
use crate::error::QuandelaResult;

// Re-export HAL cooling types used in ingest methods.
use arvak_hal::capability::{
    CompressorSpec, CompressorType, CoolingProfile, PufEnrollment, QuietWindow,
    TransferFunctionSample,
};

/// Alsvid enrollment record from the alsvid-lab `/signature` API.
///
/// Deserialises the JSON output of `POST /signature` from alsvid-lab.
/// Field names match the Python `AlsvidEnrollment` schema exactly —
/// this is a contract test boundary (see `test_alsvid_enrollment_schema_matches_arvak_struct`).
#[derive(Debug, serde::Deserialize)]
pub struct AlsvidEnrollment {
    /// Installation-unique identifier.
    pub installation_id: String,
    /// Compressor type as classified by alsvid spectral analysis.
    ///
    /// Possible values: `"rotary_valve"`, `"bellows"`, `"multi_motor"`.
    /// `"bellows"` corresponds to Stirling-cycle / PWS metal-bellows compressors.
    /// Note: for Quandela Altair the 4K cold head is always `GiffordMcMahon`
    /// regardless of this field — alsvid analyses the *compressor drive*, not
    /// the cold head thermodynamic cycle.
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

/// Quandela Altair photonic QPU backend.
///
/// # Status
///
/// Circuit submission (DEBT-Q4) is not yet implemented — the photonic dual-rail
/// encoding pass is pending. `validate()` returns `RequiresTranspilation` for
/// any valid circuit to signal this to orchestrators.
///
/// Alsvid integration is complete: use `ingest_alsvid_enrollment` to populate
/// the PUF enrollment from alsvid-lab output, and `ingest_alsvid_schedule` to
/// add quiet-window scheduling hints.
///
/// # Authentication
///
/// Set `QUANDELA_API_KEY` in the environment.
pub struct QuandelaBackend {
    config: BackendConfig,
    capabilities: Capabilities,
    client: QuandelaClient,
}

impl std::fmt::Debug for QuandelaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuandelaBackend")
            .field("name", &self.config.name)
            .finish()
    }
}

impl QuandelaBackend {
    /// Create a backend using the `QUANDELA_API_KEY` environment variable.
    pub fn new() -> QuandelaResult<Self> {
        let api_key = std::env::var("QUANDELA_API_KEY").unwrap_or_default();
        Self::with_key(api_key)
    }

    /// Create a backend using an explicit API key (useful for testing).
    pub fn with_key(api_key: impl Into<String>) -> QuandelaResult<Self> {
        let api_key = api_key.into();
        let client = QuandelaClient::new(api_key.clone())?;
        let config = BackendConfig::new("quandela_altair");
        let capabilities = Capabilities::quandela("quandela_altair");

        Ok(Self {
            config,
            capabilities,
            client,
        })
    }

    /// Bridge: ingest an alsvid-lab enrollment record into the `CoolingProfile`.
    ///
    /// Populates `cooling_profile.puf_enrollment` and stores per-phase
    /// `visibility_modulation` samples in the transfer function. Overwrites
    /// any existing enrollment.
    pub fn ingest_alsvid_enrollment(&mut self, e: AlsvidEnrollment) {
        let puf = PufEnrollment {
            installation_id: e.installation_id,
            fingerprint_hash: e.fingerprint_hash,
            enrolled_at: e.enrolled_at,
            enrollment_shots: e.enrollment_shots,
            intra_distance_threshold: e.intra_distance_threshold,
        };

        // Build transfer function samples from per-phase HOM visibility.
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
                    t1_modulation: 0.0, // not applicable to photonic
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

    /// Bridge: ingest quiet windows from alsvid-lab scheduling output.
    ///
    /// Overwrites existing quiet windows in the cooling profile.
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

    fn from_config_impl(config: BackendConfig) -> QuandelaResult<Self> {
        let api_key = config
            .token
            .clone()
            .or_else(|| std::env::var("QUANDELA_API_KEY").ok())
            .unwrap_or_default();

        let client = QuandelaClient::new(api_key)?;
        let capabilities = Capabilities::quandela(config.name.clone());

        Ok(Self {
            config,
            capabilities,
            client,
        })
    }
}

#[async_trait]
impl Backend for QuandelaBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "quandela_altair"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[instrument(skip(self))]
    async fn availability(&self) -> HalResult<BackendAvailability> {
        self.client
            .ping()
            .await
            .map(|()| BackendAvailability::always_available())
            .map_err(HalError::from)
    }

    #[instrument(skip(self, circuit))]
    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        // Check qubit count (Altair: 5 logical qubits in dual-rail encoding).
        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit has {} qubits; Quandela Altair supports at most {} \
                 logical qubits in dual-rail encoding",
                circuit.num_qubits(),
                caps.num_qubits
            ));
        }

        // Check gate set.
        for (_, inst) in circuit.dag().topological_ops() {
            if let InstructionKind::Gate(gate) = &inst.kind {
                let name = gate.name();
                if !caps.gate_set.contains(name) {
                    reasons.push(format!("unsupported gate: {name}"));
                    break;
                }
            }
        }

        if !reasons.is_empty() {
            return Ok(ValidationResult::Invalid { reasons });
        }

        // All checks passed, but submission still requires the photonic encoding pass.
        Ok(ValidationResult::RequiresTranspilation {
            details: "photonic dual-rail encoding required; DEBT-Q4".into(),
        })
    }

    async fn submit(
        &self,
        _circuit: &Circuit,
        _shots: u32,
        _parameters: Option<&std::collections::HashMap<String, f64>>,
    ) -> HalResult<JobId> {
        // DEBT-Q4: photonic dual-rail encoding pass not yet implemented.
        Err(HalError::Backend(
            "DEBT-Q4: photonic encoding pass not implemented".into(),
        ))
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        Err(HalError::JobNotFound(job_id.0.clone()))
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        Err(HalError::JobNotFound(job_id.0.clone()))
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        Err(HalError::JobNotFound(job_id.0.clone()))
    }
}

impl BackendFactory for QuandelaBackend {
    fn from_config(config: BackendConfig) -> HalResult<Self> {
        Self::from_config_impl(config).map_err(HalError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bell_circuit() -> Circuit {
        Circuit::bell().expect("bell circuit")
    }

    #[test]
    fn test_quandela_name() {
        let b = QuandelaBackend::with_key("k").unwrap();
        assert_eq!(b.name(), "quandela_altair");
    }

    #[test]
    fn test_quandela_capabilities() {
        let b = QuandelaBackend::with_key("k").unwrap();
        let caps = b.capabilities();
        assert_eq!(caps.num_qubits, 5);
        assert!(!caps.is_simulator);
        assert!(caps.features.contains(&"photonic".to_string()));
        assert!(caps.cooling_profile.is_some());
        let cp = caps.cooling_profile.as_ref().unwrap();
        assert!(!cp.compressor.is_rotary_valve());
    }

    #[tokio::test]
    async fn test_validate_bell_requires_transpilation() {
        let b = QuandelaBackend::with_key("k").unwrap();
        let circuit = bell_circuit();
        let result = b.validate(&circuit).await.unwrap();
        assert!(
            matches!(result, ValidationResult::RequiresTranspilation { .. }),
            "expected RequiresTranspilation, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_validate_too_many_qubits() {
        let b = QuandelaBackend::with_key("k").unwrap();
        let circuit = Circuit::with_size("big", 6, 0);
        let result = b.validate(&circuit).await.unwrap();
        assert!(
            matches!(result, ValidationResult::Invalid { .. }),
            "expected Invalid for 6 qubits, got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_submit_returns_debt_q4_error() {
        let b = QuandelaBackend::with_key("k").unwrap();
        let circuit = bell_circuit();
        let err = b.submit(&circuit, 100).await.unwrap_err();
        assert!(err.to_string().contains("DEBT-Q4"));
    }

    #[test]
    fn test_ingest_alsvid_enrollment_populates_cooling_profile() {
        let mut b = QuandelaBackend::with_key("k").unwrap();
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
        let mut b = QuandelaBackend::with_key("k").unwrap();
        let windows = vec![QuietWindow {
            cycle_offset: 0.1,
            cycle_fraction: 0.15,
            t1_improvement_factor: None,
        }];
        b.ingest_alsvid_schedule(windows);

        let cp = b.capabilities().cooling_profile.as_ref().unwrap();
        assert_eq!(cp.quiet_windows.len(), 1);
    }
}
