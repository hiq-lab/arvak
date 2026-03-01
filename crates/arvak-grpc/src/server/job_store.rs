//! Job storage and management.
//!
//! This module provides a wrapper around pluggable storage backends.
//! The actual storage implementation can be in-memory, `SQLite`, `PostgreSQL`, etc.

use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::ExecutionResult;
use arvak_ir::circuit::Circuit;
use chrono::Utc;
use std::sync::Arc;

use crate::error::Result;
use crate::storage::{JobStorage, MemoryStorage, StoredJob};

/// Thread-safe job store using pluggable storage backend.
#[derive(Clone)]
pub struct JobStore {
    storage: Arc<dyn JobStorage>,
}

impl JobStore {
    /// Create a new job store with in-memory storage.
    pub fn new() -> Self {
        Self {
            storage: Arc::new(MemoryStorage::new()),
        }
    }

    /// Create a job store with a custom storage backend.
    pub fn with_storage(storage: Arc<dyn JobStorage>) -> Self {
        Self { storage }
    }

    /// Create a new job and return its ID.
    pub async fn create_job(
        &self,
        circuit: Circuit,
        backend_id: String,
        shots: u32,
        parameters: Option<std::collections::HashMap<String, f64>>,
    ) -> Result<JobId> {
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
            parameters,
        };

        self.storage.store_job(&job).await?;

        Ok(job_id)
    }

    /// Update job status.
    pub async fn update_status(&self, job_id: &JobId, status: JobStatus) -> Result<()> {
        self.storage.update_status(job_id, status).await
    }

    /// Store job result.
    pub async fn store_result(&self, job_id: &JobId, result: ExecutionResult) -> Result<()> {
        self.storage.store_result(job_id, result).await
    }

    /// Get job by ID.
    pub async fn get_job(&self, job_id: &JobId) -> Result<StoredJob> {
        self.storage
            .get_job(job_id)
            .await?
            .ok_or_else(|| crate::error::Error::JobNotFound(job_id.0.clone()))
    }

    /// Get job result by ID.
    pub async fn get_result(&self, job_id: &JobId) -> Result<ExecutionResult> {
        self.storage.get_result(job_id).await
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

        let job_id = store
            .create_job(circuit, "simulator".to_string(), 1000, None)
            .await
            .unwrap();

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
        let job_id = store
            .create_job(circuit, "simulator".to_string(), 1000, None)
            .await
            .unwrap();

        store
            .update_status(&job_id, JobStatus::Running)
            .await
            .unwrap();
        let job = store.get_job(&job_id).await.unwrap();
        assert_eq!(job.status, JobStatus::Running);
        assert!(job.started_at.is_some());

        store
            .update_status(&job_id, JobStatus::Completed)
            .await
            .unwrap();
        let job = store.get_job(&job_id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_job_not_found() {
        let store = JobStore::new();
        let job_id = JobId::new("nonexistent".to_string());

        let result = store.get_job(&job_id).await;
        assert!(matches!(result, Err(crate::error::Error::JobNotFound(_))));
    }
}
