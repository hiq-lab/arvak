//! WebSocket event types for real-time updates.
//!
//! Placeholder for Phase 3 implementation.

use serde::Serialize;

/// Events sent to WebSocket clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DashboardEvent {
    /// Job status changed.
    JobStatusChanged { job_id: String, status: String },
    /// Job completed with results.
    JobCompleted { job_id: String },
    /// Backend availability changed.
    BackendStatusChanged { backend: String, available: bool },
}
