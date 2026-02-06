//! Job storage and management.
//!
//! This module provides thread-safe in-memory storage for jobs using
//! `Arc<RwLock<FxHashMap>>`. In Phase 1, jobs are not persisted to disk.

use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::ExecutionResult;
use arvak_ir::circuit::Circuit;
use chrono::{DateTime, Utc};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::{Error, Result};

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
}

/// Thread-safe job store.
#[derive(Clone)]
pub struct JobStore {
    jobs: Arc<RwLock<FxHashMap<String, StoredJob>>>,
}

impl JobStore {
    /// Create a new job store.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }

    /// Create a new job and return its ID.
    pub async fn create_job(
        &self,
        circuit: Circuit,
        backend_id: String,
        shots: u32,
    ) -> JobId {
        let job_id = JobId::new(uuid::Uuid::new_v4().to_string());

        let job = StoredJob {
            id: job_id.clone(),
            circuit,
            backend_id,
            shots,
            status: JobStatus::Queued,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        let mut jobs = self.jobs.write().await;
        jobs.insert(job_id.0.clone(), job);

        job_id
    }

    /// Update job status.
    pub async fn update_status(&self, job_id: &JobId, status: JobStatus) -> Result<()> {
        let mut jobs = self.jobs.write().await;

        let job = jobs
            .get_mut(&job_id.0)
            .ok_or_else(|| Error::JobNotFound(job_id.0.clone()))?;

        job.status = status.clone();

        // Update timestamps based on status
        match status {
            JobStatus::Running if job.started_at.is_none() => {
                job.started_at = Some(Utc::now());
            }
            JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled
                if job.completed_at.is_none() =>
            {
                job.completed_at = Some(Utc::now());
            }
            _ => {}
        }

        Ok(())
    }

    /// Store job result.
    pub async fn store_result(&self, job_id: &JobId, result: ExecutionResult) -> Result<()> {
        let mut jobs = self.jobs.write().await;

        let job = jobs
            .get_mut(&job_id.0)
            .ok_or_else(|| Error::JobNotFound(job_id.0.clone()))?;

        job.result = Some(result);
        job.status = JobStatus::Completed;

        if job.completed_at.is_none() {
            job.completed_at = Some(Utc::now());
        }

        Ok(())
    }

    /// Get job by ID.
    pub async fn get_job(&self, job_id: &JobId) -> Result<StoredJob> {
        let jobs = self.jobs.read().await;
        jobs.get(&job_id.0)
            .cloned()
            .ok_or_else(|| Error::JobNotFound(job_id.0.clone()))
    }

    /// Get job result by ID.
    pub async fn get_result(&self, job_id: &JobId) -> Result<ExecutionResult> {
        let job = self.get_job(job_id).await?;

        match &job.status {
            JobStatus::Completed => job
                .result
                .ok_or_else(|| Error::Internal("Completed job has no result".to_string())),
            JobStatus::Failed(msg) => Err(Error::JobFailed(msg.clone())),
            _ => Err(Error::JobNotCompleted(job_id.0.clone())),
        }
    }
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_job_lifecycle() {
        let store = JobStore::new();
        let circuit = Circuit::with_size("test", 2, 0);

        let job_id = store.create_job(circuit, "simulator".to_string(), 1000).await;

        let job = store.get_job(&job_id).await.unwrap();
        assert_eq!(job.status, JobStatus::Queued);
        assert!(job.submitted_at <= Utc::now());
        assert!(job.started_at.is_none());
        assert!(job.completed_at.is_none());
    }

    #[tokio::test]
    async fn test_status_update() {
        let store = JobStore::new();
        let circuit = Circuit::with_size("test", 2, 0);
        let job_id = store.create_job(circuit, "simulator".to_string(), 1000).await;

        store.update_status(&job_id, JobStatus::Running).await.unwrap();
        let job = store.get_job(&job_id).await.unwrap();
        assert_eq!(job.status, JobStatus::Running);
        assert!(job.started_at.is_some());

        store.update_status(&job_id, JobStatus::Completed).await.unwrap();
        let job = store.get_job(&job_id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_job_not_found() {
        let store = JobStore::new();
        let job_id = JobId::new("nonexistent");

        let result = store.get_job(&job_id).await;
        assert!(matches!(result, Err(Error::JobNotFound(_))));
    }
}
