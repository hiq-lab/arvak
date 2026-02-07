//! Job management endpoints.

use std::sync::Arc;

use arvak_sched::{
    CircuitSpec, JobFilter, Priority, ScheduledJob, ScheduledJobId, ScheduledJobStatus,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::dto::{
    CreateJobRequest, HistogramBar, JobDetails, JobListParams, JobSummary, ResultHistogram,
    ResultStatistics,
};
use crate::error::ApiError;
use crate::state::AppState;

/// GET /api/jobs - List all jobs.
pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<JobListParams>,
) -> Result<Json<Vec<JobSummary>>, ApiError> {
    let store = match state.store.as_ref() {
        Some(s) => s,
        None => return Ok(Json(vec![])),
    };

    // Build filter from query params
    let mut filter = JobFilter::default();

    if params.pending {
        filter.pending_only = true;
    }
    if params.running {
        filter.running_only = true;
    }
    if let Some(limit) = params.limit {
        filter.limit = Some(limit);
    }
    if let Some(ref status) = params.status {
        filter.status = Some(vec![status.clone()]);
    }

    let jobs = store
        .list_jobs(&filter)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let summaries: Vec<JobSummary> = jobs.into_iter().map(job_to_summary).collect();

    Ok(Json(summaries))
}

/// GET /api/jobs/:id - Get job details.
pub async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<JobDetails>, ApiError> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| ApiError::Internal("No job store configured".to_string()))?;

    let job_id = ScheduledJobId::parse(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid job ID: {}", id)))?;

    let job = store
        .load_job(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Job not found: {}", id)))?;

    Ok(Json(job_to_details(&job)))
}

/// POST /api/jobs - Create a new job.
pub async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateJobRequest>,
) -> Result<Json<JobSummary>, ApiError> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| ApiError::Internal("No job store configured".to_string()))?;

    // Validate QASM
    let _ = arvak_qasm3::parse(&req.qasm)?;

    // Create circuit spec
    let circuit = CircuitSpec::from_qasm(&req.qasm);

    // Create the job
    let mut job = ScheduledJob::new(&req.name, circuit)
        .with_shots(req.shots)
        .with_priority(Priority::new(req.priority));

    // Set matched backend if specified
    if let Some(backend) = req.backend {
        job.matched_backend = Some(backend);
    }

    // Save the job
    store
        .save_job(&job)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(job_to_summary(job)))
}

/// DELETE /api/jobs/:id - Cancel/delete a job.
pub async fn delete_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| ApiError::Internal("No job store configured".to_string()))?;

    let job_id = ScheduledJobId::parse(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid job ID: {}", id)))?;

    // Load the job to check if it exists
    let job = store
        .load_job(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Job not found: {}", id)))?;

    // If not terminal, cancel it first
    if !job.status.is_terminal() {
        store
            .update_status(&job_id, ScheduledJobStatus::Cancelled)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // Delete the job
    store
        .delete_job(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "deleted": true,
        "id": id
    })))
}

/// GET /api/jobs/:id/result - Get job execution result.
pub async fn get_job_result(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ResultHistogram>, ApiError> {
    let store = state
        .store
        .as_ref()
        .ok_or_else(|| ApiError::Internal("No job store configured".to_string()))?;

    let job_id = ScheduledJobId::parse(&id)
        .map_err(|_| ApiError::BadRequest(format!("Invalid job ID: {}", id)))?;

    // Check job exists and is completed
    let job = store
        .load_job(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("Job not found: {}", id)))?;

    if !job.status.is_terminal() {
        return Err(ApiError::BadRequest(
            "Job has not completed yet".to_string(),
        ));
    }

    // Load result
    let result = store
        .load_result(&job_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("No result found for job: {}", id)))?;

    // Convert to histogram
    let histogram = result_to_histogram(&id, &result);

    Ok(Json(histogram))
}

// ============================================================================
// Conversion helpers
// ============================================================================

fn job_to_summary(job: ScheduledJob) -> JobSummary {
    let status_details = match &job.status {
        ScheduledJobStatus::SlurmQueued { slurm_job_id }
        | ScheduledJobStatus::SlurmRunning { slurm_job_id } => {
            Some(format!("SLURM: {}", slurm_job_id))
        }
        ScheduledJobStatus::QuantumSubmitted { quantum_job_id, .. }
        | ScheduledJobStatus::QuantumRunning { quantum_job_id, .. } => {
            Some(format!("Quantum: {}", quantum_job_id.0))
        }
        ScheduledJobStatus::Failed { reason, .. } => Some(reason.clone()),
        _ => None,
    };

    JobSummary {
        id: job.id.to_string(),
        name: job.name,
        status: job.status.name().to_string(),
        status_details,
        backend: job.matched_backend,
        shots: job.shots,
        num_circuits: job.circuits.len(),
        priority: job.priority.value(),
        created_at: job.created_at.to_rfc3339(),
        submitted_at: job.submitted_at.map(|t| t.to_rfc3339()),
        completed_at: job.completed_at.map(|t| t.to_rfc3339()),
    }
}

fn job_to_details(job: &ScheduledJob) -> JobDetails {
    let status_details = match &job.status {
        ScheduledJobStatus::SlurmQueued { slurm_job_id }
        | ScheduledJobStatus::SlurmRunning { slurm_job_id } => {
            Some(format!("SLURM: {}", slurm_job_id))
        }
        ScheduledJobStatus::QuantumSubmitted { quantum_job_id, .. }
        | ScheduledJobStatus::QuantumRunning { quantum_job_id, .. } => {
            Some(format!("Quantum: {}", quantum_job_id.0))
        }
        ScheduledJobStatus::Failed { reason, .. } => Some(reason.clone()),
        _ => None,
    };

    // Get QASM from first circuit
    let qasm = job.circuits.first().and_then(|c| match c {
        CircuitSpec::Qasm3(qasm) => Some(qasm.clone()),
        CircuitSpec::QasmFile(_) => None,
    });

    JobDetails {
        id: job.id.to_string(),
        name: job.name.clone(),
        status: job.status.name().to_string(),
        status_details,
        backend: job.matched_backend.clone(),
        shots: job.shots,
        priority: job.priority.value(),
        qasm,
        num_circuits: job.circuits.len(),
        created_at: job.created_at.to_rfc3339(),
        submitted_at: job.submitted_at.map(|t| t.to_rfc3339()),
        completed_at: job.completed_at.map(|t| t.to_rfc3339()),
        metadata: job
            .metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

fn result_to_histogram(job_id: &str, result: &arvak_hal::ExecutionResult) -> ResultHistogram {
    let mut bars: Vec<HistogramBar> = result
        .counts
        .iter()
        .map(|(bitstring, &count)| {
            let probability = count as f64 / result.shots as f64;
            HistogramBar {
                bitstring: bitstring.clone(),
                count,
                probability,
            }
        })
        .collect();

    // Sort by count descending
    bars.sort_by(|a, b| b.count.cmp(&a.count));

    let total_shots: u64 = bars.iter().map(|b| b.count).sum();
    let unique_outcomes = bars.len();
    let (most_frequent, most_frequent_count) = bars
        .first()
        .map(|b| (b.bitstring.clone(), b.count))
        .unwrap_or_default();

    ResultHistogram {
        job_id: job_id.to_string(),
        shots: result.shots,
        execution_time_ms: result.execution_time_ms,
        bars,
        statistics: ResultStatistics {
            total_shots,
            unique_outcomes,
            most_frequent,
            most_frequent_count,
        },
    }
}
