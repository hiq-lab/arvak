//! Persistence layer for job state.

mod json_store;
mod sqlite_store;

pub use json_store::JsonStore;
pub use sqlite_store::SqliteStore;

use async_trait::async_trait;
use hiq_hal::ExecutionResult;

use crate::error::SchedResult;
use crate::job::{JobFilter, ScheduledJob, ScheduledJobId, ScheduledJobStatus};
use crate::workflow::{Workflow, WorkflowId};

/// Trait for persistent state storage.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Save a job to the store.
    async fn save_job(&self, job: &ScheduledJob) -> SchedResult<()>;

    /// Load a job from the store.
    async fn load_job(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ScheduledJob>>;

    /// Update a job's status.
    async fn update_status(
        &self,
        job_id: &ScheduledJobId,
        status: ScheduledJobStatus,
    ) -> SchedResult<()>;

    /// Delete a job from the store.
    async fn delete_job(&self, job_id: &ScheduledJobId) -> SchedResult<bool>;

    /// List jobs matching a filter.
    async fn list_jobs(&self, filter: &JobFilter) -> SchedResult<Vec<ScheduledJob>>;

    /// Save execution result for a job.
    async fn save_result(
        &self,
        job_id: &ScheduledJobId,
        result: &ExecutionResult,
    ) -> SchedResult<()>;

    /// Load execution result for a job.
    async fn load_result(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ExecutionResult>>;

    /// Save a workflow to the store.
    async fn save_workflow(&self, workflow: &Workflow) -> SchedResult<()>;

    /// Load a workflow from the store.
    async fn load_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<Option<Workflow>>;

    /// Delete a workflow from the store.
    async fn delete_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<bool>;

    /// List all workflow IDs.
    async fn list_workflows(&self) -> SchedResult<Vec<WorkflowId>>;

    /// Clean up old completed/failed jobs.
    async fn cleanup_old_jobs(&self, max_age_seconds: u64) -> SchedResult<usize>;
}
