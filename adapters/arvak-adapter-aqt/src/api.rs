//! AQT REST API client.
//!
//! Implements the AQT Arnica cloud API (`https://arnica.aqt.eu/api/v1`) for
//! submitting quantum circuits and retrieving results.

// Allow dead code for API response fields deserialized but not yet consumed.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::error::{AqtError, AqtResult};

/// AQT Arnica cloud API base URL.
pub const BASE_URL: &str = "https://arnica.aqt.eu/api/v1";

/// AQT REST API client.
///
/// Authenticates via a static Bearer token read from `AQT_TOKEN`.
/// Offline simulators (`offline_simulator_no_noise`) work with any token value.
pub struct AqtClient {
    /// HTTP client with timeouts configured.
    client: Client,
    /// API base URL (without trailing slash).
    base_url: String,
    /// Bearer token for authentication.
    token: String,
}

impl std::fmt::Debug for AqtClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AqtClient")
            .field("base_url", &self.base_url)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl AqtClient {
    /// Create a new client using the default production API endpoint.
    ///
    /// Reads the token from the `AQT_TOKEN` environment variable.
    /// For offline simulators, `AQT_TOKEN` may be any value (including empty).
    pub fn new(token: impl Into<String>) -> AqtResult<Self> {
        Self::with_base_url(BASE_URL, token)
    }

    /// Create a client targeting a custom base URL (useful for testing).
    pub fn with_base_url(base_url: impl Into<String>, token: impl Into<String>) -> AqtResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(AqtError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
        })
    }

    /// Build the `Authorization` header value.
    ///
    /// AQT uses `Authorization: Bearer <token>` (standard Bearer scheme).
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Perform a GET request, returning the deserialized JSON body.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> AqtResult<T> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        debug!("GET {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a POST request with a JSON body, returning the deserialized JSON body.
    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> AqtResult<T> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        debug!("POST {}", url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a DELETE request, returning `Ok(())` for 2xx responses.
    ///
    /// AQT cancel returns 204 on success, 208 if already cancelled.
    /// Both 204 and 208 are in the 2xx range and treated as success.
    async fn delete(&self, path: &str) -> AqtResult<()> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        debug!("DELETE {}", url);

        let resp = self
            .client
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            Err(AqtError::ApiError { status, message })
        }
    }

    /// Handle HTTP response: deserialize JSON or return an error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> AqtResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let message = response.text().await.unwrap_or_default();
            match status {
                StatusCode::UNAUTHORIZED => Err(AqtError::ApiError {
                    status: 401,
                    message,
                }),
                StatusCode::NOT_FOUND => Err(AqtError::JobNotFound(message)),
                StatusCode::GONE => Err(AqtError::JobNotFound(
                    "Result expired (24h limit exceeded)".into(),
                )),
                _ => Err(AqtError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Public API methods
    // -----------------------------------------------------------------------

    /// List workspaces and their resources.
    #[instrument(skip(self))]
    pub async fn list_workspaces(&self) -> AqtResult<Vec<WorkspaceInfo>> {
        debug!("Listing AQT workspaces");
        self.get("workspaces").await
    }

    /// Get a specific resource's details (qubit count, type, status).
    #[instrument(skip(self))]
    pub async fn get_resource(&self, resource_id: &str) -> AqtResult<ResourceInfo> {
        debug!("Getting AQT resource: {}", resource_id);
        self.get(&format!("resources/{resource_id}")).await
    }

    /// Submit a quantum circuit job.
    ///
    /// `workspace` and `resource` identify the target backend
    /// (e.g., `"default"` + `"offline_simulator_no_noise"`).
    #[instrument(skip(self, req))]
    pub async fn submit_circuit(
        &self,
        workspace: &str,
        resource: &str,
        req: &SubmitRequest,
    ) -> AqtResult<SubmitResponse> {
        debug!("Submitting circuit to AQT {}/{}", workspace, resource);
        self.post(&format!("submit/{workspace}/{resource}"), req)
            .await
    }

    /// Poll status or retrieve results for a job.
    ///
    /// Results are available for 24 hours after completion (410 Gone after that).
    #[instrument(skip(self))]
    pub async fn get_result(&self, job_id: &str) -> AqtResult<ResultResponse> {
        debug!("Getting AQT result for job {}", job_id);
        self.get(&format!("result/{job_id}")).await
    }

    /// Cancel a queued or running job.
    ///
    /// Returns `Ok(())` for 204 (cancelled) and 208 (already cancelled).
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> AqtResult<()> {
        debug!("Cancelling AQT job {}", job_id);
        self.delete(&format!("jobs/{job_id}")).await
    }
}

// ---------------------------------------------------------------------------
// Circuit operation types (AQT JSON gate format)
// ---------------------------------------------------------------------------

/// A single AQT quantum circuit operation.
///
/// All angles are in units of π (divide radians by π before sending).
/// Serializes with `"operation"` as the internal tag field.
#[derive(Debug, Serialize)]
#[serde(tag = "operation")]
pub enum AqtOp {
    /// Z-axis rotation: `RZ(φ·π)`.
    #[serde(rename = "RZ")]
    Rz {
        /// Target qubit index (0-based).
        qubit: u32,
        /// Rotation angle in units of π.
        phi: f64,
    },

    /// Phased-X rotation: `R(θ·π, φ·π) = RZ(-φ·π)·RX(θ·π)·RZ(φ·π)`.
    ///
    /// Covers X (θ=1, φ=0), Y (θ=1, φ=0.5), and general single-qubit rotations.
    #[serde(rename = "R")]
    R {
        /// Target qubit index (0-based).
        qubit: u32,
        /// Rotation angle in units of π, θ ∈ [0, 1].
        theta: f64,
        /// Phase angle in units of π, φ ∈ [0, 2).
        phi: f64,
    },

    /// Mølmer-Sørensen gate: `exp(-i·θ·π/2·XX)`.
    ///
    /// All-to-all connectivity — any qubit pair is valid.
    #[serde(rename = "RXX")]
    Rxx {
        /// Target qubit indices (0-based), `[q0, q1]`.
        qubits: [u32; 2],
        /// Entangling angle in units of π, θ ∈ (0, 0.5].
        theta: f64,
    },

    /// Projective measurement of all qubits simultaneously.
    ///
    /// Must be the last operation in the circuit.
    #[serde(rename = "MEASURE")]
    Measure,
}

// ---------------------------------------------------------------------------
// Request / response serde types
// ---------------------------------------------------------------------------

/// Request body for `POST /submit/{workspace}/{resource}`.
#[derive(Debug, Serialize)]
pub struct SubmitRequest {
    /// Job type — always `"quantum_circuit"`.
    pub job_type: &'static str,
    /// Optional human-readable label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Circuit payload.
    pub payload: SubmitPayload,
}

impl SubmitRequest {
    /// Create a new quantum circuit submission request.
    pub fn new(circuits: Vec<CircuitPayload>) -> Self {
        Self {
            job_type: "quantum_circuit",
            label: Some(format!("arvak-{}", uuid::Uuid::new_v4())),
            payload: SubmitPayload { circuits },
        }
    }
}

/// Top-level payload containing one or more circuits.
#[derive(Debug, Serialize)]
pub struct SubmitPayload {
    /// Circuits to execute (can be batched).
    pub circuits: Vec<CircuitPayload>,
}

/// A single circuit to execute.
#[derive(Debug, Serialize)]
pub struct CircuitPayload {
    /// Number of shots (1–2000).
    pub repetitions: u32,
    /// Number of qubits in the circuit (1–20).
    pub number_of_qubits: u32,
    /// Sequence of quantum operations (including terminal MEASURE).
    pub quantum_circuit: Vec<AqtOp>,
}

/// Response from `POST /submit/{workspace}/{resource}`.
#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    /// Job identification.
    pub job: JobIdWrapper,
    /// Initial status.
    pub response: InitialStatus,
}

/// Wrapper holding the assigned job UUID.
#[derive(Debug, Deserialize)]
pub struct JobIdWrapper {
    /// Assigned job identifier.
    pub job_id: String,
}

/// Initial status returned after submission.
#[derive(Debug, Deserialize)]
pub struct InitialStatus {
    /// Job status (typically `"queued"` immediately after submission).
    pub status: String,
}

/// Response from `GET /result/{job_id}`.
#[derive(Debug, Deserialize)]
pub struct ResultResponse {
    /// Status and optional results.
    pub response: ResultBody,
}

/// The inner body of a result response.
#[derive(Debug, Deserialize)]
pub struct ResultBody {
    /// Job status: `queued | ongoing | finished | error | cancelled`.
    pub status: String,

    /// Measurement results, present when `status == "finished"`.
    ///
    /// Keyed by circuit index string (`"0"`, `"1"`, ...).
    /// Each value is a `[shots × n_qubits]` array of 0/1 integers.
    #[serde(default)]
    pub result: Option<HashMap<String, Vec<Vec<u8>>>>,

    /// Error message if `status == "error"`.
    #[serde(default)]
    pub message: Option<String>,
}

impl ResultBody {
    /// Whether the job is still pending (not yet in a terminal state).
    pub fn is_pending(&self) -> bool {
        matches!(self.status.to_lowercase().as_str(), "queued" | "ongoing")
    }

    /// Whether the job completed successfully.
    pub fn is_finished(&self) -> bool {
        self.status.to_lowercase() == "finished"
    }

    /// Whether the job failed.
    pub fn is_error(&self) -> bool {
        self.status.to_lowercase() == "error"
    }

    /// Whether the job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status.to_lowercase() == "cancelled"
    }
}

/// Workspace info returned by `GET /workspaces`.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceInfo {
    /// Workspace identifier.
    pub id: String,
    /// Resources available in this workspace.
    #[serde(default)]
    pub resources: Vec<ResourceInfo>,
}

/// Resource info returned by `GET /resources/{id}` or nested in workspaces.
#[derive(Debug, Clone, Deserialize)]
pub struct ResourceInfo {
    /// Resource identifier (e.g., `"offline_simulator_no_noise"`).
    pub id: String,
    /// Human-readable name.
    #[serde(default)]
    pub name: Option<String>,
    /// Resource type: `"device"`, `"simulator"`, `"offline_simulator"`.
    #[serde(rename = "type", default)]
    pub resource_type: Option<String>,
    /// Maximum number of qubits.
    #[serde(default)]
    pub num_qubits: Option<u32>,
    /// Current status (e.g., `"online"`, `"offline"`).
    #[serde(default)]
    pub status: Option<String>,
}

impl ResourceInfo {
    /// Whether the resource is currently online / available.
    pub fn is_online(&self) -> bool {
        self.status
            .as_deref()
            .is_some_and(|s| matches!(s.to_lowercase().as_str(), "online" | "available" | "ready"))
    }

    /// Whether this is a simulator (offline or cloud-hosted).
    pub fn is_simulator(&self) -> bool {
        self.resource_type
            .as_deref()
            .is_some_and(|t| matches!(t, "simulator" | "offline_simulator"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aqt_op_rz_serialization() {
        let op = AqtOp::Rz { qubit: 0, phi: 0.5 };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains(r#""operation":"RZ""#));
        assert!(json.contains(r#""qubit":0"#));
        assert!(json.contains(r#""phi":0.5"#));
    }

    #[test]
    fn test_aqt_op_r_serialization() {
        let op = AqtOp::R {
            qubit: 1,
            theta: 0.5,
            phi: 0.25,
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains(r#""operation":"R""#));
        assert!(json.contains(r#""theta":0.5"#));
        assert!(json.contains(r#""phi":0.25"#));
    }

    #[test]
    fn test_aqt_op_rxx_serialization() {
        let op = AqtOp::Rxx {
            qubits: [0, 1],
            theta: 0.25,
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains(r#""operation":"RXX""#));
        assert!(json.contains(r"[0,1]"));
        assert!(json.contains(r#""theta":0.25"#));
    }

    #[test]
    fn test_aqt_op_measure_serialization() {
        let op = AqtOp::Measure;
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains(r#""operation":"MEASURE""#));
        // No extra fields
        assert!(!json.contains("qubit"));
    }

    #[test]
    fn test_result_body_status_methods() {
        let mut body = ResultBody {
            status: "queued".into(),
            result: None,
            message: None,
        };
        assert!(body.is_pending());
        assert!(!body.is_finished());

        body.status = "ongoing".into();
        assert!(body.is_pending());

        body.status = "finished".into();
        assert!(body.is_finished());
        assert!(!body.is_pending());

        body.status = "error".into();
        assert!(body.is_error());

        body.status = "cancelled".into();
        assert!(body.is_cancelled());
    }

    #[test]
    fn test_resource_info_is_simulator() {
        let sim = ResourceInfo {
            id: "offline_simulator_no_noise".into(),
            name: None,
            resource_type: Some("offline_simulator".into()),
            num_qubits: Some(20),
            status: Some("online".into()),
        };
        assert!(sim.is_simulator());
        assert!(sim.is_online());
    }

    #[test]
    fn test_submit_request_creation() {
        let circuit = CircuitPayload {
            repetitions: 100,
            number_of_qubits: 2,
            quantum_circuit: vec![AqtOp::Rz { qubit: 0, phi: 0.5 }, AqtOp::Measure],
        };
        let req = SubmitRequest::new(vec![circuit]);
        assert_eq!(req.job_type, "quantum_circuit");
        assert!(req.label.is_some());
        assert_eq!(req.payload.circuits.len(), 1);
    }
}
