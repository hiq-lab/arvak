//! SQLite-based persistence for production use.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use hiq_hal::ExecutionResult;
use rusqlite::Connection;
use std::sync::Mutex;

use crate::error::{SchedError, SchedResult};
use crate::job::{JobFilter, ScheduledJob, ScheduledJobId, ScheduledJobStatus};
use crate::persistence::StateStore;
use crate::workflow::{Workflow, WorkflowId};

/// SQLite-based state store.
///
/// Provides persistent storage with ACID guarantees. Recommended for
/// production use.
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Create a new SQLite store at the given path.
    pub fn new(path: impl AsRef<Path>) -> SchedResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema_sync()?;
        Ok(store)
    }

    /// Create a new in-memory SQLite store.
    pub fn in_memory() -> SchedResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema_sync()?;
        Ok(store)
    }

    fn init_schema_sync(&self) -> SchedResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL,
                submitted_at TEXT,
                completed_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_priority ON jobs(priority);
            CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);

            CREATE TABLE IF NOT EXISTS results (
                job_id TEXT PRIMARY KEY,
                data TEXT NOT NULL,
                FOREIGN KEY (job_id) REFERENCES jobs(id)
            );

            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

#[async_trait]
impl StateStore for SqliteStore {
    async fn save_job(&self, job: &ScheduledJob) -> SchedResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        let data = serde_json::to_string(job)?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO jobs (id, name, status, priority, data, created_at, submitted_at, completed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            rusqlite::params![
                job.id.to_string(),
                job.name,
                job.status.name(),
                job.priority.value(),
                data,
                job.created_at.to_rfc3339(),
                job.submitted_at.map(|t| t.to_rfc3339()),
                job.completed_at.map(|t| t.to_rfc3339()),
            ],
        )?;

        Ok(())
    }

    async fn load_job(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ScheduledJob>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        let mut stmt = conn.prepare("SELECT data FROM jobs WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![job_id.to_string()])?;

        if let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let job: ScheduledJob = serde_json::from_str(&data)?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    async fn update_status(
        &self,
        job_id: &ScheduledJobId,
        status: ScheduledJobStatus,
    ) -> SchedResult<()> {
        // Load, update, save
        let mut job = self
            .load_job(job_id)
            .await?
            .ok_or_else(|| SchedError::JobNotFound(job_id.to_string()))?;

        job.status = status.clone();
        if status.is_terminal() {
            job.completed_at = Some(chrono::Utc::now());
        }

        self.save_job(&job).await
    }

    async fn delete_job(&self, job_id: &ScheduledJobId) -> SchedResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        let deleted = conn.execute(
            "DELETE FROM jobs WHERE id = ?1",
            rusqlite::params![job_id.to_string()],
        )?;
        Ok(deleted > 0)
    }

    async fn list_jobs(&self, filter: &JobFilter) -> SchedResult<Vec<ScheduledJob>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        // Build query based on filter
        let mut sql = String::from("SELECT data FROM jobs WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref statuses) = filter.status {
            let placeholders: Vec<_> = statuses
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect();
            sql.push_str(&format!(" AND status IN ({})", placeholders.join(", ")));
            for s in statuses {
                params.push(Box::new(s.clone()));
            }
        }

        if filter.pending_only {
            sql.push_str(" AND status IN ('Pending', 'WaitingOnDependencies')");
        }

        if filter.running_only {
            sql.push_str(" AND status IN ('SlurmRunning', 'QuantumRunning')");
        }

        if let Some(min_priority) = filter.min_priority {
            let idx = params.len() + 1;
            sql.push_str(&format!(" AND priority >= ?{}", idx));
            params.push(Box::new(min_priority.value() as i64));
        }

        if let Some(max_priority) = filter.max_priority {
            let idx = params.len() + 1;
            sql.push_str(&format!(" AND priority <= ?{}", idx));
            params.push(Box::new(max_priority.value() as i64));
        }

        sql.push_str(" ORDER BY priority DESC, created_at ASC");

        if let Some(limit) = filter.limit {
            let idx = params.len() + 1;
            sql.push_str(&format!(" LIMIT ?{}", idx));
            params.push(Box::new(limit as i64));
        }

        let mut stmt = conn.prepare(&sql)?;

        // Convert params to references for query
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let mut rows = stmt.query(params_refs.as_slice())?;

        let mut jobs = Vec::new();
        while let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let job: ScheduledJob = serde_json::from_str(&data)?;

            // Apply additional filters that can't be done in SQL
            if let Some(ref pattern) = filter.name_pattern {
                if !job.name.contains(pattern) {
                    continue;
                }
            }

            if let Some(after) = filter.created_after {
                if job.created_at < after {
                    continue;
                }
            }

            if let Some(before) = filter.created_before {
                if job.created_at > before {
                    continue;
                }
            }

            jobs.push(job);
        }

        Ok(jobs)
    }

    async fn save_result(
        &self,
        job_id: &ScheduledJobId,
        result: &ExecutionResult,
    ) -> SchedResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        let data = serde_json::to_string(result)?;

        conn.execute(
            "INSERT OR REPLACE INTO results (job_id, data) VALUES (?1, ?2)",
            rusqlite::params![job_id.to_string(), data],
        )?;

        Ok(())
    }

    async fn load_result(&self, job_id: &ScheduledJobId) -> SchedResult<Option<ExecutionResult>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        let mut stmt = conn.prepare("SELECT data FROM results WHERE job_id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![job_id.to_string()])?;

        if let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let result: ExecutionResult = serde_json::from_str(&data)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn save_workflow(&self, workflow: &Workflow) -> SchedResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        let data = serde_json::to_string(workflow)?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO workflows (id, name, data, created_at)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            rusqlite::params![
                workflow.id.to_string(),
                workflow.name,
                data,
                workflow.created_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    async fn load_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<Option<Workflow>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        let mut stmt = conn.prepare("SELECT data FROM workflows WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![workflow_id.to_string()])?;

        if let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let workflow: Workflow = serde_json::from_str(&data)?;
            Ok(Some(workflow))
        } else {
            Ok(None)
        }
    }

    async fn delete_workflow(&self, workflow_id: &WorkflowId) -> SchedResult<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;
        let deleted = conn.execute(
            "DELETE FROM workflows WHERE id = ?1",
            rusqlite::params![workflow_id.to_string()],
        )?;
        Ok(deleted > 0)
    }

    async fn list_workflows(&self) -> SchedResult<Vec<WorkflowId>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        let mut stmt = conn.prepare("SELECT id FROM workflows ORDER BY created_at DESC")?;
        let mut rows = stmt.query([])?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next()? {
            let id_str: String = row.get(0)?;
            if let Ok(id) = WorkflowId::parse(&id_str) {
                ids.push(id);
            }
        }

        Ok(ids)
    }

    async fn cleanup_old_jobs(&self, max_age_seconds: u64) -> SchedResult<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_seconds as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let conn = self
            .conn
            .lock()
            .map_err(|e| SchedError::DatabaseError(e.to_string()))?;

        // Delete results for old jobs first (foreign key)
        conn.execute(
            r#"
            DELETE FROM results WHERE job_id IN (
                SELECT id FROM jobs
                WHERE completed_at IS NOT NULL
                AND completed_at < ?1
                AND status IN ('Completed', 'Failed', 'Cancelled')
            )
            "#,
            rusqlite::params![cutoff_str],
        )?;

        // Delete old jobs
        let deleted = conn.execute(
            r#"
            DELETE FROM jobs
            WHERE completed_at IS NOT NULL
            AND completed_at < ?1
            AND status IN ('Completed', 'Failed', 'Cancelled')
            "#,
            rusqlite::params![cutoff_str],
        )?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{CircuitSpec, Priority};

    #[tokio::test]
    async fn test_sqlite_store_basic() {
        let store = SqliteStore::in_memory().unwrap();

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
    async fn test_sqlite_store_list() {
        let store = SqliteStore::in_memory().unwrap();

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

        // List with limit
        let jobs = store
            .list_jobs(&JobFilter::pending().with_limit(1))
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "job2");
    }

    #[tokio::test]
    async fn test_sqlite_store_results() {
        use hiq_hal::Counts;

        let store = SqliteStore::in_memory().unwrap();

        let circuit = CircuitSpec::from_qasm("OPENQASM 3.0; qubit[2] q;");
        let job = ScheduledJob::new("test_job", circuit);
        let job_id = job.id.clone();

        store.save_job(&job).await.unwrap();

        // Save result
        let counts = Counts::from_pairs([("00", 500u64), ("11", 500u64)]);
        let result = ExecutionResult::new(counts, 1000);
        store.save_result(&job_id, &result).await.unwrap();

        // Load result
        let loaded = store.load_result(&job_id).await.unwrap().unwrap();
        assert_eq!(loaded.shots, 1000);
        assert_eq!(loaded.counts.get("00"), 500);
    }
}
