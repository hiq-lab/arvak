//! gRPC service implementation.

use std::sync::Arc;
use arvak_hal::backend::Backend;
use arvak_hal::job::{JobId, JobStatus};
use arvak_ir::circuit::Circuit;
use tonic::{Request, Response, Status};

use crate::error::{Error, Result};
use crate::proto::*;
use crate::server::{BackendRegistry, JobStore};

/// Arvak gRPC service implementation.
pub struct ArvakServiceImpl {
    job_store: Arc<JobStore>,
    backends: Arc<BackendRegistry>,
}

impl ArvakServiceImpl {
    /// Create a new service with custom components.
    pub fn with_components(job_store: JobStore, backends: BackendRegistry) -> Self {
        Self {
            job_store: Arc::new(job_store),
            backends: Arc::new(backends),
        }
    }

    /// Create a new service with default components.
    pub fn new() -> Self {
        use crate::server::backend_registry::create_default_registry;
        Self::with_components(JobStore::new(), create_default_registry())
    }

    /// Parse circuit from protobuf payload.
    fn parse_circuit(&self, payload: Option<CircuitPayload>) -> Result<Circuit> {
        let payload = payload.ok_or_else(|| Error::InvalidCircuit("Missing circuit payload".to_string()))?;

        match payload.format {
            Some(circuit_payload::Format::Qasm3(qasm)) => {
                let circuit = arvak_qasm3::parse(&qasm)?;
                Ok(circuit)
            }
            Some(circuit_payload::Format::ArvakIrJson(_json)) => {
                // TODO: Implement Circuit JSON deserialization
                Err(Error::InvalidCircuit("Arvak IR JSON format not yet supported".to_string()))
            }
            None => Err(Error::InvalidCircuit("No circuit format specified".to_string())),
        }
    }

    /// Convert HAL JobStatus to protobuf JobState.
    fn to_proto_state(status: &JobStatus) -> JobState {
        match status {
            JobStatus::Queued => JobState::Queued,
            JobStatus::Running => JobState::Running,
            JobStatus::Completed => JobState::Completed,
            JobStatus::Failed(_) => JobState::Failed,
            JobStatus::Cancelled => JobState::Canceled,
        }
    }

    /// Spawn async task to execute a job.
    fn spawn_job_execution(
        job_store: Arc<JobStore>,
        backend: Arc<dyn Backend>,
        job_id: JobId,
    ) {
        tokio::spawn(async move {
            // Update to RUNNING
            if let Err(e) = job_store.update_status(&job_id, JobStatus::Running).await {
                tracing::error!("Failed to update job status to running: {}", e);
                return;
            }

            // Get job details
            let job = match job_store.get_job(&job_id).await {
                Ok(job) => job,
                Err(e) => {
                    tracing::error!("Failed to get job: {}", e);
                    return;
                }
            };

            // Execute on backend
            match backend.submit(&job.circuit, job.shots).await {
                Ok(backend_job_id) => {
                    // Wait for backend to complete
                    match backend.wait(&backend_job_id).await {
                        Ok(result) => {
                            if let Err(e) = job_store.store_result(&job_id, result).await {
                                tracing::error!("Failed to store job result: {}", e);
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Backend wait failed: {}", e);
                            if let Err(e) = job_store
                                .update_status(&job_id, JobStatus::Failed(error_msg))
                                .await
                            {
                                tracing::error!("Failed to update job status to failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Backend submit failed: {}", e);
                    if let Err(e) = job_store
                        .update_status(&job_id, JobStatus::Failed(error_msg))
                        .await
                    {
                        tracing::error!("Failed to update job status to failed: {}", e);
                    }
                }
            }
        });
    }
}

impl Default for ArvakServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl arvak_service_server::ArvakService for ArvakServiceImpl {
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> std::result::Result<Response<SubmitJobResponse>, Status> {
        let req = request.into_inner();

        // Parse circuit
        let circuit = self.parse_circuit(req.circuit)
            .map_err(|e| Status::from(e))?;

        // Validate backend exists
        let backend = self.backends.get(&req.backend_id)
            .map_err(|e| Status::from(e))?;

        // Create job in store (status = QUEUED)
        let job_id = self.job_store
            .create_job(circuit, req.backend_id, req.shots)
            .await;

        // Spawn async execution task (non-blocking)
        Self::spawn_job_execution(
            self.job_store.clone(),
            backend,
            job_id.clone(),
        );

        // Return immediately
        Ok(Response::new(SubmitJobResponse {
            job_id: job_id.0,
        }))
    }

    async fn submit_batch(
        &self,
        request: Request<SubmitBatchRequest>,
    ) -> std::result::Result<Response<SubmitBatchResponse>, Status> {
        let req = request.into_inner();

        // Validate backend exists
        let backend = self.backends.get(&req.backend_id)
            .map_err(|e| Status::from(e))?;

        let mut job_ids = Vec::new();

        // Submit each job
        for batch_job in req.jobs {
            let circuit = self.parse_circuit(batch_job.circuit)
                .map_err(|e| Status::from(e))?;

            let job_id = self.job_store
                .create_job(circuit, req.backend_id.clone(), batch_job.shots)
                .await;

            Self::spawn_job_execution(
                self.job_store.clone(),
                backend.clone(),
                job_id.clone(),
            );

            job_ids.push(job_id.0);
        }

        Ok(Response::new(SubmitBatchResponse { job_ids }))
    }

    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> std::result::Result<Response<GetJobStatusResponse>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id);

        let job = self.job_store.get_job(&job_id).await
            .map_err(|e| Status::from(e))?;

        let error_message = match &job.status {
            JobStatus::Failed(msg) => msg.clone(),
            _ => String::new(),
        };

        let proto_job = Job {
            job_id: job.id.0,
            state: Self::to_proto_state(&job.status) as i32,
            submitted_at: job.submitted_at.timestamp(),
            started_at: job.started_at.map(|t| t.timestamp()).unwrap_or(0),
            completed_at: job.completed_at.map(|t| t.timestamp()).unwrap_or(0),
            backend_id: job.backend_id,
            shots: job.shots,
            error_message,
        };

        Ok(Response::new(GetJobStatusResponse {
            job: Some(proto_job),
        }))
    }

    async fn get_job_result(
        &self,
        request: Request<GetJobResultRequest>,
    ) -> std::result::Result<Response<GetJobResultResponse>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id.clone());

        let result = self.job_store.get_result(&job_id).await
            .map_err(|e| Status::from(e))?;

        // Convert counts to protobuf map
        let mut counts = std::collections::HashMap::new();
        for (bitstring, count) in result.counts.iter() {
            counts.insert(bitstring.clone(), *count);
        }

        let metadata_json = serde_json::to_string(&result.metadata)
            .unwrap_or_else(|_| "{}".to_string());

        let proto_result = JobResult {
            job_id: req.job_id,
            counts,
            shots: result.shots,
            execution_time_ms: result.execution_time_ms.unwrap_or(0),
            metadata_json,
        };

        Ok(Response::new(GetJobResultResponse {
            result: Some(proto_result),
        }))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> std::result::Result<Response<CancelJobResponse>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id);

        // Check current status
        let job = self.job_store.get_job(&job_id).await
            .map_err(|e| Status::from(e))?;

        if job.status.is_terminal() {
            return Ok(Response::new(CancelJobResponse {
                success: false,
                message: format!("Job already in terminal state: {}", job.status),
            }));
        }

        // Update status to cancelled
        self.job_store
            .update_status(&job_id, JobStatus::Cancelled)
            .await
            .map_err(|e| Status::from(e))?;

        Ok(Response::new(CancelJobResponse {
            success: true,
            message: "Job cancelled successfully".to_string(),
        }))
    }

    async fn list_backends(
        &self,
        _request: Request<ListBackendsRequest>,
    ) -> std::result::Result<Response<ListBackendsResponse>, Status> {
        let backend_ids = self.backends.list();
        let mut backends = Vec::new();

        for id in backend_ids {
            let backend = self.backends.get(&id)
                .map_err(|e| Status::from(e))?;

            let caps = backend.capabilities().await
                .map_err(|e| Status::internal(format!("Failed to get capabilities: {}", e)))?;

            let is_available = backend.is_available().await
                .unwrap_or(false);

            let topology_json = serde_json::to_string(&caps.topology)
                .unwrap_or_else(|_| "{}".to_string());

            let mut supported_gates = caps.gate_set.single_qubit.clone();
            supported_gates.extend(caps.gate_set.two_qubit.clone());

            backends.push(BackendInfo {
                backend_id: id.clone(),
                name: caps.name.clone(),
                is_available,
                max_qubits: caps.num_qubits,
                max_shots: caps.max_shots,
                description: format!("{} ({} qubits)", backend.name(), caps.num_qubits),
                supported_gates,
                topology_json,
            });
        }

        Ok(Response::new(ListBackendsResponse { backends }))
    }

    async fn get_backend_info(
        &self,
        request: Request<GetBackendInfoRequest>,
    ) -> std::result::Result<Response<GetBackendInfoResponse>, Status> {
        let req = request.into_inner();

        let backend = self.backends.get(&req.backend_id)
            .map_err(|e| Status::from(e))?;

        let caps = backend.capabilities().await
            .map_err(|e| Status::internal(format!("Failed to get capabilities: {}", e)))?;

        let is_available = backend.is_available().await
            .unwrap_or(false);

        let topology_json = serde_json::to_string(&caps.topology)
            .unwrap_or_else(|_| "{}".to_string());

        let mut supported_gates = caps.gate_set.single_qubit.clone();
        supported_gates.extend(caps.gate_set.two_qubit.clone());

        let backend_info = BackendInfo {
            backend_id: req.backend_id.clone(),
            name: caps.name.clone(),
            is_available,
            max_qubits: caps.num_qubits,
            max_shots: caps.max_shots,
            description: format!("{} ({} qubits)", backend.name(), caps.num_qubits),
            supported_gates,
            topology_json,
        };

        Ok(Response::new(GetBackendInfoResponse {
            backend: Some(backend_info),
        }))
    }
}
