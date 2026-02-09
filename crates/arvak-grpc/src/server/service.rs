//! gRPC service implementation.

use arvak_hal::backend::Backend;
use arvak_hal::job::{JobId, JobStatus};
use arvak_ir::circuit::Circuit;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{error, info, instrument, warn};

use crate::config::ResourceLimits;
use crate::error::{Error, Result};
use crate::metrics::Metrics;
use crate::proto::{
    BackendInfo, BatchJobResult, BatchJobSubmission, CancelJobRequest, CancelJobResponse,
    CircuitPayload, GetBackendInfoRequest, GetBackendInfoResponse, GetJobResultRequest,
    GetJobResultResponse, GetJobStatusRequest, GetJobStatusResponse, Job, JobResult, JobState,
    JobStatusUpdate, ListBackendsRequest, ListBackendsResponse, ResultChunk, StreamResultsRequest,
    SubmitBatchRequest, SubmitBatchResponse, SubmitJobRequest, SubmitJobResponse, WatchJobRequest,
    arvak_service_server, batch_job_result, circuit_payload,
};
use crate::resource_manager::ResourceManager;
use crate::server::{BackendRegistry, JobStore};

/// Arvak gRPC service implementation.
pub struct ArvakServiceImpl {
    job_store: Arc<JobStore>,
    backends: Arc<BackendRegistry>,
    metrics: Metrics,
    resources: Option<ResourceManager>,
}

impl ArvakServiceImpl {
    /// Create a new service with custom components.
    pub fn with_components(job_store: JobStore, backends: BackendRegistry) -> Self {
        let metrics = Metrics::new();

        // Initialize backend availability metrics
        for backend_id in backends.list() {
            metrics.set_backend_available(&backend_id, true);
        }

        Self {
            job_store: Arc::new(job_store),
            backends: Arc::new(backends),
            metrics,
            resources: None,
        }
    }

    /// Create a new service with custom components and resource limits.
    pub fn with_limits(
        job_store: JobStore,
        backends: BackendRegistry,
        limits: ResourceLimits,
    ) -> Self {
        let mut service = Self::with_components(job_store, backends);
        service.resources = Some(ResourceManager::new(limits));
        service
    }

    /// Create a new service with default components.
    pub fn new() -> Self {
        use crate::server::backend_registry::create_default_registry;
        Self::with_components(JobStore::new(), create_default_registry())
    }

    /// Get a reference to the backend registry.
    pub fn backends(&self) -> Arc<BackendRegistry> {
        self.backends.clone()
    }

    /// Parse circuit from protobuf payload (static version for use in async contexts).
    fn parse_circuit_static(payload: Option<CircuitPayload>) -> Result<Circuit> {
        let payload =
            payload.ok_or_else(|| Error::InvalidCircuit("Missing circuit payload".to_string()))?;

        match payload.format {
            Some(circuit_payload::Format::Qasm3(qasm)) => {
                let circuit = arvak_qasm3::parse(&qasm)?;
                Ok(circuit)
            }
            Some(circuit_payload::Format::ArvakIrJson(_json)) => Err(Error::InvalidCircuit(
                "Arvak IR JSON format not yet supported".to_string(),
            )),
            None => Err(Error::InvalidCircuit(
                "No circuit format specified".to_string(),
            )),
        }
    }

    /// Parse circuit from protobuf payload.
    fn parse_circuit(&self, payload: Option<CircuitPayload>) -> Result<Circuit> {
        Self::parse_circuit_static(payload)
    }

    /// Execute a job synchronously (wait for completion).
    async fn execute_job_sync(
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
    #[instrument(skip(job_store, backend, metrics, resources), fields(job_id = %job_id.0))]
    fn spawn_job_execution(
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
                                .num_milliseconds()
                                as u64;

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
}

impl Default for ArvakServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[tonic::async_trait]
impl arvak_service_server::ArvakService for ArvakServiceImpl {
    type WatchJobStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = std::result::Result<JobStatusUpdate, Status>> + Send>,
    >;

    type StreamResultsStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = std::result::Result<ResultChunk, Status>> + Send>,
    >;

    type SubmitBatchStreamStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = std::result::Result<BatchJobResult, Status>> + Send>,
    >;

    #[instrument(skip(self, request), fields(backend_id, job_id))]
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> std::result::Result<Response<SubmitJobResponse>, Status> {
        let start = std::time::Instant::now();

        // Extract client IP from request metadata (if available)
        let client_ip = request.remote_addr().map(|addr| addr.ip().to_string());

        let req = request.into_inner();

        tracing::Span::current().record("backend_id", req.backend_id.as_str());

        // Check resource limits if manager is configured
        if let Some(ref resources) = self.resources {
            resources
                .check_can_submit(client_ip.as_deref())
                .await
                .map_err(|e| Status::resource_exhausted(e.to_string()))?;
        }

        // Parse circuit
        let circuit = self.parse_circuit(req.circuit).map_err(Status::from)?;

        // Validate backend exists
        let backend = self.backends.get(&req.backend_id).map_err(Status::from)?;

        // Create job in store (status = QUEUED)
        let job_id = self
            .job_store
            .create_job(circuit, req.backend_id.clone(), req.shots)
            .await
            .map_err(Status::from)?;

        tracing::Span::current().record("job_id", job_id.0.as_str());
        info!(shots = req.shots, "Job submitted");

        // Record job submission metric
        self.metrics.record_job_submitted(&req.backend_id);

        // Update resource tracking
        if let Some(ref resources) = self.resources {
            resources.job_submitted(client_ip.as_deref()).await;
        }

        // Spawn async execution task (non-blocking)
        Self::spawn_job_execution(
            self.job_store.clone(),
            backend,
            job_id.clone(),
            self.metrics.clone(),
            self.resources.clone(),
        );

        // Record RPC duration
        let duration = start.elapsed().as_millis() as u64;
        self.metrics.record_rpc_duration("SubmitJob", duration);

        // Return immediately
        Ok(Response::new(SubmitJobResponse { job_id: job_id.0 }))
    }

    async fn submit_batch(
        &self,
        request: Request<SubmitBatchRequest>,
    ) -> std::result::Result<Response<SubmitBatchResponse>, Status> {
        let start = std::time::Instant::now();
        let req = request.into_inner();

        // Validate backend exists
        let backend = self.backends.get(&req.backend_id).map_err(Status::from)?;

        let mut job_ids = Vec::new();

        // Submit each job
        for batch_job in req.jobs {
            let circuit = self
                .parse_circuit(batch_job.circuit)
                .map_err(Status::from)?;

            let job_id = self
                .job_store
                .create_job(circuit, req.backend_id.clone(), batch_job.shots)
                .await
                .map_err(Status::from)?;

            // Record job submission metric
            self.metrics.record_job_submitted(&req.backend_id);

            Self::spawn_job_execution(
                self.job_store.clone(),
                backend.clone(),
                job_id.clone(),
                self.metrics.clone(),
                self.resources.clone(),
            );

            job_ids.push(job_id.0);
        }

        // Record RPC duration
        let duration = start.elapsed().as_millis() as u64;
        self.metrics.record_rpc_duration("SubmitBatch", duration);

        Ok(Response::new(SubmitBatchResponse { job_ids }))
    }

    #[instrument(skip(self, request), fields(job_id))]
    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> std::result::Result<Response<GetJobStatusResponse>, Status> {
        let start = std::time::Instant::now();
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id);

        tracing::Span::current().record("job_id", job_id.0.as_str());

        let job = self
            .job_store
            .get_job(&job_id)
            .await
            .map_err(Status::from)?;

        let error_message = match &job.status {
            JobStatus::Failed(msg) => msg.clone(),
            _ => String::new(),
        };

        let proto_job = Job {
            job_id: job.id.0,
            state: Self::to_proto_state(&job.status) as i32,
            submitted_at: job.submitted_at.timestamp(),
            started_at: job.started_at.map_or(0, |t| t.timestamp()),
            completed_at: job.completed_at.map_or(0, |t| t.timestamp()),
            backend_id: job.backend_id,
            shots: job.shots,
            error_message,
        };

        // Record RPC duration
        let duration = start.elapsed().as_millis() as u64;
        self.metrics.record_rpc_duration("GetJobStatus", duration);

        Ok(Response::new(GetJobStatusResponse {
            job: Some(proto_job),
        }))
    }

    #[instrument(skip(self, request), fields(job_id))]
    async fn watch_job(
        &self,
        request: Request<WatchJobRequest>,
    ) -> std::result::Result<Response<Self::WatchJobStream>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id.clone());

        tracing::Span::current().record("job_id", job_id.0.as_str());
        info!("Starting job watch stream");

        let job_store = self.job_store.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        // Spawn watcher task
        tokio::spawn(async move {
            loop {
                match job_store.get_job(&job_id).await {
                    Ok(job) => {
                        let error_message = match &job.status {
                            JobStatus::Failed(msg) => msg.clone(),
                            _ => String::new(),
                        };

                        let update = JobStatusUpdate {
                            job_id: job.id.0.clone(),
                            state: Self::to_proto_state(&job.status) as i32,
                            timestamp: chrono::Utc::now().timestamp(),
                            error_message,
                        };

                        // Send update
                        if tx.send(Ok(update)).await.is_err() {
                            // Client disconnected
                            break;
                        }

                        // Check if job is in terminal state
                        match job.status {
                            JobStatus::Completed | JobStatus::Failed(_) | JobStatus::Cancelled => {
                                // Job finished, close stream
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("Failed to get job: {e}"))))
                            .await;
                        break;
                    }
                }

                // Poll every 500ms
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::WatchJobStream))
    }

    #[instrument(skip(self, request), fields(job_id))]
    async fn stream_results(
        &self,
        request: Request<StreamResultsRequest>,
    ) -> std::result::Result<Response<Self::StreamResultsStream>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id.clone());
        let chunk_size = if req.chunk_size > 0 {
            req.chunk_size as usize
        } else {
            1000 // Default chunk size
        };

        tracing::Span::current().record("job_id", job_id.0.as_str());
        info!(chunk_size = chunk_size, "Starting result stream");

        // Get the complete result first
        let result = self
            .job_store
            .get_result(&job_id)
            .await
            .map_err(Status::from)?;

        let (tx, rx) = tokio::sync::mpsc::channel(16);

        // Spawn task to stream result chunks
        tokio::spawn(async move {
            let all_counts: Vec<(String, u64)> =
                result.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();

            let total_entries = all_counts.len();
            let total_chunks = total_entries.div_ceil(chunk_size);

            for (chunk_index, chunk_entries) in all_counts.chunks(chunk_size).enumerate() {
                let mut chunk_counts = std::collections::HashMap::new();
                for (bitstring, count) in chunk_entries {
                    chunk_counts.insert(bitstring.clone(), *count);
                }

                let is_final = chunk_index == total_chunks - 1;
                let chunk = ResultChunk {
                    job_id: req.job_id.clone(),
                    counts: chunk_counts,
                    is_final,
                    chunk_index: chunk_index as u32,
                    total_chunks: total_chunks as u32,
                };

                if tx.send(Ok(chunk)).await.is_err() {
                    // Client disconnected
                    break;
                }
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::StreamResultsStream))
    }

    #[instrument(skip(self, request))]
    async fn submit_batch_stream(
        &self,
        request: Request<tonic::Streaming<BatchJobSubmission>>,
    ) -> std::result::Result<Response<Self::SubmitBatchStreamStream>, Status> {
        info!("Starting batch stream submission");

        let mut in_stream = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        let job_store = self.job_store.clone();
        let backends = self.backends.clone();
        let metrics = self.metrics.clone();
        let resources = self.resources.clone();

        // Spawn task to handle incoming submissions
        tokio::spawn(async move {
            while let Some(result) = in_stream.message().await.transpose() {
                match result {
                    Ok(submission) => {
                        let client_request_id = submission.client_request_id.clone();

                        // Parse circuit
                        let circuit = match Self::parse_circuit_static(submission.circuit) {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = tx
                                    .send(Ok(BatchJobResult {
                                        job_id: String::new(),
                                        client_request_id,
                                        result: Some(batch_job_result::Result::Error(format!(
                                            "Circuit parsing failed: {e}"
                                        ))),
                                    }))
                                    .await;
                                continue;
                            }
                        };

                        // Get backend
                        let backend = match backends.get(&submission.backend_id) {
                            Ok(b) => b,
                            Err(e) => {
                                let _ = tx
                                    .send(Ok(BatchJobResult {
                                        job_id: String::new(),
                                        client_request_id,
                                        result: Some(batch_job_result::Result::Error(format!(
                                            "Backend not found: {e}"
                                        ))),
                                    }))
                                    .await;
                                continue;
                            }
                        };

                        // Create job
                        match job_store
                            .create_job(circuit, submission.backend_id.clone(), submission.shots)
                            .await
                        {
                            Ok(job_id) => {
                                // Send submission confirmation
                                let _ = tx
                                    .send(Ok(BatchJobResult {
                                        job_id: job_id.0.clone(),
                                        client_request_id: client_request_id.clone(),
                                        result: Some(batch_job_result::Result::Submitted(
                                            "Job submitted successfully".to_string(),
                                        )),
                                    }))
                                    .await;

                                metrics.record_job_submitted(&submission.backend_id);

                                // Spawn execution and wait for result
                                let job_store_clone = job_store.clone();
                                let tx_clone = tx.clone();
                                let backend_clone = backend.clone();
                                let metrics_clone = metrics.clone();
                                let resources_clone = resources.clone();
                                let _backend_id = submission.backend_id.clone();

                                tokio::spawn(async move {
                                    Self::execute_job_sync(
                                        job_store_clone.clone(),
                                        backend_clone,
                                        job_id.clone(),
                                        metrics_clone,
                                        resources_clone,
                                    )
                                    .await;

                                    // Send completion notification
                                    if let Ok(result) = job_store_clone.get_result(&job_id).await {
                                        let mut counts = std::collections::HashMap::new();
                                        for (k, v) in result.counts.iter() {
                                            counts.insert(k.clone(), *v);
                                        }

                                        let metadata_json = serde_json::to_string(&result.metadata)
                                            .unwrap_or_else(|_| "{}".to_string());

                                        let _ = tx_clone
                                            .send(Ok(BatchJobResult {
                                                job_id: job_id.0.clone(),
                                                client_request_id,
                                                result: Some(batch_job_result::Result::Completed(
                                                    JobResult {
                                                        job_id: job_id.0,
                                                        counts,
                                                        shots: result.shots,
                                                        execution_time_ms: result
                                                            .execution_time_ms
                                                            .unwrap_or(0),
                                                        metadata_json,
                                                    },
                                                )),
                                            }))
                                            .await;
                                    }
                                });
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Ok(BatchJobResult {
                                        job_id: String::new(),
                                        client_request_id,
                                        result: Some(batch_job_result::Result::Error(format!(
                                            "Job creation failed: {e}"
                                        ))),
                                    }))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("Stream error: {e}"))))
                            .await;
                        break;
                    }
                }
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(stream) as Self::SubmitBatchStreamStream
        ))
    }

    async fn get_job_result(
        &self,
        request: Request<GetJobResultRequest>,
    ) -> std::result::Result<Response<GetJobResultResponse>, Status> {
        let req = request.into_inner();
        let job_id = JobId::new(req.job_id.clone());

        let result = self
            .job_store
            .get_result(&job_id)
            .await
            .map_err(Status::from)?;

        // Convert counts to protobuf map
        let mut counts = std::collections::HashMap::new();
        for (bitstring, count) in result.counts.iter() {
            counts.insert(bitstring.clone(), *count);
        }

        let metadata_json =
            serde_json::to_string(&result.metadata).unwrap_or_else(|_| "{}".to_string());

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
        let job = self
            .job_store
            .get_job(&job_id)
            .await
            .map_err(Status::from)?;

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
            .map_err(Status::from)?;

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
            let backend = self.backends.get(&id).map_err(Status::from)?;

            let caps = backend
                .capabilities()
                .await
                .map_err(|e| Status::internal(format!("Failed to get capabilities: {e}")))?;

            let is_available = backend.is_available().await.unwrap_or(false);

            let topology_json =
                serde_json::to_string(&caps.topology).unwrap_or_else(|_| "{}".to_string());

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

        let backend = self.backends.get(&req.backend_id).map_err(Status::from)?;

        let caps = backend
            .capabilities()
            .await
            .map_err(|e| Status::internal(format!("Failed to get capabilities: {e}")))?;

        let is_available = backend.is_available().await.unwrap_or(false);

        let topology_json =
            serde_json::to_string(&caps.topology).unwrap_or_else(|_| "{}".to_string());

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
