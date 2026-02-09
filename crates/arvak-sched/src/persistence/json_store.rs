//! JSON file-based persistence for development and testing.

use std::path::{Path, PathBuf};

use arvak_hal::ExecutionResult;
use async_trait::async_trait;
use tokio::fs;
use tokio::sync::RwLock;

use crate::error::{SchedError, SchedResult};
use crate::job::{JobFilter, ScheduledJob, ScheduledJobId, ScheduledJobStatus};
use crate::persistence::StateStore;
use crate::workflow::{Workflow, WorkflowId};

/// JSON file-based state store.
///
/// Stores each job as a separate JSON file. Suitable for development
/// and testing, not recommended for production use.
pub struct JsonStore {
    /// Base directory for storage.
    base_dir: PathBuf,

    /// In-memory cache of jobs.
    cache: RwLock<rustc_hash::FxHashMap<ScheduledJobId, ScheduledJob>>,
}

impl JsonStore {
    /// Create a new JSON store at the given path.
    pub async fn new(base_dir: impl AsRef<Path>) -> SchedResult<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();

        // Create directories
        fs::create_dir_all(base_dir.join("jobs")).await?;
        fs::create_dir_all(base_dir.join("results")).await?;
        fs::create_dir_all(base_dir.join("workflows")).await?;

        let store = Self {
            base_dir,
            cache: RwLock::new(rustc_hash::FxHashMap::default()),
        };

        // Load existing jobs into cache
        store.load_all_jobs().await?;

        Ok(store)
    }

    /// Create a new JSON store in a temporary directory.
    pub async fn temp() -> SchedResult<Self> {
        let temp_dir = std::env::temp_dir().join(format!("arvak-sched-{}", uuid::Uuid::new_v4()));
        Self::new(temp_dir).await
    }

    fn job_path(&self, job_id: &ScheduledJobId) -> PathBuf {
        self.base_dir.join("jobs").join(format!("{job_id}.json"))
    }

    fn result_path(&self, job_id: &ScheduledJobId) -> PathBuf {
        self.base_dir.join("results").join(format!("{job_id}.json"))
    }

    fn workflow_path(&self, workflow_id: &WorkflowId) -> PathBuf {
        self.base_dir
            .join("workflows")
            .join(format!("{workflow_id}.json"))
    }

    async fn load_all_jobs(&self) -> SchedResult<()> {
        let jobs_dir = self.base_dir.join("jobs");
        let mut cache = self.cache.write().await;

        let mut entries = fs::read_dir(&jobs_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                match fs::read_to_string(&path).await {
                    Ok(content) => match serde_json::from_str::<ScheduledJob>(&content) {
                        Ok(job) => {
                            cache.insert(job.id.clone(), job);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse job file {:?}: {}", path, e);
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read job file {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl StateStore for JsonStore {
    async fn save_job(&self, job: &ScheduledJob) -> SchedResult<()> {
        let path = self.job_path(&job.id);
        let json = serde_json::to_string_pretty(job)?;
        fs::write(&path, json).await?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.insert(job.id.clone(), job.clone());

        Ok(())
    }

    async fn load_job(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ScheduledJob>> {
        // Check cache first
        let cache = self.cache.read().await;
        if let Some(job) = cache.get(job_id) {
            return Ok(Some(job.clone()));
        }
        drop(cache);

        // Load from file
        let path = self.job_path(job_id);
        match fs::read_to_string(&path).await {
            Ok(content) => {
                let job: ScheduledJob = serde_json::from_str(&content)?;
                // Update cache
                let mut cache = self.cache.write().await;
                cache.insert(job.id.clone(), job.clone());
                Ok(Some(job))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SchedError::IoError(e)),
        }
    }

    async fn update_status(
        &self,
        job_id: &ScheduledJobId,
        status: ScheduledJobStatus,
    ) -> SchedResult<()> {
        let mut cache = self.cache.write().await;

        if let Some(job) = cache.get_mut(job_id) {
            job.status = status.clone();

            // Update completion time if terminal
            if status.is_terminal() {
                job.completed_at = Some(chrono::Utc::now());
            }

            // Write to file
            let path = self.job_path(job_id);
            let json = serde_json::to_string_pretty(&*job)?;
            fs::write(&path, json).await?;

            Ok(())
        } else {
            Err(SchedError::JobNotFound(job_id.to_string()))
        }
    }

    async fn delete_job(&self, job_id: &ScheduledJobId) -> SchedResult<bool> {
        let path = self.job_path(job_id);

        // Remove from cache
        let mut cache = self.cache.write().await;
        let was_present = cache.remove(job_id).is_some();

        // Remove file
        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(was_present),
            Err(e) => Err(SchedError::IoError(e)),
        }
    }

    async fn list_jobs(&self, filter: &JobFilter) -> SchedResult<Vec<ScheduledJob>> {
        let cache = self.cache.read().await;

        let mut jobs: Vec<_> = cache
            .values()
            .filter(|job| filter.matches(job))
            .cloned()
            .collect();

        // Sort by priority (descending), then by creation time (ascending)
        jobs.sort_by(|a, b| match b.priority.cmp(&a.priority) {
            std::cmp::Ordering::Equal => a.created_at.cmp(&b.created_at),
            other => other,
        });

        // Apply limit
        if let Some(limit) = filter.limit {
            jobs.truncate(limit);
        }

        Ok(jobs)
    }

    async fn save_result(
        &self,
        job_id: &ScheduledJobId,
        result: &ExecutionResult,
    ) -> SchedResult<()> {
        let path = self.result_path(job_id);
        let json = serde_json::to_string_pretty(result)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    async fn load_result(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ExecutionResult>> {
        let path = self.result_path(job_id);
        match fs::read_to_string(&path).await {
            Ok(content) => {
                let result: ExecutionResult = serde_json::from_str(&content)?;
                Ok(Some(result))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SchedError::IoError(e)),
        }
    }

    async fn save_workflow(&self, workflow: &Workflow) -> SchedResult<()> {
        let path = self.workflow_path(&workflow.id);
        let json = serde_json::to_string_pretty(workflow)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    async fn load_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<Option<Workflow>> {
        let path = self.workflow_path(workflow_id);
        match fs::read_to_string(&path).await {
            Ok(content) => {
                let workflow: Workflow = serde_json::from_str(&content)?;
                Ok(Some(workflow))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(SchedError::IoError(e)),
        }
    }

    async fn delete_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<bool> {
        let path = self.workflow_path(workflow_id);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(SchedError::IoError(e)),
        }
    }

    async fn list_workflows(&self) -> SchedResult<Vec<WorkflowId>> {
        let workflows_dir = self.base_dir.join("workflows");
        let mut workflow_ids = Vec::new();

        let mut entries = fs::read_dir(&workflows_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Some(stem) = path.file_stem() {
                    if let Some(stem_str) = stem.to_str() {
                        if let Ok(id) = WorkflowId::parse(stem_str) {
                            workflow_ids.push(id);
                        }
                    }
                }
            }
        }

        Ok(workflow_ids)
    }

    async fn cleanup_old_jobs(&self, max_age_seconds: u64) -> SchedResult<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_seconds as i64);
        let mut removed = 0;

        let cache = self.cache.read().await;
        let to_remove: Vec<_> = cache
            .values()
            .filter(|job| job.status.is_terminal() && job.completed_at.is_some_and(|t| t < cutoff))
            .map(|job| job.id.clone())
            .collect();
        drop(cache);

        for job_id in to_remove {
            if self.delete_job(&job_id).await? {
                // Also delete result if exists
                let result_path = self.result_path(&job_id);
                let _ = fs::remove_file(&result_path).await;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};

    #[tokio::test]
    async fn test_json_store_basic() {
        let store = JsonStore::temp().await.unwrap();

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("test_job", circuit).with_priority(Priority::high());
        let job_id = job.id.clone();

        // Save
        store.save_job(&job).await.unwrap();

        // Load
        let loaded = store.load_job(&job_id).await.unwrap().unwrap();
        assert_eq!(loaded.name, "test_job");
        assert_eq!(loaded.priority, Priority::HIGH);

        // Update status
        store
            .update_status(
                &job_id,
                ScheduledJobStatus::SlurmQueued {
                    slurm_job_id: "12345".to_string(),
                },
            )
            .await
            .unwrap();

        let updated = store.load_job(&job_id).await.unwrap().unwrap();
        assert!(matches!(
            updated.status,
            ScheduledJobStatus::SlurmQueued { .. }
        ));

        // Delete
        assert!(store.delete_job(&job_id).await.unwrap());
        assert!(store.load_job(&job_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_json_store_list_filter() {
        let store = JsonStore::temp().await.unwrap();

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");

        let job1 = ScheduledJob::new("job1", circuit.clone()).with_priority(Priority::low());
        let job2 = ScheduledJob::new("job2", circuit.clone()).with_priority(Priority::high());

        store.save_job(&job1).await.unwrap();
        store.save_job(&job2).await.unwrap();

        // List all pending
        let jobs = store.list_jobs(&JobFilter::pending()).await.unwrap();
        assert_eq!(jobs.len(), 2);
        // Should be sorted by priority (high first)
        assert_eq!(jobs[0].name, "job2");
        assert_eq!(jobs[1].name, "job1");
    }
}
