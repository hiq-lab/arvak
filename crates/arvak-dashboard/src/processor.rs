//! Background job processor that picks up pending jobs and executes them on backends.

use std::sync::Arc;
use std::time::Duration;

use arvak_sched::{JobFilter, ScheduledJobStatus};
use tokio::time;
use tracing::{error, info, warn};

use crate::state::AppState;

/// Run the background job processor loop.
///
/// Polls the store for pending jobs every 5 seconds, executes them on a
/// matched backend, and saves the results.
pub async fn run_job_processor(state: Arc<AppState>) {
    let mut interval = time::interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let store = match &state.store {
            Some(s) => Arc::clone(s),
            None => continue,
        };

        let pending_jobs = match store.list_jobs(&JobFilter::pending()).await {
            Ok(jobs) => jobs,
            Err(e) => {
                warn!("Failed to list pending jobs: {}", e);
                continue;
            }
        };

        for job in pending_jobs {
            let job_id = job.id.clone();

            // Find backend: prefer matched_backend, fall back to first available
            let backends = state.backends.read().await;
            let backend = if let Some(ref name) = job.matched_backend {
                backends.get(name).cloned()
            } else {
                backends.values().next().cloned()
            };
            drop(backends);

            let backend = if let Some(b) = backend {
                b
            } else {
                warn!("No backend available for job {}", job_id);
                continue;
            };

            // Resolve the first circuit
            let circuit = if let Some(spec) = job.circuits.first() {
                match spec.resolve() {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to resolve circuit for job {}: {}", job_id, e);
                        let _ = store
                            .update_status(
                                &job_id,
                                ScheduledJobStatus::Failed {
                                    reason: format!("Circuit resolve error: {e}"),
                                    slurm_job_id: None,
                                    quantum_job_id: None,
                                },
                            )
                            .await;
                        continue;
                    }
                }
            } else {
                error!("Job {} has no circuits", job_id);
                let _ = store
                    .update_status(
                        &job_id,
                        ScheduledJobStatus::Failed {
                            reason: "Job has no circuits".to_string(),
                            slurm_job_id: None,
                            quantum_job_id: None,
                        },
                    )
                    .await;
                continue;
            };

            // Submit to backend
            let quantum_job_id = match backend.submit(&circuit, job.shots).await {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to submit job {} to backend: {}", job_id, e);
                    let _ = store
                        .update_status(
                            &job_id,
                            ScheduledJobStatus::Failed {
                                reason: format!("Backend submit error: {e}"),
                                slurm_job_id: None,
                                quantum_job_id: None,
                            },
                        )
                        .await;
                    continue;
                }
            };

            // Update status to QuantumRunning
            let slurm_job_id = "local".to_string();
            let _ = store
                .update_status(
                    &job_id,
                    ScheduledJobStatus::QuantumRunning {
                        slurm_job_id: slurm_job_id.clone(),
                        quantum_job_id: quantum_job_id.clone(),
                    },
                )
                .await;

            // Retrieve result
            match backend.result(&quantum_job_id).await {
                Ok(result) => {
                    if let Err(e) = store.save_result(&job_id, &result).await {
                        error!("Failed to save result for job {}: {}", job_id, e);
                    }

                    let _ = store
                        .update_status(
                            &job_id,
                            ScheduledJobStatus::Completed {
                                slurm_job_id: slurm_job_id.clone(),
                                quantum_job_id: quantum_job_id.clone(),
                            },
                        )
                        .await;

                    info!("Job {} completed successfully", job_id);
                }
                Err(e) => {
                    error!("Failed to get result for job {}: {}", job_id, e);
                    let _ = store
                        .update_status(
                            &job_id,
                            ScheduledJobStatus::Failed {
                                reason: format!("Backend result error: {e}"),
                                slurm_job_id: Some(slurm_job_id),
                                quantum_job_id: Some(quantum_job_id),
                            },
                        )
                        .await;
                }
            }
        }
    }
}
