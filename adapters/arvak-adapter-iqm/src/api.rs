//! IQM Resonance v1 API client.
//!
//! Talks to the public IQM Resonance REST API rooted at
//! `https://resonance.iqm.tech/api/v1`. Authentication is a Bearer
//! token from the user's API tokens page (the `IQM_TOKEN` env var
//! the rest of the adapter already reads).
//!
//! # API surface
//!
//! The Resonance v1 surface differs significantly from the retired
//! `cocos.resonance.meetiqm.com/api/v1` interface this adapter used to
//! target. Notable differences:
//!
//! - Job submission is `POST /jobs/{qc}/{job_type}` — the quantum
//!   computer alias (or UUID) and the job type are part of the URL,
//!   not the body. Only `circuit` is currently a valid job type for
//!   live QPUs.
//! - Job status comes back in the `GET /jobs/{job_id}` response body
//!   (there is no separate `/status` endpoint).
//! - Measurement results live under
//!   `GET /jobs/{job_id}/artifacts/measurement_counts` (the artifact
//!   names are advertised in the `Job.artifacts` array).
//! - Status enum is `{waiting, processing, completed, failed, cancelled}`.
//! - The submission body for `circuit` jobs uses the IQM IR JSON
//!   shape — each instruction is `{name, locus, args, implementation}`
//!   where `locus` is a list of qubit labels (`"QB1"`, `"QB2"`, ...).
//!
//! The HAL `Backend` trait only requires this adapter to translate
//! the user's already-IQM-native circuit into this wire shape. Lowering
//! arbitrary gates to PRX/CZ is `arvak-compile`'s job, not ours.

#![allow(dead_code)]

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument};

use crate::error::{IqmError, IqmResult};

/// IQM Resonance v1 API client.
#[derive(Clone)]
pub struct IqmClient {
    /// HTTP client.
    client: Client,
    /// API base URL (e.g. `https://resonance.iqm.tech/api/v1`).
    base_url: String,
    /// Authentication token.
    token: String,
}

impl std::fmt::Debug for IqmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IqmClient")
            .field("base_url", &self.base_url)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl IqmClient {
    /// Create a new IQM Resonance client.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> IqmResult<Self> {
        let base_url = base_url.into();
        let token = token.into();

        if token.is_empty() {
            return Err(IqmError::MissingToken);
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(IqmError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    /// Build the standard request headers (auth + a non-empty UA).
    ///
    /// Resonance is fronted by Cloudflare and 1010-blocks bare HTTP
    /// clients without a User-Agent. The string itself is not
    /// version-gated; anything non-empty passes.
    fn auth_headers(&self) -> reqwest::header::HeaderMap {
        use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .expect("IQM token contains non-ASCII; rejected at insertion"),
        );
        h.insert(
            USER_AGENT,
            HeaderValue::from_static("arvak-adapter-iqm/1.0"),
        );
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        h
    }

    /// Submit a `circuit` job to a quantum computer.
    #[instrument(skip(self, request))]
    pub async fn submit_circuit(&self, qc: &str, request: &SubmitRequest) -> IqmResult<Job> {
        let url = format!("{}/jobs/{}/circuit?use_timeslot=false", self.base_url, qc);
        debug!("POST {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .json(request)
            .send()
            .await?;

        Self::handle_response(response).await
    }

    /// Fetch a job by id (status + metadata).
    ///
    /// The returned [`Job`] carries the current `status` field. There
    /// is no separate `/status` endpoint on the v1 API — this single
    /// GET replaces the legacy two-call pattern.
    #[instrument(skip(self))]
    pub async fn get_job(&self, job_id: &str) -> IqmResult<Job> {
        let url = format!("{}/jobs/{}", self.base_url, job_id);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        Self::handle_response(response).await
    }

    /// Fetch the `measurement_counts` artifact for a completed job.
    ///
    /// Returns a list of per-circuit count blocks; for a single-
    /// circuit job (the only shape this adapter currently submits)
    /// the list has one entry whose `counts` map is keyed by the
    /// concatenated bitstring as IQM serialises it (instruction
    /// order, not qubit-id order — caller may need to re-index).
    #[instrument(skip(self))]
    pub async fn get_measurement_counts(&self, job_id: &str) -> IqmResult<Vec<MeasurementCounts>> {
        let url = format!(
            "{}/jobs/{}/artifacts/measurement_counts",
            self.base_url, job_id
        );
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        Self::handle_response(response).await
    }

    /// Cancel a queued or running job.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> IqmResult<()> {
        let url = format!("{}/jobs/{}/cancel", self.base_url, job_id);
        debug!("POST {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            Err(IqmError::ApiError { status, message })
        }
    }

    /// Get a quantum computer's job-size limits.
    #[instrument(skip(self))]
    pub async fn get_qc_limits(&self, qc: &str) -> IqmResult<QcLimits> {
        let url = format!("{}/quantum-computers/{}/limits", self.base_url, qc);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        Self::handle_response(response).await
    }

    /// Get a quantum computer's health status.
    #[instrument(skip(self))]
    pub async fn get_qc_health(&self, qc: &str) -> IqmResult<QcHealthStatus> {
        let url = format!("{}/quantum-computers/{}/health", self.base_url, qc);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        Self::handle_response(response).await
    }

    /// List all quantum computers visible to this token.
    #[instrument(skip(self))]
    pub async fn list_quantum_computers(&self) -> IqmResult<Vec<QuantumComputer>> {
        let url = format!("{}/quantum-computers", self.base_url);
        debug!("GET {}", url);

        let response = self
            .client
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await?;

        let envelope: QuantumComputerList = Self::handle_response(response).await?;
        Ok(envelope.quantum_computers)
    }

    /// Map a 4xx/5xx response to a typed error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        response: reqwest::Response,
    ) -> IqmResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            return Ok(body);
        }

        let message = response.text().await.unwrap_or_default();
        match status {
            StatusCode::UNAUTHORIZED => Err(IqmError::AuthFailed(message)),
            StatusCode::NOT_FOUND => Err(IqmError::JobNotFound(message)),
            _ => Err(IqmError::ApiError {
                status: status.as_u16(),
                message,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Request / response types — IQM Resonance v1 JSON wire format
// ---------------------------------------------------------------------------

/// Submission body for `POST /jobs/{qc}/circuit`.
///
/// The IQM IR JSON shape: one or more circuits, each a list of
/// instructions in IQM-native form, plus the shot count.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitRequest {
    /// Circuits to execute (we currently only submit one per job).
    pub circuits: Vec<IqmCircuit>,
    /// Number of shots (per circuit).
    pub shots: u32,
}

impl SubmitRequest {
    /// Construct a single-circuit submission.
    pub fn single(circuit: IqmCircuit, shots: u32) -> Self {
        Self {
            circuits: vec![circuit],
            shots,
        }
    }
}

/// One circuit in IQM IR JSON form.
#[derive(Debug, Clone, Serialize)]
pub struct IqmCircuit {
    /// Free-form name used for debugging/Resonance UI display.
    pub name: String,
    /// Instructions in execution order.
    pub instructions: Vec<IqmInstruction>,
}

/// One IQM-native instruction.
///
/// Examples (from a real Resonance payload):
///
/// ```text
/// { "name": "prx",     "locus": ["QB1"],            "args": {"angle": 1.5708, "phase": 0.0}, "implementation": null }
/// { "name": "cz",      "locus": ["QB1", "QB2"],     "args": {},                              "implementation": null }
/// { "name": "measure", "locus": ["QB1"],            "args": {"key": "c_0_0_0"},              "implementation": null }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct IqmInstruction {
    /// Operation name: `prx`, `cz`, `move`, `measure`, ...
    pub name: String,
    /// Qubit / classical register labels this instruction acts on.
    pub locus: Vec<String>,
    /// Operation arguments (e.g. `{angle, phase}` for `prx`, `{key}` for `measure`).
    #[serde(default)]
    pub args: serde_json::Value,
    /// Implementation override (always `null` for client-submitted circuits).
    pub implementation: Option<String>,
}

impl IqmInstruction {
    /// Build a `prx(angle, phase)` instruction.
    pub fn prx(qubit: &str, angle: f64, phase: f64) -> Self {
        Self {
            name: "prx".into(),
            locus: vec![qubit.to_string()],
            args: serde_json::json!({"angle": angle, "phase": phase}),
            implementation: None,
        }
    }

    /// Build a `cz` instruction on two qubits.
    pub fn cz(q1: &str, q2: &str) -> Self {
        Self {
            name: "cz".into(),
            locus: vec![q1.to_string(), q2.to_string()],
            args: serde_json::json!({}),
            implementation: None,
        }
    }

    /// Build a `measure` instruction with the given result key.
    pub fn measure(qubit: &str, key: &str) -> Self {
        Self {
            name: "measure".into(),
            locus: vec![qubit.to_string()],
            args: serde_json::json!({"key": key}),
            implementation: None,
        }
    }
}

/// A submitted job (response from submit / get-job / cancel).
#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    /// Job UUID.
    pub id: String,
    /// Job type (currently always `"circuit"`).
    #[serde(default)]
    pub r#type: Option<String>,
    /// Target quantum computer.
    #[serde(default)]
    pub qc: Option<JobQc>,
    /// Current status.
    pub status: JobStatusValue,
    /// Wall-clock runtime in milliseconds (populated when terminal).
    #[serde(default)]
    pub runtime_ms: Option<u64>,
    /// Position in queue, if applicable.
    #[serde(default)]
    pub queue_position: Option<u32>,
    /// Per-source/state timeline.
    #[serde(default)]
    pub timeline: Vec<JobTimelineEvent>,
    /// Errors leading to a `failed` status.
    #[serde(default)]
    pub errors: Option<Vec<JobError>>,
    /// Artifact descriptors (use the `type` field to GET each artifact).
    #[serde(default)]
    pub artifacts: Vec<JobArtifact>,
}

/// Reference to the QC a job targets.
#[derive(Debug, Clone, Deserialize)]
pub struct JobQc {
    pub id: String,
}

/// One step in a job's processing timeline.
#[derive(Debug, Clone, Deserialize)]
pub struct JobTimelineEvent {
    pub source: String,
    pub status: String,
    pub timestamp: String,
}

/// Descriptor of an artifact available for a job.
#[derive(Debug, Clone, Deserialize)]
pub struct JobArtifact {
    pub r#type: String,
}

/// Error report on a failed job.
#[derive(Debug, Clone, Deserialize)]
pub struct JobError {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

/// IQM Resonance job status enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatusValue {
    Waiting,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatusValue {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// One block of measurement counts (one per submitted circuit).
#[derive(Debug, Clone, Deserialize)]
pub struct MeasurementCounts {
    /// Order of measurement keys; the bitstring keys in `counts`
    /// concatenate one bit per entry in this list, in this order.
    pub measurement_keys: Vec<String>,
    /// Bitstring → count map.
    pub counts: HashMap<String, u64>,
}

/// Job-size limits for a quantum computer.
///
/// Field names mirror the IQM Resonance JSON shape verbatim, hence the
/// repeated `max_*` prefix — renaming them would break serde wire
/// compatibility, so the lint is allowed locally.
#[derive(Debug, Clone, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct QcLimits {
    pub max_instructions_per_circuit: u32,
    pub max_executions_per_job: u64,
    #[serde(default)]
    pub max_circuits_per_job: Option<u32>,
    #[serde(default)]
    pub max_shots_per_circuit: Option<u32>,
}

/// Health snapshot for a quantum computer.
#[derive(Debug, Clone, Deserialize)]
pub struct QcHealthStatus {
    pub healthy: bool,
    pub updated_at: String,
}

/// Quantum computer entry from `GET /quantum-computers`.
#[derive(Debug, Clone, Deserialize)]
pub struct QuantumComputer {
    pub id: String,
    pub alias: String,
    pub display_name: String,
    pub station_control_version: String,
}

/// Envelope for `GET /quantum-computers`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QuantumComputerList {
    pub quantum_computers: Vec<QuantumComputer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_request_serializes_to_resonance_wire_format() {
        // Captured from a real Resonance payload — the exact shape the
        // adapter must produce.
        let req = SubmitRequest::single(
            IqmCircuit {
                name: "probe-bell".into(),
                instructions: vec![
                    IqmInstruction::prx("QB1", std::f64::consts::FRAC_PI_2, 0.0),
                    IqmInstruction::cz("QB1", "QB2"),
                    IqmInstruction::measure("QB1", "c_0_0_0"),
                    IqmInstruction::measure("QB2", "c_0_0_1"),
                ],
            },
            1,
        );
        let json = serde_json::to_value(&req).unwrap();

        assert_eq!(json["shots"], 1);
        let circuits = json["circuits"].as_array().unwrap();
        assert_eq!(circuits.len(), 1);
        assert_eq!(circuits[0]["name"], "probe-bell");

        let instructions = circuits[0]["instructions"].as_array().unwrap();
        assert_eq!(instructions.len(), 4);

        // prx — verify the args shape
        assert_eq!(instructions[0]["name"], "prx");
        assert_eq!(instructions[0]["locus"], serde_json::json!(["QB1"]));
        assert!(instructions[0]["args"]["angle"].is_number());
        assert_eq!(instructions[0]["args"]["phase"], 0.0);
        assert!(instructions[0]["implementation"].is_null());

        // cz — locus is a 2-tuple of qubit labels
        assert_eq!(instructions[1]["name"], "cz");
        assert_eq!(instructions[1]["locus"], serde_json::json!(["QB1", "QB2"]));

        // measure — args carries the result key
        assert_eq!(instructions[2]["name"], "measure");
        assert_eq!(instructions[2]["args"]["key"], "c_0_0_0");
    }

    #[test]
    fn job_status_round_trip_via_lowercase_enum() {
        // Resonance uses lowercase server names; the enum must accept them.
        for (server, expected) in [
            ("waiting", JobStatusValue::Waiting),
            ("processing", JobStatusValue::Processing),
            ("completed", JobStatusValue::Completed),
            ("failed", JobStatusValue::Failed),
            ("cancelled", JobStatusValue::Cancelled),
        ] {
            let parsed: JobStatusValue = serde_json::from_str(&format!("\"{server}\"")).unwrap();
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn job_status_terminality() {
        assert!(!JobStatusValue::Waiting.is_terminal());
        assert!(!JobStatusValue::Processing.is_terminal());
        assert!(JobStatusValue::Completed.is_terminal());
        assert!(JobStatusValue::Failed.is_terminal());
        assert!(JobStatusValue::Cancelled.is_terminal());
    }

    #[test]
    fn measurement_counts_parses_resonance_artifact_shape() {
        // From a real Sirius job's measurement_counts artifact.
        let raw = r#"[
            {
              "measurement_keys": ["c_0_0_0"],
              "counts": {"0": 7, "1": 3}
            }
        ]"#;
        let parsed: Vec<MeasurementCounts> = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].measurement_keys, vec!["c_0_0_0"]);
        assert_eq!(parsed[0].counts.get("0").copied(), Some(7));
        assert_eq!(parsed[0].counts.get("1").copied(), Some(3));
    }
}
