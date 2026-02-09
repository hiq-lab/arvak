//! Prometheus metrics for monitoring the Arvak gRPC service.
//!
//! This module provides comprehensive metrics tracking for:
//! - Job submissions, completions, failures
//! - Job execution timing and queue time
//! - RPC request duration
//! - Active and queued job counts
//! - Backend availability

use lazy_static::lazy_static;
use prometheus::{
    CounterVec, Encoder, Gauge, GaugeVec, HistogramVec, TextEncoder, register_counter_vec,
    register_gauge, register_gauge_vec, register_histogram_vec,
};

lazy_static! {
    /// Counter for total jobs submitted, labeled by backend_id
    pub static ref JOBS_SUBMITTED: CounterVec = register_counter_vec!(
        "arvak_jobs_submitted_total",
        "Total number of jobs submitted",
        &["backend_id"]
    )
    .unwrap();

    /// Counter for total jobs completed successfully, labeled by backend_id
    pub static ref JOBS_COMPLETED: CounterVec = register_counter_vec!(
        "arvak_jobs_completed_total",
        "Total number of jobs completed successfully",
        &["backend_id"]
    )
    .unwrap();

    /// Counter for total jobs failed, labeled by backend_id and error_type
    pub static ref JOBS_FAILED: CounterVec = register_counter_vec!(
        "arvak_jobs_failed_total",
        "Total number of jobs that failed",
        &["backend_id", "error_type"]
    )
    .unwrap();

    /// Counter for total jobs cancelled, labeled by backend_id
    pub static ref JOBS_CANCELLED: CounterVec = register_counter_vec!(
        "arvak_jobs_cancelled_total",
        "Total number of jobs cancelled",
        &["backend_id"]
    )
    .unwrap();

    /// Histogram for job execution duration in milliseconds
    pub static ref JOB_DURATION: HistogramVec = register_histogram_vec!(
        "arvak_job_duration_milliseconds",
        "Job execution duration in milliseconds",
        &["backend_id"],
        vec![10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0, 30000.0]
    )
    .unwrap();

    /// Histogram for job queue time (time from submission to start) in milliseconds
    pub static ref JOB_QUEUE_TIME: HistogramVec = register_histogram_vec!(
        "arvak_job_queue_time_milliseconds",
        "Time jobs spend in queue before execution starts",
        &["backend_id"],
        vec![10.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0]
    )
    .unwrap();

    /// Histogram for RPC request duration in milliseconds
    pub static ref RPC_DURATION: HistogramVec = register_histogram_vec!(
        "arvak_rpc_duration_milliseconds",
        "RPC request duration in milliseconds",
        &["method"],
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0]
    )
    .unwrap();

    /// Gauge for currently active (running) jobs
    pub static ref ACTIVE_JOBS: Gauge = register_gauge!(
        "arvak_active_jobs",
        "Number of currently active (running) jobs"
    )
    .unwrap();

    /// Gauge for currently queued jobs
    pub static ref QUEUED_JOBS: Gauge = register_gauge!(
        "arvak_queued_jobs",
        "Number of jobs currently in queue"
    )
    .unwrap();

    /// Gauge for backend availability (1 = available, 0 = unavailable)
    pub static ref BACKEND_AVAILABILITY: GaugeVec = register_gauge_vec!(
        "arvak_backend_available",
        "Backend availability status (1 = available, 0 = unavailable)",
        &["backend_id"]
    )
    .unwrap();
}

/// Metrics aggregator for the Arvak gRPC service.
///
/// This struct provides convenience methods for recording various metrics.
/// The actual metrics are stored in global static variables (`lazy_static`).
#[derive(Clone)]
pub struct Metrics;

impl Metrics {
    /// Create a new Metrics instance.
    pub fn new() -> Self {
        Self
    }

    /// Record a job submission.
    pub fn record_job_submitted(&self, backend_id: &str) {
        JOBS_SUBMITTED.with_label_values(&[backend_id]).inc();
        QUEUED_JOBS.inc();
    }

    /// Record a job starting execution.
    pub fn record_job_started(&self, _backend_id: &str) {
        QUEUED_JOBS.dec();
        ACTIVE_JOBS.inc();
    }

    /// Record a job completion with execution duration.
    pub fn record_job_completed(&self, backend_id: &str, duration_ms: u64) {
        JOBS_COMPLETED.with_label_values(&[backend_id]).inc();
        ACTIVE_JOBS.dec();
        JOB_DURATION
            .with_label_values(&[backend_id])
            .observe(duration_ms as f64);
    }

    /// Record a job failure.
    pub fn record_job_failed(&self, backend_id: &str, error_type: &str) {
        JOBS_FAILED
            .with_label_values(&[backend_id, error_type])
            .inc();
        ACTIVE_JOBS.dec();
    }

    /// Record a job cancellation.
    pub fn record_job_cancelled(&self, backend_id: &str) {
        JOBS_CANCELLED.with_label_values(&[backend_id]).inc();
        ACTIVE_JOBS.dec();
    }

    /// Record job queue time (time from submission to start).
    pub fn record_queue_time(&self, backend_id: &str, queue_time_ms: u64) {
        JOB_QUEUE_TIME
            .with_label_values(&[backend_id])
            .observe(queue_time_ms as f64);
    }

    /// Record an RPC request duration.
    pub fn record_rpc_duration(&self, method: &str, duration_ms: u64) {
        RPC_DURATION
            .with_label_values(&[method])
            .observe(duration_ms as f64);
    }

    /// Set backend availability status.
    pub fn set_backend_available(&self, backend_id: &str, available: bool) {
        let value = if available { 1.0 } else { 0.0 };
        BACKEND_AVAILABILITY
            .with_label_values(&[backend_id])
            .set(value);
    }

    /// Get current metrics as Prometheus text format.
    pub fn export(&self) -> Result<String, std::fmt::Error> {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();

        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|_| std::fmt::Error)?;

        String::from_utf8(buffer).map_err(|_| std::fmt::Error)
    }

    /// Get a snapshot of current metric values.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            active_jobs: ACTIVE_JOBS.get() as u64,
            queued_jobs: QUEUED_JOBS.get() as u64,
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of current metric values.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub active_jobs: u64,
    pub queued_jobs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        let snapshot = metrics.snapshot();

        // Prometheus metrics are global (lazy_static), so we can only verify
        // that snapshot() returns without panicking — values may carry over
        // from other tests running in the same process.
        let _ = snapshot;
    }

    #[test]
    fn test_job_lifecycle_metrics() {
        let metrics = Metrics::new();

        // Submit job
        metrics.record_job_submitted("simulator");
        let snapshot = metrics.snapshot();
        assert!(snapshot.queued_jobs > 0);

        // Start job
        metrics.record_job_started("simulator");
        let snapshot = metrics.snapshot();
        assert!(snapshot.active_jobs > 0);

        // Complete job
        metrics.record_job_completed("simulator", 1500);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.active_jobs, 0);
    }

    #[test]
    fn test_backend_availability() {
        let metrics = Metrics::new();

        metrics.set_backend_available("simulator", true);
        metrics.set_backend_available("ibm", false);

        // Verify it completes without panic by reaching this point
        let snapshot = metrics.snapshot();
        let _ = snapshot;
    }

    #[test]
    fn test_metrics_export() {
        let metrics = Metrics::new();

        metrics.record_job_submitted("test_backend");
        let exported = metrics.export().unwrap();

        // Should contain prometheus formatted metrics
        assert!(exported.contains("arvak_jobs_submitted_total"));
    }
}
