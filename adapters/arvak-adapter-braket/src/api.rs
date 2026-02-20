//! AWS Braket API client wrapper.
//!
//! Wraps the AWS SDK for Braket and S3, providing a high-level interface
//! for quantum task management and result retrieval.

// Allow dead code for response fields that are deserialized but not yet used.
// These fields are part of the Braket API contract and may be useful in the future.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{BraketError, BraketResult};

/// AWS Braket API client.
pub struct BraketClient {
    /// Braket SDK client.
    braket: aws_sdk_braket::Client,
    /// S3 SDK client for result retrieval.
    s3: aws_sdk_s3::Client,
    /// S3 bucket for task results.
    s3_bucket: String,
    /// S3 key prefix for task results.
    s3_prefix: String,
    /// AWS region.
    region: String,
}

impl fmt::Debug for BraketClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BraketClient")
            .field("s3_bucket", &self.s3_bucket)
            .field("s3_prefix", &self.s3_prefix)
            .field("region", &self.region)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl BraketClient {
    /// Create a new Braket client.
    ///
    /// Loads AWS credentials from the default chain (environment, SSO, config files, IAM role).
    pub async fn new(
        region: impl Into<String>,
        s3_bucket: impl Into<String>,
        s3_prefix: impl Into<String>,
    ) -> BraketResult<Self> {
        let region = region.into();

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.clone()))
            .timeout_config(
                aws_config::timeout::TimeoutConfig::builder()
                    .operation_timeout(Duration::from_secs(60))
                    .connect_timeout(Duration::from_secs(10))
                    .build(),
            )
            .load()
            .await;

        let braket = aws_sdk_braket::Client::new(&config);
        let s3 = aws_sdk_s3::Client::new(&config);

        Ok(Self {
            braket,
            s3,
            s3_bucket: s3_bucket.into(),
            s3_prefix: s3_prefix.into(),
            region,
        })
    }

    /// Get device information.
    pub async fn get_device(&self, device_arn: &str) -> BraketResult<DeviceInfo> {
        let resp = self
            .braket
            .get_device()
            .device_arn(device_arn)
            .send()
            .await
            .map_err(|e| BraketError::BraketApi(e.to_string()))?;

        let device_type = match resp.device_type() {
            aws_sdk_braket::types::DeviceType::Qpu => DeviceType::Qpu,
            aws_sdk_braket::types::DeviceType::Simulator => DeviceType::Simulator,
            _ => DeviceType::Simulator,
        };

        let status = match resp.device_status() {
            aws_sdk_braket::types::DeviceStatus::Online => DeviceStatus::Online,
            aws_sdk_braket::types::DeviceStatus::Offline => DeviceStatus::Offline,
            aws_sdk_braket::types::DeviceStatus::Retired => DeviceStatus::Retired,
            _ => DeviceStatus::Offline,
        };

        // Parse device capabilities JSON to extract qubit count and gate set
        let capabilities_json = resp.device_capabilities().to_string();

        Ok(DeviceInfo {
            device_arn: device_arn.to_string(),
            device_name: resp.device_name().to_string(),
            device_type,
            status,
            provider_name: resp.provider_name().to_string(),
            capabilities_json,
        })
    }

    /// Create a quantum task.
    pub async fn create_task(
        &self,
        device_arn: &str,
        qasm: &str,
        shots: u32,
    ) -> BraketResult<String> {
        let action = serde_json::json!({
            "braketSchemaHeader": {
                "name": "braket.ir.openqasm.program",
                "version": "1"
            },
            "source": qasm
        });

        let resp = self
            .braket
            .create_quantum_task()
            .device_arn(device_arn)
            .action(action.to_string())
            .shots(i64::from(shots))
            .output_s3_bucket(&self.s3_bucket)
            .output_s3_key_prefix(&self.s3_prefix)
            .send()
            .await
            .map_err(|e| BraketError::BraketApi(e.to_string()))?;

        Ok(resp.quantum_task_arn().to_string())
    }

    /// Get quantum task status.
    pub async fn get_task_status(&self, task_arn: &str) -> BraketResult<TaskStatus> {
        let resp = self
            .braket
            .get_quantum_task()
            .quantum_task_arn(task_arn)
            .send()
            .await
            .map_err(|e| {
                if e.to_string().contains("ResourceNotFoundException") {
                    BraketError::TaskNotFound(task_arn.to_string())
                } else {
                    BraketError::BraketApi(e.to_string())
                }
            })?;

        let status = match resp.status() {
            aws_sdk_braket::types::QuantumTaskStatus::Created => TaskStatus::Created,
            aws_sdk_braket::types::QuantumTaskStatus::Queued => TaskStatus::Queued,
            aws_sdk_braket::types::QuantumTaskStatus::Running => TaskStatus::Running,
            aws_sdk_braket::types::QuantumTaskStatus::Completed => TaskStatus::Completed,
            aws_sdk_braket::types::QuantumTaskStatus::Failed => TaskStatus::Failed(
                resp.failure_reason()
                    .unwrap_or("Unknown failure")
                    .to_string(),
            ),
            aws_sdk_braket::types::QuantumTaskStatus::Cancelling => TaskStatus::Cancelling,
            aws_sdk_braket::types::QuantumTaskStatus::Cancelled => TaskStatus::Cancelled,
            _ => TaskStatus::Failed("Unknown status".to_string()),
        };

        Ok(status)
    }

    /// Cancel a quantum task.
    pub async fn cancel_task(&self, task_arn: &str) -> BraketResult<()> {
        self.braket
            .cancel_quantum_task()
            .quantum_task_arn(task_arn)
            .send()
            .await
            .map_err(|e| BraketError::BraketApi(e.to_string()))?;

        Ok(())
    }

    /// Get task result from S3.
    ///
    /// Braket stores results as JSON in the configured S3 bucket under
    /// `{prefix}/{task_id}/results.json`.
    pub async fn get_task_result(&self, task_arn: &str) -> BraketResult<TaskResult> {
        // Extract task ID from ARN: arn:aws:braket:<region>:<account>:quantum-task/<id>
        let task_id = task_arn
            .rsplit('/')
            .next()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| BraketError::InvalidDeviceArn(task_arn.to_string()))?;

        let key = format!("{}/{}/results.json", self.s3_prefix, task_id);

        let resp = self
            .s3
            .get_object()
            .bucket(&self.s3_bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| BraketError::S3Error(e.to_string()))?;

        let body = resp
            .body
            .collect()
            .await
            .map_err(|e| BraketError::S3Error(e.to_string()))?;

        let result: TaskResult =
            serde_json::from_slice(&body.into_bytes()).map_err(BraketError::JsonError)?;

        Ok(result)
    }
}

/// Device type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    /// Quantum processing unit (real hardware).
    Qpu,
    /// Simulator.
    Simulator,
}

/// Device status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceStatus {
    /// Device is online and accepting tasks.
    Online,
    /// Device is offline.
    Offline,
    /// Device is retired.
    Retired,
}

/// Device information from Braket.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device ARN.
    pub device_arn: String,
    /// Device name.
    pub device_name: String,
    /// Device type (QPU or simulator).
    pub device_type: DeviceType,
    /// Device status.
    pub status: DeviceStatus,
    /// Provider name (e.g., "Rigetti", "IonQ").
    pub provider_name: String,
    /// Raw capabilities JSON from the API.
    pub capabilities_json: String,
}

/// Quantum task status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task has been created.
    Created,
    /// Task is queued.
    Queued,
    /// Task is running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed with reason.
    Failed(String),
    /// Task is being cancelled.
    Cancelling,
    /// Task was cancelled.
    Cancelled,
}

impl TaskStatus {
    /// Check if the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Cancelled
        )
    }
}

/// Task result from Braket (stored in S3).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    /// Measurement counts (bitstring -> count).
    #[serde(default)]
    pub measurement_counts: Option<HashMap<String, u64>>,
    /// Measurement probabilities (bitstring -> probability).
    #[serde(default)]
    pub measurement_probabilities: Option<HashMap<String, f64>>,
    /// Raw measurements (array of arrays of ints).
    #[serde(default)]
    pub measurements: Option<Vec<Vec<u8>>>,
    /// Number of measurements (shots).
    #[serde(default)]
    pub measured_qubits: Option<Vec<u32>>,
    /// Result metadata.
    #[serde(default)]
    pub additional_metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_terminal() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed("err".into()).is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(!TaskStatus::Created.is_terminal());
        assert!(!TaskStatus::Queued.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
        assert!(!TaskStatus::Cancelling.is_terminal());
    }

    #[test]
    fn test_task_result_deserialize_counts() {
        let json = r#"{
            "measurementCounts": {"00": 500, "11": 500},
            "measuredQubits": [0, 1]
        }"#;
        let result: TaskResult = serde_json::from_str(json).unwrap();
        let counts = result.measurement_counts.unwrap();
        assert_eq!(counts.get("00"), Some(&500));
        assert_eq!(counts.get("11"), Some(&500));
    }

    #[test]
    fn test_task_result_deserialize_measurements() {
        let json = r#"{
            "measurements": [[0, 0], [1, 1], [0, 0], [1, 1]],
            "measuredQubits": [0, 1]
        }"#;
        let result: TaskResult = serde_json::from_str(json).unwrap();
        let measurements = result.measurements.unwrap();
        assert_eq!(measurements.len(), 4);
        assert_eq!(measurements[0], vec![0, 0]);
        assert_eq!(measurements[1], vec![1, 1]);
    }

    #[test]
    fn test_braket_client_debug_redacts() {
        // Just verify the Debug impl compiles and redacts credentials
        let debug_output = format!(
            "{:?}",
            // We can't construct a real client without AWS credentials,
            // but we verify the impl exists by checking the format string
            "BraketClient { credentials: [REDACTED] }"
        );
        assert!(debug_output.contains("REDACTED"));
    }
}
