//! Job lifecycle types.
//!
//! `JobId` and `JobStatus` are re-exported from the HAL Contract spec crate.
//! `Job` is an Arvak extension for orchestration tracking.
//!
//! # HAL Contract v2
//!
//! The job state machine:
//!
//! ```text
//!   submit() ──→ Queued ──→ Running ──→ Completed
//!                  │           │
//!                  │           ├──→ Failed(reason)
//!                  │           │
//!                  └───────────┴──→ Cancelled
//! ```
//!
//! **Invariants:**
//! - `submit()` MUST return `Queued`.
//! - Transitions are monotonic — a job never moves backward.
//! - Terminal states (`Completed`, `Failed`, `Cancelled`) are permanent.
//! - `result()` is only valid when status is `Completed`.

// ── Re-exported from HAL Contract spec ──────────────────────────────────────
pub use hal_contract::job::{JobId, JobStatus};

// ── Arvak extension ─────────────────────────────────────────────────────────

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Arvak extension — not part of HAL Contract v2 spec.
/// A job with metadata for orchestration tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// The job identifier.
    pub id: JobId,
    /// Current status.
    pub status: JobStatus,
    /// Number of shots requested.
    pub shots: u32,
    /// Time the job was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    /// Time the job started running.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// Time the job finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// Backend the job was submitted to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
}

impl Job {
    /// Create a new job.
    pub fn new(id: impl Into<JobId>, shots: u32) -> Self {
        Self {
            id: id.into(),
            status: JobStatus::Queued,
            shots,
            created_at: Some(Utc::now()),
            started_at: None,
            finished_at: None,
            backend: None,
        }
    }

    /// Set the backend name.
    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    /// Update the status.
    pub fn with_status(mut self, status: JobStatus) -> Self {
        self.status = status;
        if matches!(self.status, JobStatus::Running) && self.started_at.is_none() {
            self.started_at = Some(Utc::now());
        }
        if self.status.is_terminal() && self.finished_at.is_none() {
            self.finished_at = Some(Utc::now());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_terminal() {
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed("error".into()).is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn test_job_creation() {
        let job = Job::new("job-123", 1000).with_backend("simulator");

        assert_eq!(job.id.0, "job-123");
        assert_eq!(job.shots, 1000);
        assert_eq!(job.backend, Some("simulator".to_string()));
        assert!(job.created_at.is_some());
    }
}
