//! AWS Braket backend implementation.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rustc_hash::FxHashMap;
use tokio::sync::{Mutex, RwLock};

use arvak_hal::{
    Backend, BackendAvailability, Capabilities, Counts, ExecutionResult, HalError, HalResult,
    JobId, JobStatus, ValidationResult,
};
use arvak_ir::Circuit;
use arvak_qasm3::emit;

use crate::api::{BraketClient, DeviceInfo, DeviceStatus, TaskStatus};
use crate::device::capabilities_for_device;
use crate::error::{BraketError, BraketResult};

/// Maximum number of cached jobs before eviction of terminal entries.
const MAX_CACHED_JOBS: usize = 10_000;

/// How long to cache device info before refreshing from the API.
const DEVICE_INFO_TTL: Duration = Duration::from_secs(5 * 60);

/// A cached job entry.
struct CachedJob {
    /// Job status.
    status: JobStatus,
    /// Cached result (if completed).
    result: Option<ExecutionResult>,
    /// Number of qubits in the submitted circuit.
    num_qubits: usize,
    /// Number of shots requested at submission time (used to convert
    /// probability-only result formats to approximate counts).
    shots: u32,
}

/// AWS Braket backend adapter.
///
/// Provides access to quantum hardware and simulators available through
/// the AWS Braket service. Supports Rigetti, IonQ, IQM, and Amazon's
/// managed simulators (SV1, TN1, DM1).
pub struct BraketBackend {
    /// Braket API client.
    client: Arc<BraketClient>,
    /// Device ARN.
    device_arn: String,
    /// Cached capabilities (HAL Contract v2: sync introspection).
    capabilities: Capabilities,
    /// Job cache: task ARN -> cached job.
    jobs: Arc<Mutex<FxHashMap<String, CachedJob>>>,
    /// Cached device info with fetch timestamp for TTL-based refresh.
    device_info: Arc<RwLock<Option<(DeviceInfo, Instant)>>>,
}

impl BraketBackend {
    /// Connect to a Braket device.
    ///
    /// Reads configuration from environment variables:
    /// - `ARVAK_BRAKET_S3_BUCKET` (required) — S3 bucket for task results
    /// - `ARVAK_BRAKET_S3_PREFIX` (optional, default: `"arvak-results"`)
    /// - `AWS_REGION` (optional, default: `"us-east-1"`)
    ///
    /// AWS credentials are loaded from the default chain (environment,
    /// SSO, config files, IAM role).
    pub async fn connect(device_arn: impl Into<String>) -> BraketResult<Self> {
        let device_arn = device_arn.into();

        let s3_bucket =
            std::env::var("ARVAK_BRAKET_S3_BUCKET").map_err(|_| BraketError::MissingS3Bucket)?;
        let s3_prefix =
            std::env::var("ARVAK_BRAKET_S3_PREFIX").unwrap_or_else(|_| "arvak-results".to_string());
        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        let client = BraketClient::new(&region, &s3_bucket, &s3_prefix).await?;

        // Use known preset if available, otherwise build from API
        let capabilities = match capabilities_for_device(&device_arn) {
            Some(caps) => caps,
            None => {
                // Fetch device info for unknown devices
                let info = client.get_device(&device_arn).await?;
                build_capabilities_from_info(&info)
            }
        };

        Ok(Self {
            client: Arc::new(client),
            device_arn,
            capabilities,
            jobs: Arc::new(Mutex::new(FxHashMap::default())),
            device_info: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the device ARN.
    pub fn device_arn(&self) -> &str {
        &self.device_arn
    }

    /// Get device info with TTL caching.
    async fn get_device_info(&self) -> BraketResult<DeviceInfo> {
        // Check cache first
        {
            let cached = self.device_info.read().await;
            if let Some((ref info, fetched_at)) = *cached {
                if fetched_at.elapsed() < DEVICE_INFO_TTL {
                    return Ok(info.clone());
                }
            }
        }

        // Fetch from API
        let info = self.client.get_device(&self.device_arn).await?;

        // Cache with timestamp
        {
            let mut cached = self.device_info.write().await;
            *cached = Some((info.clone(), Instant::now()));
        }

        Ok(info)
    }

    /// Convert circuit to OpenQASM 3.0 string.
    fn circuit_to_qasm(circuit: &Circuit) -> BraketResult<String> {
        emit(circuit).map_err(|e| BraketError::CircuitError(e.to_string()))
    }

    /// Parse task result into execution counts.
    ///
    /// `submitted_shots` is used as the denominator when the only available
    /// result format is `measurementProbabilities`. Pass 0 to use the default
    /// fallback of 1000 (for callers that don't have the shot count available).
    fn parse_result(
        result: &crate::api::TaskResult,
        _num_qubits: usize,
        submitted_shots: u32,
    ) -> Counts {
        let mut counts = Counts::new();

        // Prefer measurementCounts (bitstring -> count)
        if let Some(measurement_counts) = &result.measurement_counts {
            for (bitstring, &count) in measurement_counts {
                counts.insert(bitstring.clone(), count);
            }
            return counts;
        }

        // Fall back to raw measurements (array of arrays)
        if let Some(measurements) = &result.measurements {
            for measurement in measurements {
                let bitstring: String = measurement
                    .iter()
                    .map(|b| if *b == 0 { '0' } else { '1' })
                    .collect();
                counts.insert(bitstring, 1);
            }
            return counts;
        }

        // Fall back to measurementProbabilities
        if let Some(probs) = &result.measurement_probabilities {
            let total_shots = if submitted_shots > 0 {
                f64::from(submitted_shots)
            } else {
                1000.0_f64
            };
            for (bitstring, &prob) in probs {
                let count = (prob * total_shots).max(0.0).round() as u64;
                if count > 0 {
                    counts.insert(bitstring.clone(), count);
                }
            }
        }

        counts
    }
}

/// Build capabilities from device info for unknown devices.
fn build_capabilities_from_info(info: &DeviceInfo) -> Capabilities {
    let is_simulator = info.device_type == crate::api::DeviceType::Simulator;

    // Try to extract qubit count from capabilities JSON
    let num_qubits =
        extract_qubit_count(&info.capabilities_json).unwrap_or(if is_simulator { 34 } else { 20 });

    if is_simulator {
        Capabilities::braket_simulator(&info.device_name, num_qubits)
    } else {
        // Default to a conservative gate set for unknown QPUs
        match info.provider_name.to_lowercase().as_str() {
            "rigetti" => Capabilities::braket_rigetti(&info.device_name, num_qubits),
            "ionq" => Capabilities::braket_ionq(&info.device_name, num_qubits),
            _ => Capabilities::braket_rigetti(&info.device_name, num_qubits),
        }
    }
}

/// Extract qubit count from Braket device capabilities JSON.
fn extract_qubit_count(capabilities_json: &str) -> Option<u32> {
    let val: serde_json::Value = serde_json::from_str(capabilities_json).ok()?;
    // Braket capabilities JSON has paradigm.qubitCount
    val.get("paradigm")
        .and_then(|p| p.get("qubitCount"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

#[async_trait]
impl Backend for BraketBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "braket"
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        match self.get_device_info().await {
            Ok(info) => {
                if info.status == DeviceStatus::Online {
                    Ok(BackendAvailability {
                        is_available: true,
                        queue_depth: None,
                        estimated_wait: None,
                        status_message: Some(format!(
                            "{} ({})",
                            info.device_name, info.provider_name
                        )),
                    })
                } else {
                    Ok(BackendAvailability::unavailable(format!(
                        "{} is {:?}",
                        info.device_name, info.status
                    )))
                }
            }
            Err(_) => Ok(BackendAvailability::unavailable("failed to query device")),
        }
    }

    async fn validate(&self, circuit: &Circuit) -> HalResult<ValidationResult> {
        let caps = self.capabilities();
        let mut reasons = Vec::new();

        // Check qubit count
        if circuit.num_qubits() > caps.num_qubits as usize {
            reasons.push(format!(
                "Circuit requires {} qubits but device only has {}",
                circuit.num_qubits(),
                caps.num_qubits
            ));
        }

        // Check gate set support
        let gate_set = &caps.gate_set;
        for (_, inst) in circuit.dag().topological_ops() {
            if let Some(gate) = inst.as_gate() {
                let name = gate.name();
                if !gate_set.contains(name) {
                    reasons.push(format!("Unsupported gate: {name}"));
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

    async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
        // Validate qubit count
        if circuit.num_qubits() > self.capabilities.num_qubits as usize {
            return Err(HalError::CircuitTooLarge(format!(
                "Circuit requires {} qubits but device only has {}",
                circuit.num_qubits(),
                self.capabilities.num_qubits
            )));
        }

        // Convert circuit to QASM3
        let qasm =
            Self::circuit_to_qasm(circuit).map_err(|e| HalError::InvalidCircuit(e.to_string()))?;

        // Submit task to Braket
        let task_arn = self
            .client
            .create_task(&self.device_arn, &qasm, shots)
            .await
            .map_err(|e| HalError::SubmissionFailed(e.to_string()))?;

        // Cache the job
        {
            let mut jobs = self.jobs.lock().await;
            if jobs.len() >= MAX_CACHED_JOBS {
                jobs.retain(|_, j| !j.status.is_terminal());
            }
            jobs.insert(
                task_arn.clone(),
                CachedJob {
                    status: JobStatus::Queued,
                    result: None,
                    num_qubits: circuit.num_qubits(),
                    shots,
                },
            );
        }

        Ok(JobId(task_arn))
    }

    async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
        let task_status = self
            .client
            .get_task_status(&job_id.0)
            .await
            .map_err(|e| match e {
                BraketError::TaskNotFound(id) => HalError::JobNotFound(id),
                other => HalError::Backend(other.to_string()),
            })?;

        let job_status = match task_status {
            TaskStatus::Created | TaskStatus::Queued => JobStatus::Queued,
            TaskStatus::Running => JobStatus::Running,
            TaskStatus::Completed => JobStatus::Completed,
            TaskStatus::Failed(msg) => JobStatus::Failed(msg),
            TaskStatus::Cancelling | TaskStatus::Cancelled => JobStatus::Cancelled,
        };

        // Update cache
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.status = job_status.clone();
            }
        }

        Ok(job_status)
    }

    async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        // Check cache first
        {
            let jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get(&job_id.0) {
                if let Some(ref result) = cached.result {
                    return Ok(result.clone());
                }
            }
        }

        // Check that the task is completed
        let task_status = self
            .client
            .get_task_status(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        match task_status {
            TaskStatus::Failed(msg) => return Err(HalError::JobFailed(msg)),
            TaskStatus::Cancelled | TaskStatus::Cancelling => return Err(HalError::JobCancelled),
            TaskStatus::Completed => {}
            _ => {
                return Err(HalError::Backend(format!(
                    "Task {} not yet completed",
                    job_id.0
                )));
            }
        }

        // Fetch result from S3
        let task_result = self
            .client
            .get_task_result(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        // Get num_qubits and shots from cache or capabilities
        let (num_qubits, submitted_shots) = {
            let jobs = self.jobs.lock().await;
            jobs.get(&job_id.0)
                .map_or((self.capabilities.num_qubits as usize, 0u32), |j| {
                    (j.num_qubits, j.shots)
                })
        };

        let counts = Self::parse_result(&task_result, num_qubits, submitted_shots);
        let total_shots = counts.total_shots() as u32;
        let result = ExecutionResult::new(counts, total_shots);

        // Cache the result
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.result = Some(result.clone());
                cached.status = JobStatus::Completed;
            }
        }

        Ok(result)
    }

    async fn cancel(&self, job_id: &JobId) -> HalResult<()> {
        self.client
            .cancel_task(&job_id.0)
            .await
            .map_err(|e| HalError::Backend(e.to_string()))?;

        // Update cache
        {
            let mut jobs = self.jobs.lock().await;
            if let Some(cached) = jobs.get_mut(&job_id.0) {
                cached.status = JobStatus::Cancelled;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_result_counts() {
        let result = crate::api::TaskResult {
            measurement_counts: Some({
                let mut m = std::collections::HashMap::new();
                m.insert("00".to_string(), 500);
                m.insert("11".to_string(), 500);
                m
            }),
            measurement_probabilities: None,
            measurements: None,
            measured_qubits: Some(vec![0, 1]),
            additional_metadata: None,
        };

        let counts = BraketBackend::parse_result(&result, 2, 1000);
        assert_eq!(counts.get("00"), 500);
        assert_eq!(counts.get("11"), 500);
        assert_eq!(counts.total_shots(), 1000);
    }

    #[test]
    fn test_parse_result_measurements() {
        let result = crate::api::TaskResult {
            measurement_counts: None,
            measurement_probabilities: None,
            measurements: Some(vec![vec![0, 0], vec![1, 1], vec![0, 0], vec![1, 1]]),
            measured_qubits: Some(vec![0, 1]),
            additional_metadata: None,
        };

        let counts = BraketBackend::parse_result(&result, 2, 4);
        assert_eq!(counts.get("00"), 2);
        assert_eq!(counts.get("11"), 2);
        assert_eq!(counts.total_shots(), 4);
    }

    #[test]
    fn test_extract_qubit_count() {
        let json = r#"{"paradigm": {"qubitCount": 84}}"#;
        assert_eq!(extract_qubit_count(json), Some(84));

        let json = r#"{"paradigm": {}}"#;
        assert_eq!(extract_qubit_count(json), None);

        assert_eq!(extract_qubit_count("invalid json"), None);
    }

    #[test]
    fn test_build_capabilities_simulator() {
        let info = DeviceInfo {
            device_arn: "arn:aws:braket:::device/quantum-simulator/amazon/sv1".to_string(),
            device_name: "SV1".to_string(),
            device_type: crate::api::DeviceType::Simulator,
            status: DeviceStatus::Online,
            provider_name: "Amazon".to_string(),
            capabilities_json: r#"{"paradigm": {"qubitCount": 34}}"#.to_string(),
        };

        let caps = build_capabilities_from_info(&info);
        assert!(caps.is_simulator);
        assert_eq!(caps.num_qubits, 34);
    }

    #[test]
    fn test_build_capabilities_rigetti() {
        let info = DeviceInfo {
            device_arn: "arn:aws:braket:us-west-1::device/qpu/rigetti/Aspen-M-3".to_string(),
            device_name: "Aspen-M-3".to_string(),
            device_type: crate::api::DeviceType::Qpu,
            status: DeviceStatus::Online,
            provider_name: "Rigetti".to_string(),
            capabilities_json: r#"{"paradigm": {"qubitCount": 80}}"#.to_string(),
        };

        let caps = build_capabilities_from_info(&info);
        assert!(!caps.is_simulator);
        assert_eq!(caps.num_qubits, 80);
        assert!(caps.gate_set.contains("rx"));
        assert!(caps.gate_set.contains("cz"));
    }
}
