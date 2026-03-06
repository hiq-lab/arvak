//! IonQ REST API client.
//!
//! Implements the IonQ cloud API v0.4 (`https://api.ionq.co/v0.4`) for
//! submitting quantum circuits and retrieving results.

// Allow dead code for API response fields deserialized but not yet consumed.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::error::{IonQError, IonQResult};

/// IonQ API v0.4 base URL.
pub const BASE_URL: &str = "https://api.ionq.co/v0.4";

/// IonQ REST API client.
///
/// Authenticates via `Authorization: apiKey <token>`.
pub struct IonQClient {
    /// HTTP client with timeouts configured.
    client: Client,
    /// API base URL (without trailing slash).
    base_url: String,
    /// API key for authentication.
    api_key: String,
}

impl std::fmt::Debug for IonQClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IonQClient")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl IonQClient {
    /// Create a new client using the default production API endpoint.
    pub fn new(api_key: impl Into<String>) -> IonQResult<Self> {
        Self::with_base_url(BASE_URL, api_key)
    }

    /// Create a client targeting a custom base URL (useful for testing).
    pub fn with_base_url(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> IonQResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(IonQError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
        })
    }

    /// Build the `Authorization` header value.
    ///
    /// IonQ uses `Authorization: apiKey <token>` (not Bearer).
    fn auth_header(&self) -> String {
        format!("apiKey {}", self.api_key)
    }

    /// Perform a GET request, returning the deserialized JSON body.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> IonQResult<T> {
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
    ) -> IonQResult<T> {
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

    /// Perform a PUT request (used for cancel).
    async fn put(&self, path: &str) -> IonQResult<serde_json::Value> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        debug!("PUT {}", url);

        let resp = self
            .client
            .put(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Perform a DELETE request (used for job deletion).
    async fn delete(&self, path: &str) -> IonQResult<()> {
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
            Err(IonQError::ApiError { status, message })
        }
    }

    /// Handle HTTP response: deserialize JSON or return an error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> IonQResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let message = response.text().await.unwrap_or_default();
            match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(IonQError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
                StatusCode::NOT_FOUND => Err(IonQError::JobNotFound(message)),
                _ => Err(IonQError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Public API methods
    // -----------------------------------------------------------------------

    /// Submit a quantum circuit job.
    #[instrument(skip(self, req))]
    pub async fn submit_job(&self, req: &JobRequest) -> IonQResult<JobResponse> {
        debug!("Submitting circuit to IonQ {}", req.backend);
        self.post("jobs", req).await
    }

    /// Get job status and results.
    #[instrument(skip(self))]
    pub async fn get_job(&self, job_id: &str) -> IonQResult<JobResponse> {
        debug!("Getting IonQ job {}", job_id);
        self.get(&format!("jobs/{job_id}")).await
    }

    /// Cancel a queued or running job.
    ///
    /// IonQ uses `PUT /jobs/{id}/status/cancel`.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> IonQResult<()> {
        debug!("Cancelling IonQ job {}", job_id);
        let _: serde_json::Value = self.put(&format!("jobs/{job_id}/status/cancel")).await?;
        Ok(())
    }

    /// Delete a job.
    #[instrument(skip(self))]
    pub async fn delete_job(&self, job_id: &str) -> IonQResult<()> {
        debug!("Deleting IonQ job {}", job_id);
        self.delete(&format!("jobs/{job_id}")).await
    }

    /// Fetch result distribution from a results URL.
    ///
    /// IonQ v0.4 returns result references like `{"url": "/v0.4/jobs/{id}/results/probabilities"}`.
    /// This method fetches the actual distribution data.
    #[instrument(skip(self))]
    pub async fn fetch_results(&self, results_path: &str) -> IonQResult<HashMap<String, f64>> {
        // The path may be relative (e.g., "/v0.4/...") — prepend base domain.
        let url = if results_path.starts_with('/') {
            // Extract the domain from self.base_url (e.g., "https://api.ionq.co")
            let base_domain = self
                .base_url
                .find("/v0")
                .map_or(self.base_url.as_str(), |idx| &self.base_url[..idx]);
            format!("{base_domain}{results_path}")
        } else {
            results_path.to_string()
        };
        debug!("Fetching IonQ results from {}", url);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// List available backends.
    #[instrument(skip(self))]
    pub async fn list_backends(&self) -> IonQResult<Vec<BackendInfo>> {
        debug!("Listing IonQ backends");
        let resp: BackendListResponse = self.get("backends").await?;
        Ok(resp.backends)
    }

    /// Get a specific backend's info.
    #[instrument(skip(self))]
    pub async fn get_backend(&self, backend_name: &str) -> IonQResult<BackendInfo> {
        debug!("Getting IonQ backend {}", backend_name);
        self.get(&format!("backends/{backend_name}")).await
    }
}

// ---------------------------------------------------------------------------
// IonQ circuit gate operations (QIS gateset)
// ---------------------------------------------------------------------------

/// A single quantum gate operation in IonQ's QIS format.
///
/// All rotation angles are in radians.
#[derive(Debug, Clone, Serialize)]
pub struct IonQGate {
    /// Gate name (e.g., "h", "cx", "rx", "ry", "rz", "swap", "ccx").
    pub gate: String,

    /// Target qubit(s). Single-qubit gates use a single target.
    /// Multi-qubit gates may use `targets` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,

    /// Target qubits for multi-qubit uncontrolled gates (e.g., swap, xx).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<u32>>,

    /// Control qubit(s) for controlled gates (e.g., cx, ccx).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<u32>,

    /// Control qubits for multi-controlled gates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controls: Option<Vec<u32>>,

    /// Rotation angle in radians (for rx, ry, rz, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
}

// ---------------------------------------------------------------------------
// Request / response serde types
// ---------------------------------------------------------------------------

/// Request body for `POST /jobs`.
#[derive(Debug, Serialize)]
pub struct JobRequest {
    /// Job type — `"ionq.circuit.v1"`.
    #[serde(rename = "type")]
    pub job_type: String,

    /// Target backend (e.g., "simulator", "qpu.aria-1").
    pub backend: String,

    /// Number of shots (default: 100).
    pub shots: u32,

    /// Human-readable job name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Circuit input definition.
    pub input: JobInput,
}

/// The input block of a job request.
#[derive(Debug, Serialize)]
pub struct JobInput {
    /// Gateset to use: "qis" (standard) or "native".
    pub gateset: String,

    /// Number of qubits in the circuit.
    pub qubits: u32,

    /// Gate operations.
    pub circuit: Vec<IonQGate>,
}

/// Response from `POST /jobs` or `GET /jobs/{id}`.
#[derive(Debug, Deserialize)]
pub struct JobResponse {
    /// Job identifier.
    pub id: String,

    /// Job status: submitted, ready, running, failed, canceled, completed.
    pub status: String,

    /// Target backend.
    #[serde(default)]
    pub backend: Option<String>,

    /// Number of qubits (not always present — check `stats.qubits` too).
    #[serde(default)]
    pub qubits: Option<u32>,

    /// Number of shots (not always present in response).
    #[serde(default)]
    pub shots: Option<u32>,

    /// Results block — may contain inline data or URLs to fetch data.
    #[serde(default)]
    pub results: Option<JobResults>,

    /// Error details if the job failed.
    #[serde(default)]
    pub error: Option<JobError>,

    /// Job name.
    #[serde(default)]
    pub name: Option<String>,

    /// Job statistics (qubits, gate counts, etc.).
    #[serde(default)]
    pub stats: Option<JobStats>,
}

/// Job statistics from the IonQ API.
#[derive(Debug, Deserialize)]
pub struct JobStats {
    /// Number of qubits used.
    #[serde(default)]
    pub qubits: Option<u32>,

    /// Number of circuits.
    #[serde(default)]
    pub circuits: Option<u32>,
}

/// Job results — may contain inline distributions or URLs to fetch them.
///
/// IonQ v0.4 returns result URLs (e.g., `{"probabilities": {"url": "/v0.4/..."}}`)
/// rather than inline data.  Use [`IonQClient::fetch_results`] to resolve URLs.
#[derive(Debug, Deserialize)]
pub struct JobResults {
    /// Probability distribution — either inline data or a `{"url": "..."}` reference.
    #[serde(default)]
    pub probabilities: Option<serde_json::Value>,

    /// Histogram — either inline data or a `{"url": "..."}` reference.
    #[serde(default)]
    pub histogram: Option<serde_json::Value>,
}

/// Error information for a failed job.
#[derive(Debug, Deserialize)]
pub struct JobError {
    /// Error message.
    #[serde(default)]
    pub message: Option<String>,

    /// Error code.
    #[serde(default)]
    pub code: Option<String>,
}

/// Backend info from `GET /backends` or `GET /backends/{name}`.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendInfo {
    /// Backend name (e.g., "simulator", "qpu.aria-1").
    pub backend: String,

    /// Backend status: available, degraded, unavailable, retired.
    pub status: String,

    /// Number of qubits.
    #[serde(default)]
    pub qubits: Option<u32>,

    /// Average single-qubit gate fidelity.
    #[serde(default)]
    pub average_queue_time: Option<u64>,

    /// Whether this is a simulator.
    #[serde(default)]
    pub has_access: Option<bool>,
}

impl BackendInfo {
    /// Whether the backend is available for job submission.
    pub fn is_available(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "available" | "degraded"
        )
    }
}

/// Wrapper for `GET /backends` response.
#[derive(Debug, Deserialize)]
struct BackendListResponse {
    #[serde(default)]
    backends: Vec<BackendInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ionq_gate_h_serialization() {
        let gate = IonQGate {
            gate: "h".into(),
            target: Some(0),
            targets: None,
            control: None,
            controls: None,
            rotation: None,
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains(r#""gate":"h""#));
        assert!(json.contains(r#""target":0"#));
        assert!(!json.contains("rotation"));
        assert!(!json.contains("control"));
    }

    #[test]
    fn test_ionq_gate_cx_serialization() {
        let gate = IonQGate {
            gate: "cx".into(),
            target: Some(1),
            targets: None,
            control: Some(0),
            controls: None,
            rotation: None,
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains(r#""gate":"cx""#));
        assert!(json.contains(r#""target":1"#));
        assert!(json.contains(r#""control":0"#));
    }

    #[test]
    fn test_ionq_gate_rx_serialization() {
        let gate = IonQGate {
            gate: "rx".into(),
            target: Some(0),
            targets: None,
            control: None,
            controls: None,
            rotation: Some(std::f64::consts::FRAC_PI_2),
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains(r#""gate":"rx""#));
        assert!(json.contains("rotation"));
    }

    #[test]
    fn test_ionq_gate_swap_serialization() {
        let gate = IonQGate {
            gate: "swap".into(),
            target: None,
            targets: Some(vec![0, 1]),
            control: None,
            controls: None,
            rotation: None,
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains(r#""gate":"swap""#));
        assert!(json.contains(r#""targets":[0,1]"#));
        assert!(!json.contains(r#""target""#));
    }

    #[test]
    fn test_ionq_gate_ccx_serialization() {
        let gate = IonQGate {
            gate: "cx".into(),
            target: None,
            targets: Some(vec![2]),
            control: None,
            controls: Some(vec![0, 1]),
            rotation: None,
        };
        let json = serde_json::to_string(&gate).unwrap();
        assert!(json.contains(r#""controls":[0,1]"#));
        assert!(json.contains(r#""targets":[2]"#));
    }

    #[test]
    fn test_backend_info_is_available() {
        let info = BackendInfo {
            backend: "simulator".into(),
            status: "available".into(),
            qubits: Some(29),
            average_queue_time: None,
            has_access: Some(true),
        };
        assert!(info.is_available());

        let degraded = BackendInfo {
            status: "degraded".into(),
            ..info.clone()
        };
        assert!(degraded.is_available());

        let unavailable = BackendInfo {
            status: "unavailable".into(),
            ..info
        };
        assert!(!unavailable.is_available());
    }

    #[test]
    fn test_job_request_serialization() {
        let req = JobRequest {
            job_type: "ionq.circuit.v1".into(),
            backend: "simulator".into(),
            shots: 100,
            name: Some("test-job".into()),
            input: JobInput {
                gateset: "qis".into(),
                qubits: 2,
                circuit: vec![
                    IonQGate {
                        gate: "h".into(),
                        target: Some(0),
                        targets: None,
                        control: None,
                        controls: None,
                        rotation: None,
                    },
                    IonQGate {
                        gate: "cx".into(),
                        target: Some(1),
                        targets: None,
                        control: Some(0),
                        controls: None,
                        rotation: None,
                    },
                ],
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"ionq.circuit.v1""#));
        assert!(json.contains(r#""backend":"simulator""#));
        assert!(json.contains(r#""gateset":"qis""#));
    }
}
