//! Pluggable storage backends for job persistence.
//!
//! This module defines the `JobStorage` trait which allows different storage
//! backends to be used interchangeably:
//!
//! - `MemoryStorage`: In-memory storage (no persistence)
//! - `SqliteStorage`: `SQLite` database for single-node deployments
//! - `PostgresStorage`: `PostgreSQL` for production clusters

use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::ExecutionResult;
use arvak_ir::circuit::Circuit;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::Result;

pub mod memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "postgres")]
pub mod postgres;

// Re-exports
pub use memory::MemoryStorage;

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStorage;

#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;

/// Stored job with metadata and state.
#[derive(Clone)]
pub struct StoredJob {
    pub id: JobId,
    pub circuit: Circuit,
    pub backend_id: String,
    pub shots: u32,
    pub status: JobStatus,
    pub submitted_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<ExecutionResult>,
    /// Optional parameter bindings for parametric circuits (DEBT-25).
    /// Keys are OpenQASM 3.0 `input float[64]` parameter names.
    pub parameters: Option<std::collections::HashMap<String, f64>>,
}

/// Filter for querying jobs.
#[derive(Clone, Debug, Default)]
pub struct JobFilter {
    /// Filter by job state
    pub state: Option<JobStatus>,
    /// Filter by backend ID
    pub backend_id: Option<String>,
    /// Only jobs submitted after this time
    pub after: Option<DateTime<Utc>>,
    /// Only jobs submitted before this time
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of results
    pub limit: usize,
}

impl JobFilter {
    pub fn new() -> Self {
        Self {
            limit: 100, // Default limit
            ..Default::default()
        }
    }

    pub fn with_state(mut self, state: JobStatus) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_backend(mut self, backend_id: String) -> Self {
        self.backend_id = Some(backend_id);
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

/// Convert a `JobStatus` to its canonical storage string.
///
/// Shared by all storage backends to guarantee a consistent on-disk format.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) fn job_status_to_string(status: &JobStatus) -> String {
    match status {
        JobStatus::Queued => "queued".to_string(),
        JobStatus::Running => "running".to_string(),
        JobStatus::Completed => "completed".to_string(),
        JobStatus::Failed(msg) => format!("failed:{}", msg),
        JobStatus::Cancelled => "cancelled".to_string(),
    }
}

/// Parse a storage string back to `JobStatus`.
///
/// Returns an error for unrecognised strings so callers can surface storage
/// corruption early rather than silently mis-classify jobs.
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub(crate) fn job_status_from_string(s: &str) -> Result<JobStatus> {
    match s {
        "queued" => Ok(JobStatus::Queued),
        "running" => Ok(JobStatus::Running),
        "completed" => Ok(JobStatus::Completed),
        "cancelled" => Ok(JobStatus::Cancelled),
        _ => {
            if let Some(msg) = s.strip_prefix("failed:") {
                Ok(JobStatus::Failed(msg.to_string()))
            } else {
                Err(crate::error::Error::StorageError(format!(
                    "Invalid job status string: {:?}",
                    s
                )))
            }
        }
    }
}

/// Trait for job storage backends.
///
/// Implementations must be thread-safe (Send + Sync) and support async operations.
#[async_trait]
pub trait JobStorage: Send + Sync {
    /// Store a new job in the storage backend.
    async fn store_job(&self, job: &StoredJob) -> Result<()>;

    /// Get a job by ID.
    ///
    /// Returns `None` if the job does not exist.
    async fn get_job(&self, job_id: &JobId) -> Result<Option<StoredJob>>;

    /// Update the status of a job.
    ///
    /// This method should also update timestamps (`started_at`, `completed_at`)
    /// based on the new status.
    async fn update_status(&self, job_id: &JobId, status: JobStatus) -> Result<()>;

    /// Store the result of a completed job.
    ///
    /// This method should also update the job status to Completed and set
    /// the `completed_at` timestamp.
    async fn store_result(&self, job_id: &JobId, result: ExecutionResult) -> Result<()>;

    /// List jobs matching the filter criteria.
    ///
    /// Results are ordered by `submitted_at` descending (most recent first).
    async fn list_jobs(&self, filter: JobFilter) -> Result<Vec<StoredJob>>;

    /// Delete a job from storage.
    ///
    /// Returns `Ok(())` even if the job doesn't exist (idempotent).
    async fn delete_job(&self, job_id: &JobId) -> Result<()>;

    /// Get a job result by ID.
    ///
    /// This is a convenience method that combines `get_job` and extracting
    /// the result. Returns an error if the job is not completed.
    async fn get_result(&self, job_id: &JobId) -> Result<ExecutionResult> {
        let job = self
            .get_job(job_id)
            .await?
            .ok_or_else(|| crate::error::Error::JobNotFound(job_id.0.clone()))?;

        match &job.status {
            JobStatus::Completed => job.result.ok_or_else(|| {
                crate::error::Error::Internal("Completed job has no result".to_string())
            }),
            JobStatus::Failed(msg) => Err(crate::error::Error::JobFailed(msg.clone())),
            _ => Err(crate::error::Error::JobNotCompleted(job_id.0.clone())),
        }
    }
}
