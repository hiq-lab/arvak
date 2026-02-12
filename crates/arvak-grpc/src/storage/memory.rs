//! In-memory job storage (no persistence).
//!
//! This implementation uses `Arc<RwLock<FxHashMap>>` for thread-safe in-memory
//! storage. Jobs are lost when the server restarts.

use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::ExecutionResult;
use async_trait::async_trait;
use chrono::Utc;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{JobFilter, JobStorage, StoredJob};
use crate::error::{Error, Result};

/// In-memory job storage.
#[derive(Clone)]
pub struct MemoryStorage {
    jobs: Arc<RwLock<FxHashMap<String, StoredJob>>>,
}

impl MemoryStorage {
    /// Create a new in-memory storage.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(FxHashMap::default())),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl JobStorage for MemoryStorage {
    async fn store_job(&self, job: &StoredJob) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.0.clone(), job.clone());
        Ok(())
    }

    async fn get_job(&self, job_id: &JobId) -> Result<Option<StoredJob>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.get(&job_id.0).cloned())
    }

    async fn update_status(&self, job_id: &JobId, status: JobStatus) -> Result<()> {
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

    async fn store_result(&self, job_id: &JobId, result: ExecutionResult) -> Result<()> {
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

    async fn list_jobs(&self, filter: JobFilter) -> Result<Vec<StoredJob>> {
        let jobs = self.jobs.read().await;

        let mut results: Vec<StoredJob> = jobs
            .values()
            .filter(|job| {
                // Filter by state
                if let Some(ref state) = filter.state {
                    if &job.status != state {
                        return false;
                    }
                }

                // Filter by backend
                if let Some(ref backend_id) = filter.backend_id {
                    if &job.backend_id != backend_id {
                        return false;
                    }
                }

                // Filter by time range
                if let Some(after) = filter.after {
                    if job.submitted_at < after {
                        return false;
                    }
                }

                if let Some(before) = filter.before {
                    if job.submitted_at > before {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort by submitted_at descending (most recent first)
        results.sort_by_key(|j| std::cmp::Reverse(j.submitted_at));

        // Apply limit
        results.truncate(filter.limit);

        Ok(results)
    }

    async fn delete_job(&self, job_id: &JobId) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        jobs.remove(&job_id.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::circuit::Circuit;

    #[tokio::test]
    async fn test_store_and_get_job() {
        let storage = MemoryStorage::new();

        let job = StoredJob {
            id: JobId::new("test-123".to_string()),
            circuit: Circuit::with_size("test", 2, 0),
            backend_id: "simulator".to_string(),
            shots: 1000,
            status: JobStatus::Queued,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        storage.store_job(&job).await.unwrap();

        let retrieved = storage.get_job(&job.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, job.id);
    }

    #[tokio::test]
    async fn test_update_status_with_timestamps() {
        let storage = MemoryStorage::new();

        let job = StoredJob {
            id: JobId::new("test-456".to_string()),
            circuit: Circuit::with_size("test", 2, 0),
            backend_id: "simulator".to_string(),
            shots: 1000,
            status: JobStatus::Queued,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        storage.store_job(&job).await.unwrap();

        // Update to Running
        storage
            .update_status(&job.id, JobStatus::Running)
            .await
            .unwrap();

        let retrieved = storage.get_job(&job.id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Running);
        assert!(retrieved.started_at.is_some());

        // Update to Completed
        storage
            .update_status(&job.id, JobStatus::Completed)
            .await
            .unwrap();

        let retrieved = storage.get_job(&job.id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, JobStatus::Completed);
        assert!(retrieved.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_list_jobs_with_filter() {
        let storage = MemoryStorage::new();

        // Create multiple jobs
        for i in 0..5 {
            let job = StoredJob {
                id: JobId::new(format!("test-{i}")),
                circuit: Circuit::with_size("test", 2, 0),
                backend_id: if i < 3 { "sim" } else { "iqm" }.to_string(),
                shots: 1000,
                status: if i < 2 {
                    JobStatus::Queued
                } else {
                    JobStatus::Completed
                },
                submitted_at: Utc::now(),
                started_at: None,
                completed_at: None,
                result: None,
            };
            storage.store_job(&job).await.unwrap();
        }

        // Filter by backend
        let filter = JobFilter::new().with_backend("sim".to_string());
        let results = storage.list_jobs(filter).await.unwrap();
        assert_eq!(results.len(), 3);

        // Filter by status
        let filter = JobFilter::new().with_state(JobStatus::Queued);
        let results = storage.list_jobs(filter).await.unwrap();
        assert_eq!(results.len(), 2);

        // Filter with limit
        let filter = JobFilter::new().with_limit(2);
        let results = storage.list_jobs(filter).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_job() {
        let storage = MemoryStorage::new();

        let job = StoredJob {
            id: JobId::new("test-delete".to_string()),
            circuit: Circuit::with_size("test", 2, 0),
            backend_id: "simulator".to_string(),
            shots: 1000,
            status: JobStatus::Queued,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        storage.store_job(&job).await.unwrap();
        assert!(storage.get_job(&job.id).await.unwrap().is_some());

        storage.delete_job(&job.id).await.unwrap();
        assert!(storage.get_job(&job.id).await.unwrap().is_none());

        // Delete again should not error (idempotent)
        storage.delete_job(&job.id).await.unwrap();
    }

    #[tokio::test]
    async fn test_job_not_found() {
        let storage = MemoryStorage::new();
        let job_id = JobId::new("nonexistent".to_string());

        let result = storage.update_status(&job_id, JobStatus::Running).await;
        assert!(matches!(result, Err(Error::JobNotFound(_))));
    }
}
