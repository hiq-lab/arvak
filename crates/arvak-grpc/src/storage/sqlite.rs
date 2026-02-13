//! SQLite storage backend implementation.
//!
//! This module provides persistent storage using SQLite with the following features:
//! - Job metadata and status storage
//! - Result storage with efficient serialization
//! - Job filtering and querying
//! - Automatic schema migrations

use crate::error::{Error, Result};
use crate::storage::{JobFilter, JobStorage, StoredJob};
use arvak_hal::job::{JobId, JobStatus};
use arvak_hal::result::ExecutionResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::task;

/// SQLite storage backend.
///
/// Uses rusqlite with a connection pool for thread-safe access.
/// Jobs and results are stored in separate tables for efficient querying.
#[derive(Clone)]
pub struct SqliteStorage {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Create a new SQLite storage backend.
    ///
    /// If the database file doesn't exist, it will be created.
    /// Schema migrations are applied automatically.
    ///
    /// **Important:** This constructor acquires a blocking `Mutex` to run
    /// `init_schema()`. It must be called outside of an async runtime context
    /// (e.g., before `tokio::main` starts, or inside `spawn_blocking`), otherwise
    /// it will block the executor thread.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let storage = Self {
            connection: Arc::new(Mutex::new(conn)),
        };

        storage.init_schema()?;

        Ok(storage)
    }

    /// Initialize the database schema.
    fn init_schema(&self) -> Result<()> {
        let conn = self
            .connection
            .lock()
            .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

        // Jobs table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS jobs (
                job_id TEXT PRIMARY KEY,
                circuit_json TEXT NOT NULL,
                backend_id TEXT NOT NULL,
                shots INTEGER NOT NULL,
                status TEXT NOT NULL,
                submitted_at INTEGER NOT NULL,
                started_at INTEGER,
                completed_at INTEGER,
                error_message TEXT
            )",
            [],
        )?;

        // Results table (separate for efficiency)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS job_results (
                job_id TEXT PRIMARY KEY,
                counts_json TEXT NOT NULL,
                shots INTEGER NOT NULL,
                execution_time_ms INTEGER,
                metadata_json TEXT,
                FOREIGN KEY (job_id) REFERENCES jobs(job_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Indexes for common queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_jobs_backend ON jobs(backend_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_jobs_submitted ON jobs(submitted_at)",
            [],
        )?;

        Ok(())
    }

    /// Serialize a circuit to a string representation.
    ///
    /// Note: Circuit doesn't implement Serialize, so we store a placeholder.
    /// The circuit is not needed after job submission - only metadata matters.
    fn serialize_circuit(_circuit: &arvak_ir::circuit::Circuit) -> Result<String> {
        // Store empty placeholder - circuit not needed for storage queries
        Ok(String::from("{}"))
    }

    /// Deserialize a circuit from string.
    ///
    /// Returns an empty circuit since we don't store the actual circuit.
    fn deserialize_circuit(_json: &str) -> Result<arvak_ir::circuit::Circuit> {
        // Return empty circuit - not needed for job queries
        Ok(arvak_ir::circuit::Circuit::new("stored"))
    }

    /// Convert JobStatus to string for storage.
    // TODO: Extract shared status conversion to storage/mod.rs (duplicated in postgres.rs)
    fn status_to_string(status: &JobStatus) -> String {
        match status {
            JobStatus::Queued => "queued".to_string(),
            JobStatus::Running => "running".to_string(),
            JobStatus::Completed => "completed".to_string(),
            JobStatus::Failed(msg) => format!("failed:{}", msg),
            JobStatus::Cancelled => "cancelled".to_string(),
        }
    }

    /// Convert string to JobStatus.
    fn string_to_status(s: &str) -> Result<JobStatus> {
        if s == "queued" {
            Ok(JobStatus::Queued)
        } else if s == "running" {
            Ok(JobStatus::Running)
        } else if s == "completed" {
            Ok(JobStatus::Completed)
        } else if s == "cancelled" {
            Ok(JobStatus::Cancelled)
        } else if let Some(msg) = s.strip_prefix("failed:") {
            Ok(JobStatus::Failed(msg.to_string()))
        } else {
            Err(Error::StorageError(format!("Invalid status: {}", s)))
        }
    }
}

#[async_trait]
impl JobStorage for SqliteStorage {
    async fn store_job(&self, job: &StoredJob) -> Result<()> {
        let job = job.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            let circuit_json = Self::serialize_circuit(&job.circuit)?;
            let status_str = Self::status_to_string(&job.status);
            let error_msg = if let JobStatus::Failed(msg) = &job.status {
                Some(msg.as_str())
            } else {
                None
            };

            conn.execute(
                "INSERT OR REPLACE INTO jobs (
                    job_id, circuit_json, backend_id, shots, status,
                    submitted_at, started_at, completed_at, error_message
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    job.id.0,
                    circuit_json,
                    job.backend_id,
                    job.shots,
                    status_str,
                    job.submitted_at.timestamp(),
                    job.started_at.map(|t| t.timestamp()),
                    job.completed_at.map(|t| t.timestamp()),
                    error_msg,
                ],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn get_job(&self, job_id: &JobId) -> Result<Option<StoredJob>> {
        let job_id = job_id.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            let result = conn
                .query_row(
                    "SELECT job_id, circuit_json, backend_id, shots, status,
                            submitted_at, started_at, completed_at
                     FROM jobs WHERE job_id = ?1",
                    params![job_id.0],
                    |row| {
                        let circuit_json: String = row.get(1)?;
                        let circuit = Self::deserialize_circuit(&circuit_json)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                        let status_str: String = row.get(4)?;
                        let status = Self::string_to_status(&status_str)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                        let submitted_ts: i64 = row.get(5)?;
                        let started_ts: Option<i64> = row.get(6)?;
                        let completed_ts: Option<i64> = row.get(7)?;

                        Ok(StoredJob {
                            id: job_id.clone(),
                            circuit,
                            backend_id: row.get(2)?,
                            shots: row.get(3)?,
                            status,
                            submitted_at: DateTime::from_timestamp(submitted_ts, 0)
                                .unwrap_or_else(|| Utc::now()),
                            started_at: started_ts.and_then(|ts| DateTime::from_timestamp(ts, 0)),
                            completed_at: completed_ts
                                .and_then(|ts| DateTime::from_timestamp(ts, 0)),
                            result: None, // Results are stored separately
                        })
                    },
                )
                .optional()?;

            Ok(result)
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn update_status(&self, job_id: &JobId, status: JobStatus) -> Result<()> {
        let job_id = job_id.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            let status_str = Self::status_to_string(&status);
            let error_msg = if let JobStatus::Failed(msg) = &status {
                Some(msg.as_str())
            } else {
                None
            };

            // Update timestamps based on status
            let now = Utc::now().timestamp();
            match status {
                JobStatus::Running => {
                    conn.execute(
                        "UPDATE jobs SET status = ?1, started_at = ?2, error_message = ?3
                         WHERE job_id = ?4",
                        params![status_str, now, error_msg, job_id.0],
                    )?;
                }
                JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled => {
                    conn.execute(
                        "UPDATE jobs SET status = ?1, completed_at = ?2, error_message = ?3
                         WHERE job_id = ?4",
                        params![status_str, now, error_msg, job_id.0],
                    )?;
                }
                _ => {
                    conn.execute(
                        "UPDATE jobs SET status = ?1, error_message = ?2
                         WHERE job_id = ?3",
                        params![status_str, error_msg, job_id.0],
                    )?;
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn store_result(&self, job_id: &JobId, result: ExecutionResult) -> Result<()> {
        let job_id = job_id.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            // Serialize counts to JSON
            let counts_json = serde_json::to_string(&result.counts)
                .map_err(|e| Error::StorageError(format!("Failed to serialize counts: {}", e)))?;

            // Serialize metadata to JSON (or null if Value::Null)
            let metadata_json = if result.metadata.is_null() {
                None
            } else {
                Some(serde_json::to_string(&result.metadata).map_err(|e| {
                    Error::StorageError(format!("Failed to serialize metadata: {}", e))
                })?)
            };

            conn.execute(
                "INSERT OR REPLACE INTO job_results (
                    job_id, counts_json, shots, execution_time_ms, metadata_json
                ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    job_id.0,
                    counts_json,
                    result.shots,
                    result.execution_time_ms,
                    metadata_json,
                ],
            )?;

            // Update job status to completed
            conn.execute(
                "UPDATE jobs SET status = ?1, completed_at = ?2 WHERE job_id = ?3",
                params!["completed", Utc::now().timestamp(), job_id.0],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn get_result(&self, job_id: &JobId) -> Result<ExecutionResult> {
        let job_id = job_id.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            conn.query_row(
                "SELECT counts_json, shots, execution_time_ms, metadata_json
                 FROM job_results WHERE job_id = ?1",
                params![job_id.0],
                |row| {
                    let counts_json: String = row.get(0)?;
                    let counts = serde_json::from_str(&counts_json)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                    let metadata_json: Option<String> = row.get(3)?;
                    let metadata = if let Some(json) = metadata_json {
                        serde_json::from_str(&json)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    } else {
                        serde_json::Value::Null
                    };

                    Ok(ExecutionResult {
                        counts,
                        shots: row.get(1)?,
                        execution_time_ms: row.get(2)?,
                        metadata,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::JobNotFound(job_id.0.clone()),
                _ => Error::from(e),
            })
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn list_jobs(&self, filter: JobFilter) -> Result<Vec<StoredJob>> {
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            // Build query based on filter
            let mut query = String::from(
                "SELECT job_id, circuit_json, backend_id, shots, status,
                                                 submitted_at, started_at, completed_at
                                          FROM jobs WHERE 1=1",
            );
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(status) = filter.state {
                query.push_str(" AND status LIKE ?");
                params.push(Box::new(format!("{}%", Self::status_to_string(&status))));
            }

            if let Some(backend_id) = filter.backend_id {
                query.push_str(" AND backend_id = ?");
                params.push(Box::new(backend_id));
            }

            if let Some(after) = filter.after {
                query.push_str(" AND submitted_at >= ?");
                params.push(Box::new(after.timestamp()));
            }

            if let Some(before) = filter.before {
                query.push_str(" AND submitted_at <= ?");
                params.push(Box::new(before.timestamp()));
            }

            query.push_str(" ORDER BY submitted_at DESC LIMIT ?");
            params.push(Box::new(filter.limit as i64));

            let mut stmt = conn.prepare(&query)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

            let jobs = stmt
                .query_map(param_refs.as_slice(), |row| {
                    let circuit_json: String = row.get(1)?;
                    let circuit = Self::deserialize_circuit(&circuit_json)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                    let job_id: String = row.get(0)?;
                    let status_str: String = row.get(4)?;
                    let status = Self::string_to_status(&status_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                    let submitted_ts: i64 = row.get(5)?;
                    let started_ts: Option<i64> = row.get(6)?;
                    let completed_ts: Option<i64> = row.get(7)?;

                    Ok(StoredJob {
                        id: JobId::new(job_id),
                        circuit,
                        backend_id: row.get(2)?,
                        shots: row.get(3)?,
                        status,
                        submitted_at: DateTime::from_timestamp(submitted_ts, 0)
                            .unwrap_or_else(|| Utc::now()),
                        started_at: started_ts.and_then(|ts| DateTime::from_timestamp(ts, 0)),
                        completed_at: completed_ts.and_then(|ts| DateTime::from_timestamp(ts, 0)),
                        result: None,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(jobs)
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }

    async fn delete_job(&self, job_id: &JobId) -> Result<()> {
        let job_id = job_id.clone();
        let conn = self.connection.clone();

        task::spawn_blocking(move || {
            let conn = conn
                .lock()
                .map_err(|_| Error::StorageError("database lock poisoned".into()))?;

            // Foreign key constraint will cascade delete to job_results
            conn.execute("DELETE FROM jobs WHERE job_id = ?1", params![job_id.0])?;

            Ok(())
        })
        .await
        .map_err(|e| Error::StorageError(format!("task join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::circuit::Circuit;
    use rustc_hash::FxHashMap;

    #[tokio::test]
    async fn test_sqlite_storage_lifecycle() {
        let storage = SqliteStorage::new(":memory:").unwrap();

        let circuit = Circuit::new("test");
        let job_id = JobId::new("test-job-1".to_string());

        let job = StoredJob {
            id: job_id.clone(),
            circuit,
            backend_id: "simulator".to_string(),
            shots: 1000,
            status: JobStatus::Queued,
            submitted_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
        };

        // Store job
        storage.store_job(&job).await.unwrap();

        // Retrieve job
        let retrieved = storage.get_job(&job_id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, job_id);
        assert_eq!(retrieved.backend_id, "simulator");

        // Update status
        storage
            .update_status(&job_id, JobStatus::Running)
            .await
            .unwrap();
        let updated = storage.get_job(&job_id).await.unwrap().unwrap();
        assert!(matches!(updated.status, JobStatus::Running));

        // Store result
        use arvak_hal::result::Counts;
        let counts = Counts::from_pairs([("00", 500), ("11", 500)]);

        let result = ExecutionResult {
            counts,
            shots: 1000,
            execution_time_ms: Some(100),
            metadata: serde_json::Value::Null,
        };

        storage.store_result(&job_id, result).await.unwrap();

        // Retrieve result
        let retrieved_result = storage.get_result(&job_id).await.unwrap();
        assert_eq!(retrieved_result.shots, 1000);
        assert_eq!(retrieved_result.counts.len(), 2);

        // Delete job
        storage.delete_job(&job_id).await.unwrap();
        let deleted = storage.get_job(&job_id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_sqlite_storage_filtering() {
        let storage = SqliteStorage::new(":memory:").unwrap();

        // Create multiple jobs
        for i in 0..5 {
            let circuit = Circuit::new("test");
            let job = StoredJob {
                id: JobId::new(format!("job-{}", i)),
                circuit,
                backend_id: if i % 2 == 0 { "sim1" } else { "sim2" }.to_string(),
                shots: 1000,
                status: if i < 3 {
                    JobStatus::Completed
                } else {
                    JobStatus::Queued
                },
                submitted_at: Utc::now(),
                started_at: None,
                completed_at: None,
                result: None,
            };
            storage.store_job(&job).await.unwrap();
        }

        // Filter by status
        let filter = JobFilter {
            state: Some(JobStatus::Completed),
            backend_id: None,
            after: None,
            before: None,
            limit: 10,
        };

        let jobs = storage.list_jobs(filter).await.unwrap();
        assert_eq!(jobs.len(), 3);

        // Filter by backend
        let filter = JobFilter {
            state: None,
            backend_id: Some("sim1".to_string()),
            after: None,
            before: None,
            limit: 10,
        };

        let jobs = storage.list_jobs(filter).await.unwrap();
        assert_eq!(jobs.len(), 3); // 0, 2, 4
    }
}
