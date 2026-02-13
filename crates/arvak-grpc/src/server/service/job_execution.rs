//! Job execution logic for the gRPC service.

use arvak_hal::backend::Backend;
use arvak_hal::job::{JobId, JobStatus};
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::metrics::Metrics;
use crate::proto::JobState;
use crate::resource_manager::ResourceManager;
use crate::server::JobStore;

/// Execute a job synchronously (wait for completion).
// TODO: Extract shared logic into a single async function — `execute_job_sync`
// and `spawn_job_execution` duplicate the status-update / metrics / backend
// submit-wait-store sequence almost identically.
pub(super) async fn execute_job_sync(
    job_store: Arc<JobStore>,
    backend: Arc<dyn Backend>,
    job_id: JobId,
    metrics: Metrics,
    resources: Option<ResourceManager>,
) {
    // Get job details
    let job = match job_store.get_job(&job_id).await {
        Ok(job) => job,
        Err(e) => {
            error!("Failed to get job: {}", e);
            return;
        }
    };

    let backend_id = job.backend_id.clone();
    let submitted_at = job.submitted_at;

    // Update to RUNNING
    if let Err(e) = job_store.update_status(&job_id, JobStatus::Running).await {
        error!("Failed to update job status to running: {}", e);
        metrics.record_job_failed(&backend_id, "status_update_error");
        if let Some(ref resources) = resources {
            resources.job_cancelled_queued().await;
        }
        return;
    }

    metrics.record_job_started(&backend_id);
    if let Some(ref resources) = resources {
        resources.job_started().await;
    }
    let queue_time = chrono::Utc::now()
        .signed_duration_since(submitted_at)
        .num_milliseconds() as u64;
    metrics.record_queue_time(&backend_id, queue_time);

    // Execute on backend
    let execution_start = chrono::Utc::now();
    match backend.submit(&job.circuit, job.shots).await {
        Ok(backend_job_id) => match backend.wait(&backend_job_id).await {
            Ok(result) => {
                let duration = chrono::Utc::now()
                    .signed_duration_since(execution_start)
                    .num_milliseconds() as u64;

                if let Err(e) = job_store.store_result(&job_id, result).await {
                    error!("Failed to store job result: {}", e);
                    metrics.record_job_failed(&backend_id, "storage_error");
                } else {
                    metrics.record_job_completed(&backend_id, duration);
                }
                if let Some(ref resources) = resources {
                    resources.job_completed().await;
                }
            }
            Err(e) => {
                let error_msg = format!("Backend wait failed: {e}");
                metrics.record_job_failed(&backend_id, "backend_wait_error");
                let _ = job_store
                    .update_status(&job_id, JobStatus::Failed(error_msg))
                    .await;
                if let Some(ref resources) = resources {
                    resources.job_completed().await;
                }
            }
        },
        Err(e) => {
            let error_msg = format!("Backend submit failed: {e}");
            metrics.record_job_failed(&backend_id, "backend_submit_error");
            let _ = job_store
                .update_status(&job_id, JobStatus::Failed(error_msg))
                .await;
            if let Some(ref resources) = resources {
                resources.job_completed().await;
            }
        }
    }
}

/// Convert HAL `JobStatus` to protobuf `JobState`.
pub(super) fn to_proto_state(status: &JobStatus) -> JobState {
    match status {
        JobStatus::Queued => JobState::Queued,
        JobStatus::Running => JobState::Running,
        JobStatus::Completed => JobState::Completed,
        JobStatus::Failed(_) => JobState::Failed,
        JobStatus::Cancelled => JobState::Canceled,
    }
}

/// Spawn async task to execute a job.
#[instrument(skip(job_store, backend, metrics, resources), fields(job_id = %job_id.0))]
pub(super) fn spawn_job_execution(
    job_store: Arc<JobStore>,
    backend: Arc<dyn Backend>,
    job_id: JobId,
    metrics: Metrics,
    resources: Option<ResourceManager>,
) {
    tokio::spawn(async move {
        // Get job details to access backend_id and submission time
        let job = match job_store.get_job(&job_id).await {
            Ok(job) => job,
            Err(e) => {
                error!("Failed to get job: {}", e);
                return;
            }
        };

        let backend_id = job.backend_id.clone();
        let submitted_at = job.submitted_at;

        info!(backend_id = %backend_id, "Starting job execution");

        // Update to RUNNING
        if let Err(e) = job_store.update_status(&job_id, JobStatus::Running).await {
            error!("Failed to update job status to running: {}", e);
            metrics.record_job_failed(&backend_id, "status_update_error");
            if let Some(ref resources) = resources {
                resources.job_cancelled_queued().await;
            }
            return;
        }

        // Record job started and queue time
        metrics.record_job_started(&backend_id);
        if let Some(ref resources) = resources {
            resources.job_started().await;
        }
        let queue_time = chrono::Utc::now()
            .signed_duration_since(submitted_at)
            .num_milliseconds() as u64;
        metrics.record_queue_time(&backend_id, queue_time);

        // Execute on backend
        let execution_start = chrono::Utc::now();
        match backend.submit(&job.circuit, job.shots).await {
            Ok(backend_job_id) => {
                // Wait for backend to complete
                match backend.wait(&backend_job_id).await {
                    Ok(result) => {
                        let duration = chrono::Utc::now()
                            .signed_duration_since(execution_start)
                            .num_milliseconds() as u64;

                        if let Err(e) = job_store.store_result(&job_id, result).await {
                            error!("Failed to store job result: {}", e);
                            metrics.record_job_failed(&backend_id, "storage_error");
                        } else {
                            info!(duration_ms = duration, "Job completed successfully");
                            metrics.record_job_completed(&backend_id, duration);
                        }
                        if let Some(ref resources) = resources {
                            resources.job_completed().await;
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Backend wait failed: {e}");
                        warn!(error = %e, "Backend wait failed");
                        metrics.record_job_failed(&backend_id, "backend_wait_error");
                        if let Err(e) = job_store
                            .update_status(&job_id, JobStatus::Failed(error_msg))
                            .await
                        {
                            error!("Failed to update job status to failed: {}", e);
                        }
                        if let Some(ref resources) = resources {
                            resources.job_completed().await;
                        }
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Backend submit failed: {e}");
                warn!(error = %e, "Backend submit failed");
                metrics.record_job_failed(&backend_id, "backend_submit_error");
                if let Err(e) = job_store
                    .update_status(&job_id, JobStatus::Failed(error_msg))
                    .await
                {
                    error!("Failed to update job status to failed: {}", e);
                }
                if let Some(ref resources) = resources {
                    resources.job_completed().await;
                }
            }
        }
    });
}
