//! Quantinuum REST API client.
//!
//! Implements the Quantinuum cloud API (`https://qapi.quantinuum.com/v1/`) for
//! submitting quantum circuits and retrieving results.

// Allow dead code for API response fields deserialized but not yet consumed.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use crate::error::{QuantinuumError, QuantinuumResult};

/// Quantinuum cloud API base URL.
pub const BASE_URL: &str = "https://qapi.quantinuum.com/v1";

/// Quantinuum REST API client.
///
/// Handles email/password authentication, JWT refresh on expiry, and
/// all job lifecycle calls.
pub struct QuantinuumClient {
    /// HTTP client with timeouts configured.
    client: Client,
    /// API base URL (without trailing slash).
    base_url: String,
    /// Email address for authentication.
    email: String,
    /// Password for authentication (stored for re-login on 401).
    password: String,
    /// Current JWT id-token; `None` means not yet authenticated.
    token: Arc<Mutex<Option<String>>>,
    /// Refresh token returned alongside the id-token.
    refresh_token: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for QuantinuumClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuantinuumClient")
            .field("base_url", &self.base_url)
            .field("email", &self.email)
            .field("token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .finish()
    }
}

impl QuantinuumClient {
    /// Create a new client using the default production API endpoint.
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> QuantinuumResult<Self> {
        Self::with_base_url(BASE_URL, email, password)
    }

    /// Create a client targeting a custom base URL (useful for testing).
    pub fn with_base_url(
        base_url: impl Into<String>,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> QuantinuumResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(QuantinuumError::Http)?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            email: email.into(),
            password: password.into(),
            token: Arc::new(Mutex::new(None)),
            refresh_token: Arc::new(Mutex::new(None)),
        })
    }

    /// Authenticate with email + password; stores the JWT and refresh token.
    #[instrument(skip(self))]
    pub async fn login(&self) -> QuantinuumResult<()> {
        let url = format!("{}/login", self.base_url);
        debug!("Logging in to Quantinuum API");

        let body = serde_json::json!({
            "email": self.email,
            "password": self.password,
        });

        let resp = self.client.post(&url).json(&body).send().await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            let msg = resp.text().await.unwrap_or_default();
            return Err(QuantinuumError::AuthFailed(msg));
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            return Err(QuantinuumError::ApiError { status, message });
        }

        let data: LoginResponse = resp.json().await?;

        {
            let mut tok = self.token.lock().await;
            *tok = Some(data.id_token.clone());
        }
        {
            let mut rt = self.refresh_token.lock().await;
            rt.clone_from(&data.refresh_token);
        }

        debug!("Quantinuum login successful");
        Ok(())
    }

    /// Return the current JWT, logging in first if not yet authenticated.
    async fn ensure_token(&self) -> QuantinuumResult<String> {
        {
            let tok = self.token.lock().await;
            if let Some(ref t) = *tok {
                return Ok(t.clone());
            }
        }
        self.login().await?;
        let tok = self.token.lock().await;
        tok.clone()
            .ok_or_else(|| QuantinuumError::AuthFailed("Login did not return a token".into()))
    }

    /// Build the `Authorization` header value.
    ///
    /// Quantinuum uses `Authorization: <id-token>` (no "Bearer" prefix).
    async fn auth_header(&self) -> QuantinuumResult<String> {
        self.ensure_token().await
    }

    /// Perform a GET request; re-authenticate once on 401.
    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> QuantinuumResult<T> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let token = self.auth_header().await?;

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &token)
            .send()
            .await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            // Re-authenticate once and retry.
            {
                let mut tok = self.token.lock().await;
                *tok = None;
            }
            let new_token = self.auth_header().await?;
            let resp = self
                .client
                .get(&url)
                .header("Authorization", &new_token)
                .send()
                .await?;
            return self.handle_response(resp).await;
        }

        self.handle_response(resp).await
    }

    /// Perform a POST request with a JSON body; re-authenticate once on 401.
    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> QuantinuumResult<T> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let token = self.auth_header().await?;

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &token)
            .json(body)
            .send()
            .await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            {
                let mut tok = self.token.lock().await;
                *tok = None;
            }
            let new_token = self.auth_header().await?;
            let resp = self
                .client
                .post(&url)
                .header("Authorization", &new_token)
                .json(body)
                .send()
                .await?;
            return self.handle_response(resp).await;
        }

        self.handle_response(resp).await
    }

    /// Perform a POST with no body (e.g. cancel); re-authenticate once on 401.
    async fn post_empty(&self, path: &str) -> QuantinuumResult<()> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let token = self.auth_header().await?;

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &token)
            .send()
            .await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            {
                let mut tok = self.token.lock().await;
                *tok = None;
            }
            let new_token = self.auth_header().await?;
            let resp = self
                .client
                .post(&url)
                .header("Authorization", &new_token)
                .send()
                .await?;
            return if resp.status().is_success() {
                Ok(())
            } else {
                let status = resp.status().as_u16();
                let message = resp.text().await.unwrap_or_default();
                Err(QuantinuumError::ApiError { status, message })
            };
        }

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let message = resp.text().await.unwrap_or_default();
            Err(QuantinuumError::ApiError { status, message })
        }
    }

    /// Handle HTTP response: deserialize JSON or return an error.
    async fn handle_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> QuantinuumResult<T> {
        let status = response.status();

        if status.is_success() {
            let body = response.json().await?;
            Ok(body)
        } else {
            let message = response.text().await.unwrap_or_default();
            match status {
                StatusCode::UNAUTHORIZED => Err(QuantinuumError::AuthFailed(message)),
                StatusCode::NOT_FOUND => Err(QuantinuumError::JobNotFound(message)),
                _ => Err(QuantinuumError::ApiError {
                    status: status.as_u16(),
                    message,
                }),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Public API methods
    // -----------------------------------------------------------------------

    /// Submit a circuit for execution.
    #[instrument(skip(self, req))]
    pub async fn submit_job(&self, req: &JobRequest) -> QuantinuumResult<JobResponse> {
        debug!("Submitting job to machine {}", req.machine);
        self.post("job", req).await
    }

    /// Get the current status (and results, if completed) of a job.
    #[instrument(skip(self))]
    pub async fn get_job(&self, job_id: &str) -> QuantinuumResult<JobStatusResponse> {
        debug!("Getting job status for {}", job_id);
        self.get(&format!("job/{job_id}")).await
    }

    /// Cancel a queued or running job.
    #[instrument(skip(self))]
    pub async fn cancel_job(&self, job_id: &str) -> QuantinuumResult<()> {
        debug!("Cancelling job {}", job_id);
        self.post_empty(&format!("job/{job_id}/cancel")).await
    }

    /// List available machines with their configuration.
    #[instrument(skip(self))]
    pub async fn list_machines(&self) -> QuantinuumResult<Vec<MachineInfo>> {
        debug!("Listing Quantinuum machines");
        self.get("machine/?config=true").await
    }

    /// Get a specific machine's status.
    #[instrument(skip(self))]
    pub async fn get_machine(&self, machine: &str) -> QuantinuumResult<MachineInfo> {
        debug!("Getting machine info for {}", machine);
        self.get(&format!("machine/{machine}")).await
    }
}

// ---------------------------------------------------------------------------
// Request / response serde types
// ---------------------------------------------------------------------------

/// Response from `POST /login`.
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    /// JWT id-token used in all subsequent requests.
    #[serde(rename = "id-token")]
    pub id_token: String,
    /// Refresh token for re-authentication.
    #[serde(rename = "refresh-token")]
    pub refresh_token: Option<String>,
}

/// Request body for `POST /job`.
#[derive(Debug, Serialize)]
pub struct JobRequest {
    /// Human-readable job name.
    pub name: String,
    /// Number of shots.
    pub count: u32,
    /// Target machine (e.g. `"H2-1LE"`).
    pub machine: String,
    /// Circuit language — must be `"OPENQASM 2.0"`.
    pub language: String,
    /// Circuit program in the specified language.
    pub program: String,
    /// Optional job options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<JobOptions>,
}

impl JobRequest {
    /// Create a new QASM 2.0 job request.
    pub fn new(machine: impl Into<String>, program: impl Into<String>, count: u32) -> Self {
        Self {
            name: format!("arvak-{}", uuid::Uuid::new_v4()),
            count,
            machine: machine.into(),
            language: "OPENQASM 2.0".into(),
            program: program.into(),
            options: None,
        }
    }

    /// Set job options.
    pub fn with_options(mut self, options: JobOptions) -> Self {
        self.options = Some(options);
        self
    }
}

/// Optional job execution options.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct JobOptions {
    /// Use state-vector simulator (for emulator targets).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simulator: Option<String>,
    /// Disable server-side optimisation.
    #[serde(rename = "no-opt", skip_serializing_if = "Option::is_none")]
    pub no_opt: Option<bool>,
    /// Server-side tket optimisation level (0–2).
    #[serde(rename = "tket-opt-level", skip_serializing_if = "Option::is_none")]
    pub tket_opt_level: Option<u8>,
}

/// Response from `POST /job`.
#[derive(Debug, Deserialize)]
pub struct JobResponse {
    /// Assigned job identifier.
    pub job: String,
}

/// Response from `GET /job/{id}`.
///
/// Also serves as the long-poll status response — the `results` field is
/// populated once `status == "completed"`.
#[derive(Debug, Deserialize)]
pub struct JobStatusResponse {
    /// Job identifier.
    pub job: Option<String>,
    /// Job name.
    #[serde(default)]
    pub name: Option<String>,
    /// Current status: `queued`, `submitted`, `running`, `completed`,
    /// `failed`, `canceled`, `cancelling`.
    pub status: String,
    /// Per-register measurement results, populated when completed.
    ///
    /// Keys are register names (e.g. `"c_0"`, `"c_1"`); values are
    /// arrays of per-shot bit values (0 or 1) indexed by shot index.
    #[serde(default)]
    pub results: Option<HashMap<String, Vec<u8>>>,
    /// Error message if `status == "failed"`.
    #[serde(default)]
    pub error: Option<String>,
    /// Queue position (present while queued).
    #[serde(rename = "queue-position", default)]
    pub queue_position: Option<u32>,
}

impl JobStatusResponse {
    /// Return the job ID from either the `job` field (submit response stores
    /// it under `job`, poll responses may also use `job`).
    pub fn id(&self) -> &str {
        self.job.as_deref().unwrap_or("")
    }

    /// Whether the job is still pending (not yet terminal).
    pub fn is_pending(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "queued" | "submitted" | "running" | "cancelling"
        )
    }

    /// Whether the job completed successfully.
    pub fn is_completed(&self) -> bool {
        self.status.to_lowercase() == "completed"
    }

    /// Whether the job failed.
    pub fn is_failed(&self) -> bool {
        self.status.to_lowercase() == "failed"
    }

    /// Whether the job was cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(
            self.status.to_lowercase().as_str(),
            "canceled" | "cancelled"
        )
    }
}

/// Machine information returned by `GET /machine/?config=true`.
#[derive(Debug, Clone, Deserialize)]
pub struct MachineInfo {
    /// Machine name.
    pub name: String,
    /// Number of available qubits.
    #[serde(rename = "n_qubits", default)]
    pub num_qubits: Option<u32>,
    /// Maximum shots per job.
    #[serde(rename = "n_shots", default)]
    pub max_shots: Option<u32>,
    /// Current status string (e.g. `"online"`, `"offline"`).
    #[serde(default)]
    pub status: Option<String>,
    /// Whether this is a simulator/emulator.
    #[serde(rename = "system_type", default)]
    pub system_type: Option<String>,
    /// Emulator name associated with this hardware machine.
    #[serde(default)]
    pub emulator: Option<String>,
}

impl MachineInfo {
    /// Whether the machine is currently online.
    pub fn is_online(&self) -> bool {
        self.status
            .as_deref()
            .is_some_and(|s| matches!(s.to_lowercase().as_str(), "online" | "available" | "ready"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_request_serialization() {
        let req = JobRequest::new("H2-1LE", "OPENQASM 2.0;\nqreg q[2];\n", 1000);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("H2-1LE"));
        assert!(json.contains("OPENQASM 2.0"));
        assert!(json.contains("1000"));
    }

    #[test]
    fn test_job_status_response_states() {
        let mut resp = JobStatusResponse {
            job: Some("abc".into()),
            name: None,
            status: "running".into(),
            results: None,
            error: None,
            queue_position: None,
        };
        assert!(resp.is_pending());
        assert!(!resp.is_completed());

        resp.status = "completed".into();
        assert!(resp.is_completed());
        assert!(!resp.is_pending());

        resp.status = "failed".into();
        assert!(resp.is_failed());

        resp.status = "canceled".into();
        assert!(resp.is_cancelled());
    }

    #[test]
    fn test_machine_info_is_online() {
        let online = MachineInfo {
            name: "H2-1".into(),
            num_qubits: Some(32),
            max_shots: Some(10_000),
            status: Some("online".into()),
            system_type: Some("hardware".into()),
            emulator: Some("H2-1E".into()),
        };
        assert!(online.is_online());

        let offline = MachineInfo {
            name: "H2-1".into(),
            num_qubits: Some(32),
            max_shots: Some(10_000),
            status: Some("offline".into()),
            system_type: Some("hardware".into()),
            emulator: Some("H2-1E".into()),
        };
        assert!(!offline.is_online());
    }
}
