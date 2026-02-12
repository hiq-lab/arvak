//! gRPC service implementation.

mod backend_service;
mod circuit_utils;
mod job_execution;
mod job_service;

use arvak_ir::circuit::Circuit;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::config::ResourceLimits;
use crate::error::Result;
use crate::metrics::Metrics;
use crate::proto::{
    BatchJobResult, BatchJobSubmission, CancelJobRequest, CancelJobResponse, CircuitPayload,
    GetBackendInfoRequest, GetBackendInfoResponse, GetJobResultRequest, GetJobResultResponse,
    GetJobStatusRequest, GetJobStatusResponse, JobStatusUpdate, ListBackendsRequest,
    ListBackendsResponse, ResultChunk, StreamResultsRequest, SubmitBatchRequest,
    SubmitBatchResponse, SubmitJobRequest, SubmitJobResponse, WatchJobRequest,
    arvak_service_server,
};
use crate::resource_manager::ResourceManager;
use crate::server::{BackendRegistry, JobStore};

use circuit_utils::parse_circuit_static;

/// Arvak gRPC service implementation.
pub struct ArvakServiceImpl {
    pub(crate) job_store: Arc<JobStore>,
    pub(crate) backends: Arc<BackendRegistry>,
    pub(crate) metrics: Metrics,
    pub(crate) resources: Option<ResourceManager>,
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

    /// Parse circuit from protobuf payload.
    fn parse_circuit(&self, payload: Option<CircuitPayload>) -> Result<Circuit> {
        parse_circuit_static(payload)
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

    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> std::result::Result<Response<SubmitJobResponse>, Status> {
        self.submit_job_impl(request).await
    }

    async fn submit_batch(
        &self,
        request: Request<SubmitBatchRequest>,
    ) -> std::result::Result<Response<SubmitBatchResponse>, Status> {
        self.submit_batch_impl(request).await
    }

    async fn get_job_status(
        &self,
        request: Request<GetJobStatusRequest>,
    ) -> std::result::Result<Response<GetJobStatusResponse>, Status> {
        self.get_job_status_impl(request).await
    }

    async fn watch_job(
        &self,
        request: Request<WatchJobRequest>,
    ) -> std::result::Result<Response<Self::WatchJobStream>, Status> {
        self.watch_job_impl(request).await
    }

    async fn stream_results(
        &self,
        request: Request<StreamResultsRequest>,
    ) -> std::result::Result<Response<Self::StreamResultsStream>, Status> {
        self.stream_results_impl(request).await
    }

    async fn submit_batch_stream(
        &self,
        request: Request<tonic::Streaming<BatchJobSubmission>>,
    ) -> std::result::Result<Response<Self::SubmitBatchStreamStream>, Status> {
        self.submit_batch_stream_impl(request).await
    }

    async fn get_job_result(
        &self,
        request: Request<GetJobResultRequest>,
    ) -> std::result::Result<Response<GetJobResultResponse>, Status> {
        self.get_job_result_impl(request).await
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> std::result::Result<Response<CancelJobResponse>, Status> {
        self.cancel_job_impl(request).await
    }

    async fn list_backends(
        &self,
        request: Request<ListBackendsRequest>,
    ) -> std::result::Result<Response<ListBackendsResponse>, Status> {
        self.list_backends_impl(request).await
    }

    async fn get_backend_info(
        &self,
        request: Request<GetBackendInfoRequest>,
    ) -> std::result::Result<Response<GetBackendInfoResponse>, Status> {
        self.get_backend_info_impl(request).await
    }
}
