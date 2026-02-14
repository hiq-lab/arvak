//! IBM Quantum backend implementation.

use std::sync::Arc;
use tokio::sync::RwLock;

use arvak_hal::{
    Backend, BackendAvailability, BackendConfig, Capabilities, Counts, ExecutionResult, HalError,
    HalResult, JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;
use arvak_qasm3::emit;
use async_trait::async_trait;

use crate::api::{BackendInfo, DEFAULT_ENDPOINT, IbmClient};
use crate::error::{IbmError, IbmResult};

/// Default IBM Quantum backend (simulator).
const DEFAULT_BACKEND: &str = "ibmq_qasm_simulator";

/// IBM Quantum backend adapter.
pub struct IbmBackend {
    /// API client.
    client: Arc<IbmClient>,
    /// Target backend name.
    target: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Cached backend info.
    backend_info: Arc<RwLock<Option<BackendInfo>>>,
}

impl IbmBackend {
    /// Create a new IBM backend with default settings.
    ///
    /// Reads the API token from the `IBM_QUANTUM_TOKEN` environment variable.
    pub fn new() -> IbmResult<Self> {
        let token = std::env::var("IBM_QUANTUM_TOKEN").map_err(|_| IbmError::MissingToken)?;

        let client = IbmClient::new(DEFAULT_ENDPOINT, &token)?;
        let target = DEFAULT_BACKEND.to_string();

        Ok(Self {
            client: Arc::new(client),
            capabilities: Capabilities::ibm(&target, 127),
            target,
            backend_info: Arc::new(RwLock::new(None)),
        })
    }

    /// Create a backend targeting a specific IBM Quantum device.
    pub fn with_target(target: impl Into<String>) -> IbmResult<Self> {
        let token = std::env::var("IBM_QUANTUM_TOKEN").map_err(|_| IbmError::MissingToken)?;

        let client = IbmClient::new(DEFAULT_ENDPOINT, &token)?;
        let target = target.into();

        Ok(Self {
            client: Arc::new(client),
            capabilities: Capabilities::ibm(&target, 127),
            target,
            backend_info: Arc::new(RwLock::new(None)),
        })
    }

    /// Create a backend with explicit configuration.
    pub fn with_config(config: BackendConfig) -> IbmResult<Self> {
        let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_ENDPOINT);

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
            capabilities: Capabilities::ibm(target, 127),
            target: target.to_string(),
            backend_info: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the target backend name.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Get backend information, fetching from API if not cached.
    async fn get_backend_info(&self) -> IbmResult<BackendInfo> {
        // Check cache first
        {
            let cached = self.backend_info.read().await;
            if let Some(info) = cached.as_ref() {
                return Ok(info.clone());
            }
        }

        // Fetch from API
        let info = self.client.get_backend(&self.target).await?;

        // Cache it
        {
            let mut cached = self.backend_info.write().await;
            *cached = Some(info.clone());
        }

        Ok(info)
    }

    /// Convert circuit to `OpenQASM` 3.0 string.
    fn circuit_to_qasm(circuit: &Circuit) -> IbmResult<String> {
        emit(circuit).map_err(|e| IbmError::CircuitError(e.to_string()))
    }

    /// Convert measurement results to counts.
    fn results_to_counts(results: &crate::api::JobResultResponse) -> Counts {
        let mut counts = Counts::new();

        // Handle sampler results
        if let Some(result) = results.results.first() {
            // Try counts first (more accurate)
            if let Some(raw_counts) = &result.counts {
                for (bitstring, &count) in raw_counts {
                    // IBM returns hex strings, convert to binary
                    let binary = hex_to_binary(bitstring);
                    counts.insert(binary, count);
                }
            }
            // Fall back to quasi-distributions
            else if let Some(quasi_dists) = &result.quasi_dists {
                if let Some(dist) = quasi_dists.first() {
                    for (bitstring, &prob) in dist {
                        let binary = hex_to_binary(bitstring);
                        // TODO: Use actual shot count from job submission instead of hardcoded 1000.0
                        // Clamp negative quasi-probabilities to zero before conversion.
                        let count = (prob * 1000.0).max(0.0).round() as u64;
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

/// Convert hex string to binary string.
///
/// TODO: Leading zeros are lost because the number of qubits is not available here.
/// Callers that need fixed-width bitstrings should pad the result, e.g.
/// `format!("{:0>width$}", hex_to_binary(s), width = num_qubits)`.
fn hex_to_binary(hex: &str) -> String {
    // Handle 0x prefix
    let hex = hex.strip_prefix("0x").unwrap_or(hex);

    // Parse as integer and format as binary.
    // Note: this does not preserve leading zeros; the width of the original
    // hex representation is used as a heuristic (4 bits per hex digit).
    if let Ok(value) = u64::from_str_radix(hex, 16) {
        let width = hex.len() * 4;
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
            Err(_) => Ok(BackendAvailability::unavailable("failed to query backend")),
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit requires {} qubits but backend only has {}",
                circuit.num_qubits(),
                caps.num_qubits
            ));
        }

        if reasons.is_empty() {
            Ok(ValidationResult::Valid)
        } else {
            Ok(ValidationResult::Invalid { reasons })
        }
    }

    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        // Check qubit count
        let info = self
            .get_backend_info()
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        if circuit.num_qubits() > info.num_qubits {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit requires {} qubits but backend only has {}",
                circuit.num_qubits(),
                info.num_qubits
            )));
        }

        // Check if backend is operational
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
            .submit_sampler_job(&self.target, vec![qasm], shots)
            .await
            .map_err(|e| HalError::SubmissionFailed(e.to_string()))?;

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

        let job_status = match status.status.as_str() {
            "QUEUED" => JobStatus::Queued,
            "VALIDATING" | "RUNNING" => JobStatus::Running,
            "COMPLETED" => JobStatus::Completed,
            "FAILED" | "ERROR" => {
                let msg = status
                    .error
                    .map_or_else(|| "Unknown error".to_string(), |e| e.message);
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
                    .error
                    .map_or_else(|| "Job failed".to_string(), |e| e.message);
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

        let counts = Self::results_to_counts(&results);
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

    #[test]
    fn test_hex_to_binary() {
        assert_eq!(hex_to_binary("0x0"), "0000");
        assert_eq!(hex_to_binary("0x1"), "0001");
        assert_eq!(hex_to_binary("0x3"), "0011");
        assert_eq!(hex_to_binary("0xf"), "1111");
        assert_eq!(hex_to_binary("0xff"), "11111111");
        assert_eq!(hex_to_binary("3"), "0011");
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
    fn test_results_to_counts() {
        use crate::api::{JobResultResponse, SamplerResult};
        use std::collections::HashMap;

        let mut raw_counts = HashMap::new();
        raw_counts.insert("0x0".to_string(), 500u64);
        raw_counts.insert("0x3".to_string(), 500u64);

        let results = JobResultResponse {
            id: "test".to_string(),
            results: vec![SamplerResult {
                quasi_dists: None,
                counts: Some(raw_counts),
                metadata: None,
            }],
        };

        let counts = IbmBackend::results_to_counts(&results);
        assert_eq!(counts.get("0000"), 500);
        assert_eq!(counts.get("0011"), 500);
        assert_eq!(counts.total_shots(), 1000);
    }
}
