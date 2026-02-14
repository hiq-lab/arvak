//! LUMI Integration Tests
//!
//! These tests verify the integration between Arvak and LUMI's HPC environment.
//! They test the SLURM adapter with LUMI-specific configurations and the
//! OIDC authentication flow for IQM's Helmi quantum computer.
//!
//! Note: These tests use mock adapters and don't require actual LUMI access.
//! For real integration testing on LUMI, use the `--ignored` flag and ensure
//! proper authentication is set up.

use std::path::PathBuf;
use std::sync::Arc;

use arvak_hal::{
    Backend, BackendAvailability, Capabilities, Counts, ExecutionResult, HalResult, JobId,
    JobStatus, TokenProvider, ValidationResult,
};
use arvak_ir::Circuit;
use arvak_sched::{
    BatchSchedulerType, CircuitSpec, HpcScheduler, PbsConfig, Priority, ResourceRequirements,
    ScheduledJob, ScheduledJobStatus, Scheduler, SchedulerConfig, SlurmConfig,
};
use async_trait::async_trait;

/// Mock IQM backend for LUMI Helmi testing.
struct MockHelmiBackend {
    name: String,
    capabilities: Capabilities,
}

impl MockHelmiBackend {
    fn new() -> Self {
        Self {
            name: "helmi".to_string(),
            capabilities: Capabilities::iqm("helmi", 5),
        }
    }
}

#[async_trait]
impl Backend for MockHelmiBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn availability(&self) -> HalResult<BackendAvailability> {
        Ok(BackendAvailability::always_available())
    }

    async fn validate(&self, _circuit: &Circuit) -> HalResult<ValidationResult> {
        Ok(ValidationResult::Valid)
    }

    async fn submit(&self, _circuit: &Circuit, _shots: u32) -> HalResult<JobId> {
        Ok(JobId::new("mock-helmi-job-12345"))
    }

    async fn status(&self, _job_id: &JobId) -> HalResult<JobStatus> {
        Ok(JobStatus::Completed)
    }

    async fn result(&self, _job_id: &JobId) -> HalResult<ExecutionResult> {
        let counts = Counts::from_pairs([("00000", 500u64), ("11111", 500u64)]);
        Ok(ExecutionResult::new(counts, 1000))
    }

    async fn cancel(&self, _job_id: &JobId) -> HalResult<()> {
        Ok(())
    }

    async fn wait(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
        self.result(job_id).await
    }
}

/// Create LUMI-specific SLURM configuration.
fn lumi_slurm_config() -> SlurmConfig {
    SlurmConfig {
        partition: "q_fiqci".to_string(),
        account: Some("project_462000test".to_string()),
        time_limit: 30,
        memory_mb: 4096,
        cpus_per_task: 1,
        work_dir: PathBuf::from("/tmp/arvak-lumi-test"),
        arvak_binary: PathBuf::from("arvak"),
        modules: vec!["iqm-client".to_string()],
        python_venv: None,
        priority_qos_mapping: None,
    }
}

/// Create LUMI scheduler configuration.
fn lumi_scheduler_config() -> SchedulerConfig {
    SchedulerConfig {
        scheduler_type: BatchSchedulerType::Slurm,
        slurm: lumi_slurm_config(),
        pbs: PbsConfig::default(),
        poll_interval_secs: 5,
        max_wait_time_secs: 1800, // 30 minutes
        auto_match_resources: true,
        state_dir: PathBuf::from("/tmp/arvak-lumi-test/state"),
    }
}

#[tokio::test]
async fn test_lumi_scheduler_creation() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    // Test that scheduler was created successfully
    let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0];");
    let job = ScheduledJob::new("lumi_test_job", circuit);
    let job_id = job.id.clone();

    let submitted_id = scheduler.submit(job).await.unwrap();
    assert_eq!(submitted_id, job_id);

    let status = scheduler.status(&job_id).await.unwrap();
    assert!(status.is_pending());
}

#[tokio::test]
async fn test_lumi_job_requirements() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    // Create job with specific LUMI requirements
    let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0]; cx q[0], q[1];");
    let requirements = ResourceRequirements::new(5).require_real_hardware();

    let job = ScheduledJob::new("lumi_helmi_job", circuit)
        .with_requirements(requirements)
        .with_priority(Priority::high())
        .with_shots(1000);

    let job_id = scheduler.submit(job).await.unwrap();
    let status = scheduler.status(&job_id).await.unwrap();
    assert!(status.is_pending());
}

#[tokio::test]
async fn test_lumi_batch_submission() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    // Submit multiple circuits as a batch
    let circuits = vec![
        CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0];"),
        CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; x q[0];"),
        CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0]; cx q[0], q[1];"),
    ];

    let job_id = scheduler
        .submit_batch(
            "lumi_batch",
            circuits,
            1000,
            Priority::default(),
            ResourceRequirements::new(5),
        )
        .await
        .unwrap();

    let status = scheduler.status(&job_id).await.unwrap();
    assert!(status.is_pending());
}

#[tokio::test]
async fn test_lumi_slurm_config_values() {
    let config = lumi_slurm_config();

    // Verify LUMI-specific configuration
    assert_eq!(config.partition, "q_fiqci");
    assert_eq!(config.account, Some("project_462000test".to_string()));
    assert!(config.modules.contains(&"iqm-client".to_string()));
}

#[tokio::test]
async fn test_lumi_workflow_submission() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    // Create a workflow with dependent jobs
    let circuit1 = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0];");
    let circuit2 = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; cx q[0], q[1];");

    let job1 = ScheduledJob::new("calibration", circuit1);
    let job2 = ScheduledJob::new("experiment", circuit2);

    let workflow = scheduler
        .create_workflow("lumi_vqe_workflow")
        .add_job(job1)
        .then(job2)
        .unwrap()
        .build();

    let workflow_id = scheduler.submit_workflow(workflow).await.unwrap();
    let status = scheduler.workflow_status(&workflow_id).await.unwrap();

    assert!(matches!(status, arvak_sched::WorkflowStatus::Pending));
}

// ============================================================================
// OIDC Authentication Tests
// ============================================================================

#[test]
fn test_lumi_oidc_config() {
    use arvak_hal::OidcConfig;

    let config = OidcConfig::lumi("project_462000123");

    assert_eq!(config.provider, "csc");
    assert!(config.auth_endpoint.contains("auth.csc.fi"));
    assert!(config.token_endpoint.contains("auth.csc.fi"));
    assert_eq!(config.project_id, Some("project_462000123".to_string()));
    assert!(config.scopes.contains(&"openid".to_string()));
}

#[test]
fn test_lrz_oidc_config() {
    use arvak_hal::OidcConfig;

    let config = OidcConfig::lrz("project_lrz_456");

    assert_eq!(config.provider, "lrz");
    assert!(config.auth_endpoint.contains("auth.lrz.de"));
    assert_eq!(config.project_id, Some("project_lrz_456".to_string()));
}

#[test]
fn test_env_token_provider_for_lumi() {
    use arvak_hal::EnvTokenProvider;

    // IQM_TOKEN is the standard environment variable for IQM backends
    let provider = EnvTokenProvider::iqm();

    // Without the env var set, should not have a valid token
    assert!(!provider.has_valid_token());
}

// ============================================================================
// PBS Adapter Tests for LUMI-like Environments
// ============================================================================

#[tokio::test]
async fn test_pbs_scheduler_creation() {
    let config = SchedulerConfig {
        scheduler_type: BatchSchedulerType::Pbs,
        pbs: PbsConfig {
            queue: "quantum".to_string(),
            account: Some("project_test".to_string()),
            walltime: "00:30:00".to_string(),
            memory: "4gb".to_string(),
            nodes: 1,
            ppn: 1,
            work_dir: PathBuf::from("/tmp/arvak-pbs-test"),
            arvak_binary: PathBuf::from("arvak"),
            modules: vec!["quantum-toolkit".to_string()],
            python_venv: None,
            server: Some("pbs-server.local".to_string()),
            extra_directives: Vec::new(),
            priority_queue_mapping: None,
        },
        ..Default::default()
    };

    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_pbs(config, backends, store);

    let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0];");
    let job = ScheduledJob::new("pbs_test_job", circuit);
    let job_id = job.id.clone();

    let submitted_id = scheduler.submit(job).await.unwrap();
    assert_eq!(submitted_id, job_id);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[tokio::test]
async fn test_circuit_too_large_for_helmi() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    // Create a circuit that's too large for Helmi (> 5 qubits)
    let large_circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[10] q; h q[0];");
    let requirements = ResourceRequirements::new(10);

    let job = ScheduledJob::new("too_large", large_circuit).with_requirements(requirements);

    // Job should still be submitted (matching happens later)
    let result = scheduler.submit(job).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_job_cancellation() {
    let config = lumi_scheduler_config();
    let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockHelmiBackend::new())];
    let store = Arc::new(arvak_sched::SqliteStore::in_memory().unwrap());

    let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

    let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[5] q; h q[0];");
    let job = ScheduledJob::new("cancel_test", circuit);
    let job_id = job.id.clone();

    scheduler.submit(job).await.unwrap();

    // Cancel the job
    let cancel_result = scheduler.cancel(&job_id).await;
    assert!(cancel_result.is_ok());

    // Status should be cancelled
    let status = scheduler.status(&job_id).await.unwrap();
    assert!(matches!(status, ScheduledJobStatus::Cancelled));
}

// ============================================================================
// Real Integration Tests (Ignored by default - require LUMI access)
// ============================================================================

#[tokio::test]
#[ignore = "Requires actual LUMI access and IQM_TOKEN"]
async fn test_real_lumi_connection() {
    // This test requires:
    // 1. Active LUMI account with q_fiqci partition access
    // 2. Valid IQM_TOKEN environment variable
    // 3. Network access to LUMI

    use arvak_hal::{OidcAuth, OidcConfig};

    let config = OidcConfig::lumi("project_462000xxx");
    let auth = OidcAuth::new(config).expect("Failed to create OIDC auth");

    // Try to get token (will fail without proper setup)
    let token_result = auth.get_token().await;
    assert!(token_result.is_ok(), "Failed to get OIDC token");
}

#[tokio::test]
#[ignore = "Requires actual LUMI access"]
async fn test_real_slurm_submission() {
    // This test requires actual SLURM access
    // It will submit a real job to the q_fiqci partition

    let config = lumi_scheduler_config();

    // This would need real backends and real SLURM access
    // For now, this test is a placeholder for manual testing
    assert!(config.slurm.partition == "q_fiqci");
}
