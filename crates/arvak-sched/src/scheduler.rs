//! HPC Scheduler implementation.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use arvak_hal::{Backend, ExecutionResult};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::time::interval;

use crate::error::{SchedError, SchedResult};
use crate::job::{
    CircuitSpec, JobFilter, Priority, ResourceRequirements, ScheduledJob, ScheduledJobId,
    ScheduledJobStatus,
};
use crate::matcher::{Matcher, ResourceMatcher};
use crate::pbs::{PbsAdapter, PbsConfig, PbsState};
use crate::persistence::StateStore;
use crate::queue::PriorityQueue;
use crate::slurm::{SlurmAdapter, SlurmConfig, SlurmState};
use crate::workflow::{Workflow, WorkflowBuilder, WorkflowId, WorkflowStatus};

/// The type of HPC batch scheduler to use.
#[derive(Debug, Clone, Default)]
pub enum BatchSchedulerType {
    /// SLURM (Simple Linux Utility for Resource Management).
    #[default]
    Slurm,
    /// PBS (Portable Batch System) / Torque / PBS Pro.
    Pbs,
}

/// Enum to hold either SLURM or PBS adapter.
enum BatchAdapter {
    Slurm(SlurmAdapter),
    Pbs(PbsAdapter),
}

/// Configuration for the HPC scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Type of batch scheduler to use.
    pub scheduler_type: BatchSchedulerType,

    /// SLURM configuration (used when `scheduler_type` is Slurm).
    pub slurm: SlurmConfig,

    /// PBS configuration (used when `scheduler_type` is Pbs).
    pub pbs: PbsConfig,

    /// Status polling interval in seconds.
    pub poll_interval_secs: u64,

    /// Maximum time to wait for a job (seconds).
    pub max_wait_time_secs: u64,

    /// Whether to automatically match resources on submit.
    pub auto_match_resources: bool,

    /// Working directory for scheduler state.
    pub state_dir: PathBuf,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            scheduler_type: BatchSchedulerType::default(),
            slurm: SlurmConfig::default(),
            pbs: PbsConfig::default(),
            poll_interval_secs: 30,
            max_wait_time_secs: 86400, // 24 hours
            auto_match_resources: true,
            state_dir: std::env::var("ARVAK_STATE_DIR")
                .map(PathBuf::from)
                .or_else(|_| {
                    std::env::var("XDG_RUNTIME_DIR")
                        .map(|d| PathBuf::from(d).join("arvak-scheduler"))
                })
                .unwrap_or_else(|_| std::env::temp_dir().join("arvak-scheduler")),
        }
    }
}

impl SchedulerConfig {
    /// Create a configuration for SLURM.
    pub fn with_slurm(slurm: SlurmConfig) -> Self {
        Self {
            scheduler_type: BatchSchedulerType::Slurm,
            slurm,
            ..Default::default()
        }
    }

    /// Create a configuration for PBS.
    pub fn with_pbs(pbs: PbsConfig) -> Self {
        Self {
            scheduler_type: BatchSchedulerType::Pbs,
            pbs,
            ..Default::default()
        }
    }
}

/// Trait for scheduler implementations.
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// Submit a job to the scheduler.
    async fn submit(&self, job: ScheduledJob) -> SchedResult<ScheduledJobId>;

    /// Submit a batch of circuits as a single job.
    async fn submit_batch(
        &self,
        name: &str,
        circuits: Vec<CircuitSpec>,
        shots: u32,
        priority: Priority,
        requirements: ResourceRequirements,
    ) -> SchedResult<ScheduledJobId>;

    /// Get the status of a job.
    async fn status(&self, job_id: &ScheduledJobId) -> SchedResult<ScheduledJobStatus>;

    /// Cancel a job.
    async fn cancel(&self, job_id: &ScheduledJobId) -> SchedResult<()>;

    /// Wait for a job to complete and return the result.
    async fn wait(&self, job_id: &ScheduledJobId) -> SchedResult<ExecutionResult>;

    /// Get the result of a completed job.
    async fn result(&self, job_id: &ScheduledJobId) -> SchedResult<ExecutionResult>;

    /// List jobs matching the filter.
    async fn list_jobs(&self, filter: JobFilter) -> SchedResult<Vec<ScheduledJob>>;

    /// Create a workflow builder.
    fn create_workflow(&self, name: &str) -> WorkflowBuilder;

    /// Submit a workflow for execution.
    async fn submit_workflow(&self, workflow: Workflow) -> SchedResult<WorkflowId>;

    /// Get the status of a workflow.
    async fn workflow_status(&self, workflow_id: &WorkflowId) -> SchedResult<WorkflowStatus>;

    /// Wait for a workflow to complete.
    async fn wait_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<()>;
}

/// Maximum number of completed job IDs to retain in memory.
const MAX_COMPLETED_JOBS: usize = 10_000;

/// Maximum number of completed workflows to retain in memory.
const MAX_COMPLETED_WORKFLOWS: usize = 1_000;

/// HPC Scheduler with SLURM and PBS integration.
pub struct HpcScheduler {
    config: SchedulerConfig,
    adapter: BatchAdapter,
    matcher: ResourceMatcher,
    store: Arc<dyn StateStore>,
    queue: RwLock<PriorityQueue>,
    workflows: RwLock<rustc_hash::FxHashMap<WorkflowId, Workflow>>,
    completed_jobs: RwLock<rustc_hash::FxHashSet<ScheduledJobId>>,
}

impl HpcScheduler {
    /// Create a new HPC scheduler.
    pub async fn new(
        config: SchedulerConfig,
        backends: Vec<Arc<dyn Backend>>,
        store: Arc<dyn StateStore>,
    ) -> SchedResult<Self> {
        let adapter = match config.scheduler_type {
            BatchSchedulerType::Slurm => {
                BatchAdapter::Slurm(SlurmAdapter::new(config.slurm.clone()).await?)
            }
            BatchSchedulerType::Pbs => {
                BatchAdapter::Pbs(PbsAdapter::new(config.pbs.clone()).await?)
            }
        };
        let matcher = ResourceMatcher::new(backends);

        Ok(Self {
            config,
            adapter,
            matcher,
            store,
            queue: RwLock::new(PriorityQueue::new()),
            workflows: RwLock::new(rustc_hash::FxHashMap::default()),
            completed_jobs: RwLock::new(rustc_hash::FxHashSet::default()),
        })
    }

    /// Create a scheduler with a mock SLURM adapter (for testing).
    pub fn with_mock_slurm(
        config: SchedulerConfig,
        backends: Vec<Arc<dyn Backend>>,
        store: Arc<dyn StateStore>,
    ) -> Self {
        let adapter = BatchAdapter::Slurm(SlurmAdapter::mock(config.slurm.clone()));
        let matcher = ResourceMatcher::new(backends);

        Self {
            config,
            adapter,
            matcher,
            store,
            queue: RwLock::new(PriorityQueue::new()),
            workflows: RwLock::new(rustc_hash::FxHashMap::default()),
            completed_jobs: RwLock::new(rustc_hash::FxHashSet::default()),
        }
    }

    /// Create a scheduler with a mock PBS adapter (for testing).
    pub fn with_mock_pbs(
        config: SchedulerConfig,
        backends: Vec<Arc<dyn Backend>>,
        store: Arc<dyn StateStore>,
    ) -> Self {
        let adapter = BatchAdapter::Pbs(PbsAdapter::mock(config.pbs.clone()));
        let matcher = ResourceMatcher::new(backends);

        Self {
            config,
            adapter,
            matcher,
            store,
            queue: RwLock::new(PriorityQueue::new()),
            workflows: RwLock::new(rustc_hash::FxHashMap::default()),
            completed_jobs: RwLock::new(rustc_hash::FxHashSet::default()),
        }
    }

    /// Start the background job processing loop.
    // TODO: Accept a CancellationToken for graceful shutdown
    pub fn start_background_processor(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let scheduler = self.clone();
        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);

        tokio::spawn(async move {
            let mut ticker = interval(poll_interval);
            loop {
                ticker.tick().await;
                if let Err(e) = scheduler.process_pending_jobs().await {
                    tracing::error!("Error processing jobs: {}", e);
                }
                if let Err(e) = scheduler.update_job_statuses().await {
                    tracing::error!("Error updating job statuses: {}", e);
                }
            }
        })
    }

    /// Process pending jobs from the queue.
    async fn process_pending_jobs(&self) -> SchedResult<()> {
        let completed = self.completed_jobs.read().await;
        let ready_jobs = {
            let mut queue = self.queue.write().await;
            queue.drain_ready(&completed)
        };

        for mut job in ready_jobs {
            // Match resources if enabled
            if self.config.auto_match_resources && job.matched_backend.is_none() {
                match self.matcher.find_match(&job.requirements).await {
                    Ok(match_result) => {
                        job.matched_backend = Some(match_result.backend_name);
                    }
                    Err(e) => {
                        tracing::warn!("Resource matching failed for job {}: {}", job.id, e);
                        job.status = ScheduledJobStatus::Failed {
                            reason: e.to_string(),
                            slurm_job_id: None,
                            quantum_job_id: None,
                        };
                        self.store.save_job(&job).await?;
                        continue;
                    }
                }
            }

            // Submit to batch scheduler (SLURM or PBS)
            let submit_result = match &self.adapter {
                BatchAdapter::Slurm(slurm) => slurm.submit(&job).await,
                BatchAdapter::Pbs(pbs) => pbs.submit(&job).await,
            };

            match submit_result {
                Ok(batch_job_id) => {
                    job.status = ScheduledJobStatus::SlurmQueued {
                        slurm_job_id: batch_job_id,
                    };
                    job.submitted_at = Some(chrono::Utc::now());
                    self.store.save_job(&job).await?;
                    tracing::info!("Submitted job {} to batch scheduler", job.id);
                }
                Err(e) => {
                    tracing::error!("Batch submission failed for job {}: {}", job.id, e);
                    job.status = ScheduledJobStatus::Failed {
                        reason: e.to_string(),
                        slurm_job_id: None,
                        quantum_job_id: None,
                    };
                    self.store.save_job(&job).await?;
                }
            }
        }

        Ok(())
    }

    /// Update statuses of running jobs.
    async fn update_job_statuses(&self) -> SchedResult<()> {
        let jobs = self.store.list_jobs(&JobFilter::running()).await?;

        for job in jobs {
            if let Some(batch_job_id) = job.status.slurm_job_id() {
                let new_status = match &self.adapter {
                    BatchAdapter::Slurm(slurm) => match slurm.status(batch_job_id).await {
                        Ok(info) => Some(self.map_slurm_status(&job, &info)),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to get status for SLURM job {}: {}",
                                batch_job_id,
                                e
                            );
                            None
                        }
                    },
                    BatchAdapter::Pbs(pbs) => match pbs.status(batch_job_id).await {
                        Ok(info) => Some(self.map_pbs_status(&job, &info)),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to get status for PBS job {}: {}",
                                batch_job_id,
                                e
                            );
                            None
                        }
                    },
                };

                if let Some(new_status) = new_status {
                    if new_status != job.status {
                        self.store
                            .update_status(&job.id, new_status.clone())
                            .await?;

                        if new_status.is_terminal() {
                            let mut completed = self.completed_jobs.write().await;
                            // Evict oldest entries when cache exceeds limit
                            if completed.len() >= MAX_COMPLETED_JOBS {
                                let to_remove: Vec<_> = completed
                                    .iter()
                                    .take(MAX_COMPLETED_JOBS / 4)
                                    .cloned()
                                    .collect();
                                for id in to_remove {
                                    completed.remove(&id);
                                }
                            }
                            completed.insert(job.id.clone());
                        }
                    }
                }
            }
        }

        // Update workflow statuses
        let mut workflows = self.workflows.write().await;
        for workflow in workflows.values_mut() {
            if !workflow.status.is_terminal() {
                workflow.update_status();
                self.store.save_workflow(workflow).await?;
            }
        }

        // Evict completed workflows when cache exceeds limit
        if workflows.len() > MAX_COMPLETED_WORKFLOWS {
            let completed_ids: Vec<_> = workflows
                .iter()
                .filter(|(_, w)| w.status.is_terminal())
                .map(|(id, _)| id.clone())
                .collect();
            let to_evict = completed_ids
                .len()
                .saturating_sub(MAX_COMPLETED_WORKFLOWS / 2);
            for id in completed_ids.into_iter().take(to_evict) {
                workflows.remove(&id);
            }
        }

        Ok(())
    }

    /// Map SLURM job state to scheduler job status.
    fn map_slurm_status(
        &self,
        job: &ScheduledJob,
        info: &crate::slurm::SlurmJobInfo,
    ) -> ScheduledJobStatus {
        let slurm_job_id = info.job_id.clone();

        match &info.state {
            SlurmState::Pending => ScheduledJobStatus::SlurmQueued { slurm_job_id },
            SlurmState::Running | SlurmState::Completing => {
                ScheduledJobStatus::SlurmRunning { slurm_job_id }
            }
            SlurmState::Completed => {
                // Job completed - in a real scenario, we'd read the result file
                // and get the quantum job ID from it
                ScheduledJobStatus::Completed {
                    slurm_job_id,
                    quantum_job_id: arvak_hal::JobId("completed".to_string()),
                }
            }
            SlurmState::Failed | SlurmState::NodeFail | SlurmState::OutOfMemory => {
                ScheduledJobStatus::Failed {
                    reason: format!("SLURM job failed: {:?}", info.state),
                    slurm_job_id: Some(slurm_job_id),
                    quantum_job_id: job.status.quantum_job_id().cloned(),
                }
            }
            SlurmState::Timeout => ScheduledJobStatus::Failed {
                reason: "SLURM job timed out".to_string(),
                slurm_job_id: Some(slurm_job_id),
                quantum_job_id: job.status.quantum_job_id().cloned(),
            },
            SlurmState::Cancelled | SlurmState::Preempted => ScheduledJobStatus::Cancelled,
            SlurmState::Unknown(state) => {
                tracing::warn!("Unknown SLURM state: {}", state);
                job.status.clone()
            }
        }
    }

    /// Map PBS job state to scheduler job status.
    fn map_pbs_status(
        &self,
        job: &ScheduledJob,
        info: &crate::pbs::PbsJobInfo,
    ) -> ScheduledJobStatus {
        let pbs_job_id = info.job_id.clone();

        // Note: SlurmQueued/SlurmRunning variant names are a naming inconsistency --
        // the enum variants are shared between SLURM and PBS but named after SLURM.
        // The `slurm_job_id` field holds the PBS job ID in this context.
        match &info.state {
            PbsState::Queued | PbsState::Waiting | PbsState::Held => {
                ScheduledJobStatus::SlurmQueued {
                    slurm_job_id: pbs_job_id,
                }
            }
            PbsState::Running | PbsState::Exiting | PbsState::ArrayRunning => {
                ScheduledJobStatus::SlurmRunning {
                    slurm_job_id: pbs_job_id,
                }
            }
            PbsState::Completed => {
                // Check exit status to determine if it was a success
                if info.exit_status == Some(0) || info.exit_status.is_none() {
                    ScheduledJobStatus::Completed {
                        slurm_job_id: pbs_job_id,
                        quantum_job_id: arvak_hal::JobId("completed".to_string()),
                    }
                } else {
                    ScheduledJobStatus::Failed {
                        reason: format!("PBS job failed with exit status {:?}", info.exit_status),
                        slurm_job_id: Some(pbs_job_id),
                        quantum_job_id: job.status.quantum_job_id().cloned(),
                    }
                }
            }
            PbsState::Failed => ScheduledJobStatus::Failed {
                reason: "PBS job failed".to_string(),
                slurm_job_id: Some(pbs_job_id),
                quantum_job_id: job.status.quantum_job_id().cloned(),
            },
            PbsState::Suspended | PbsState::Transit => {
                // Keep current status for suspended/transit jobs
                job.status.clone()
            }
            PbsState::Unknown(state) => {
                tracing::warn!("Unknown PBS state: {}", state);
                job.status.clone()
            }
        }
    }
}

#[async_trait]
impl Scheduler for HpcScheduler {
    async fn submit(&self, mut job: ScheduledJob) -> SchedResult<ScheduledJobId> {
        let job_id = job.id.clone();

        // Check if job has unsatisfied dependencies
        if !job.dependencies.is_empty() {
            let completed = self.completed_jobs.read().await;
            if !job.dependencies_satisfied(&completed) {
                job.status = ScheduledJobStatus::WaitingOnDependencies;
            }
        }

        // Save to store
        self.store.save_job(&job).await?;

        // Add to queue
        let mut queue = self.queue.write().await;
        queue.push(job);

        tracing::info!("Job {} submitted to scheduler", job_id);
        Ok(job_id)
    }

    async fn submit_batch(
        &self,
        name: &str,
        circuits: Vec<CircuitSpec>,
        shots: u32,
        priority: Priority,
        requirements: ResourceRequirements,
    ) -> SchedResult<ScheduledJobId> {
        let job = ScheduledJob::batch(name, circuits)
            .with_shots(shots)
            .with_priority(priority)
            .with_requirements(requirements);

        self.submit(job).await
    }

    async fn status(&self, job_id: &ScheduledJobId) -> SchedResult<ScheduledJobStatus> {
        // Check queue first
        {
            let queue = self.queue.read().await;
            if let Some(job) = queue.get(job_id) {
                return Ok(job.status.clone());
            }
        }

        // Check store
        let job = self
            .store
            .load_job(job_id)
            .await?
            .ok_or_else(|| SchedError::JobNotFound(job_id.to_string()))?;

        Ok(job.status)
    }

    async fn cancel(&self, job_id: &ScheduledJobId) -> SchedResult<()> {
        // Remove from queue if present
        {
            let mut queue = self.queue.write().await;
            if queue.remove(job_id).is_some() {
                self.store
                    .update_status(job_id, ScheduledJobStatus::Cancelled)
                    .await?;
                return Ok(());
            }
        }

        // Check if job is running on batch scheduler
        let job = self
            .store
            .load_job(job_id)
            .await?
            .ok_or_else(|| SchedError::JobNotFound(job_id.to_string()))?;

        if let Some(batch_job_id) = job.status.slurm_job_id() {
            match &self.adapter {
                BatchAdapter::Slurm(slurm) => slurm.cancel(batch_job_id).await?,
                BatchAdapter::Pbs(pbs) => pbs.cancel(batch_job_id).await?,
            }
        }

        self.store
            .update_status(job_id, ScheduledJobStatus::Cancelled)
            .await?;

        Ok(())
    }

    async fn wait(&self, job_id: &ScheduledJobId) -> SchedResult<ExecutionResult> {
        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);
        let max_wait = Duration::from_secs(self.config.max_wait_time_secs);
        let start = std::time::Instant::now();

        loop {
            let status = self.status(job_id).await?;

            if status.is_terminal() {
                if status.is_success() {
                    return self.result(job_id).await;
                }
                return Err(SchedError::JobNotFound(format!(
                    "Job {job_id} failed or was cancelled: {status:?}"
                )));
            }

            if start.elapsed() > max_wait {
                return Err(SchedError::Timeout(format!(
                    "Timeout waiting for job {job_id}"
                )));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn result(&self, job_id: &ScheduledJobId) -> SchedResult<ExecutionResult> {
        self.store
            .load_result(job_id)
            .await?
            .ok_or_else(|| SchedError::JobNotFound(format!("No result for job {job_id}")))
    }

    async fn list_jobs(&self, filter: JobFilter) -> SchedResult<Vec<ScheduledJob>> {
        self.store.list_jobs(&filter).await
    }

    fn create_workflow(&self, name: &str) -> WorkflowBuilder {
        WorkflowBuilder::new(name)
    }

    async fn submit_workflow(&self, workflow: Workflow) -> SchedResult<WorkflowId> {
        let workflow_id = workflow.id.clone();

        // Save workflow
        self.store.save_workflow(&workflow).await?;

        // Submit all jobs
        for job in workflow.all_jobs() {
            self.store.save_job(job).await?;
            let mut queue = self.queue.write().await;
            queue.push(job.clone());
        }

        // Store workflow for tracking
        {
            let mut workflows = self.workflows.write().await;
            workflows.insert(workflow_id.clone(), workflow);
        }

        tracing::info!("Workflow {} submitted", workflow_id);
        Ok(workflow_id)
    }

    async fn workflow_status(&self, workflow_id: &WorkflowId) -> SchedResult<WorkflowStatus> {
        let workflows = self.workflows.read().await;
        workflows
            .get(workflow_id)
            .map(|w| w.status.clone())
            .ok_or_else(|| SchedError::WorkflowNotFound(workflow_id.to_string()))
    }

    async fn wait_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<()> {
        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);
        let max_wait = Duration::from_secs(self.config.max_wait_time_secs);
        let start = std::time::Instant::now();

        loop {
            let status = self.workflow_status(workflow_id).await?;

            if status.is_terminal() {
                return match status {
                    WorkflowStatus::Completed => Ok(()),
                    WorkflowStatus::Failed { reason } => {
                        Err(SchedError::Internal(format!("Workflow failed: {reason}")))
                    }
                    WorkflowStatus::Cancelled => {
                        Err(SchedError::Cancelled(workflow_id.to_string()))
                    }
                    _ => unreachable!(),
                };
            }

            if start.elapsed() > max_wait {
                return Err(SchedError::Timeout(format!(
                    "Timeout waiting for workflow {workflow_id}"
                )));
            }

            tokio::time::sleep(poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::SqliteStore;
    use arvak_hal::{Capabilities, Counts};

    /// Mock backend for testing.
    struct MockBackend {
        name: String,
        capabilities: Capabilities,
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str {
            &self.name
        }

        fn capabilities(&self) -> &Capabilities {
            &self.capabilities
        }

        async fn availability(&self) -> arvak_hal::HalResult<arvak_hal::BackendAvailability> {
            Ok(arvak_hal::BackendAvailability::always_available())
        }

        async fn validate(
            &self,
            _circuit: &arvak_ir::Circuit,
        ) -> arvak_hal::HalResult<arvak_hal::ValidationResult> {
            Ok(arvak_hal::ValidationResult::Valid)
        }

        async fn submit(
            &self,
            _circuit: &arvak_ir::Circuit,
            _shots: u32,
        ) -> arvak_hal::HalResult<arvak_hal::JobId> {
            Ok(arvak_hal::JobId("mock".to_string()))
        }

        async fn status(
            &self,
            _job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::JobStatus> {
            Ok(arvak_hal::JobStatus::Completed)
        }

        async fn result(
            &self,
            _job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::ExecutionResult> {
            let counts = Counts::from_pairs([("00", 500u64), ("11", 500u64)]);
            Ok(arvak_hal::ExecutionResult::new(counts, 1000))
        }

        async fn cancel(&self, _job_id: &arvak_hal::JobId) -> arvak_hal::HalResult<()> {
            Ok(())
        }

        async fn wait(
            &self,
            job_id: &arvak_hal::JobId,
        ) -> arvak_hal::HalResult<arvak_hal::ExecutionResult> {
            self.result(job_id).await
        }
    }

    #[tokio::test]
    async fn test_scheduler_submit() {
        let config = SchedulerConfig::default();
        let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockBackend {
            name: "test_backend".to_string(),
            capabilities: Capabilities::simulator(10),
        })];
        let store = Arc::new(SqliteStore::in_memory().unwrap());

        let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];");
        let job = ScheduledJob::new("test_job", circuit);
        let job_id = job.id.clone();

        let submitted_id = scheduler.submit(job).await.unwrap();
        assert_eq!(submitted_id, job_id);

        let status = scheduler.status(&job_id).await.unwrap();
        assert!(status.is_pending());
    }

    #[tokio::test]
    async fn test_scheduler_submit_batch() {
        let config = SchedulerConfig::default();
        let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockBackend {
            name: "test_backend".to_string(),
            capabilities: Capabilities::simulator(10),
        })];
        let store = Arc::new(SqliteStore::in_memory().unwrap());

        let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

        let circuits = vec![
            CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0];"),
            CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; x q[0];"),
        ];

        let job_id = scheduler
            .submit_batch(
                "batch_test",
                circuits,
                1000,
                Priority::default(),
                ResourceRequirements::default(),
            )
            .await
            .unwrap();

        let status = scheduler.status(&job_id).await.unwrap();
        assert!(status.is_pending());
    }

    #[tokio::test]
    async fn test_scheduler_workflow() {
        let config = SchedulerConfig::default();
        let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockBackend {
            name: "test_backend".to_string(),
            capabilities: Capabilities::simulator(10),
        })];
        let store = Arc::new(SqliteStore::in_memory().unwrap());

        let scheduler = HpcScheduler::with_mock_slurm(config, backends, store);

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job1 = ScheduledJob::new("job1", circuit.clone());
        let job2 = ScheduledJob::new("job2", circuit);

        let workflow = scheduler
            .create_workflow("test_workflow")
            .add_job(job1)
            .then(job2)
            .unwrap()
            .build();

        let workflow_id = scheduler.submit_workflow(workflow).await.unwrap();

        let status = scheduler.workflow_status(&workflow_id).await.unwrap();
        assert!(matches!(status, WorkflowStatus::Pending));
    }

    #[tokio::test]
    async fn test_scheduler_submit_with_pbs() {
        let config = SchedulerConfig::with_pbs(PbsConfig::default());
        let backends: Vec<Arc<dyn Backend>> = vec![Arc::new(MockBackend {
            name: "test_backend".to_string(),
            capabilities: Capabilities::simulator(10),
        })];
        let store = Arc::new(SqliteStore::in_memory().unwrap());

        let scheduler = HpcScheduler::with_mock_pbs(config, backends, store);

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];");
        let job = ScheduledJob::new("pbs_test_job", circuit);
        let job_id = job.id.clone();

        let submitted_id = scheduler.submit(job).await.unwrap();
        assert_eq!(submitted_id, job_id);

        let status = scheduler.status(&job_id).await.unwrap();
        assert!(status.is_pending());
    }

    #[tokio::test]
    async fn test_scheduler_config_builders() {
        let slurm_config = SchedulerConfig::with_slurm(SlurmConfig {
            partition: "quantum".to_string(),
            ..Default::default()
        });
        assert!(matches!(
            slurm_config.scheduler_type,
            BatchSchedulerType::Slurm
        ));

        let pbs_config = SchedulerConfig::with_pbs(PbsConfig {
            queue: "quantum".to_string(),
            ..Default::default()
        });
        assert!(matches!(pbs_config.scheduler_type, BatchSchedulerType::Pbs));
    }
}
