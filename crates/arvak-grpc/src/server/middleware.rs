//! Tower middleware for gRPC service.
//!
//! This module provides tower layers for:
//! - Request timing and latency tracking
//! - Metrics collection
//! - Connection management

use std::task::{Context, Poll};
use std::time::Instant;
use tonic::body::BoxBody;
use tower::{Layer, Service};
use tracing::{info, instrument};

/// Timing middleware layer that tracks request duration.
///
/// This layer measures the time taken to process each request and logs it.
#[derive(Clone)]
pub struct TimingLayer;

impl Default for TimingLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl TimingLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for TimingLayer {
    type Service = TimingMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        TimingMiddleware { inner: service }
    }
}

/// Timing middleware service.
#[derive(Clone)]
pub struct TimingMiddleware<S> {
    inner: S,
}

impl<S> Service<hyper::Request<BoxBody>> for TimingMiddleware<S>
where
    S: Service<hyper::Request<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: hyper::Request<BoxBody>) -> Self::Future {
        let start = Instant::now();
        let method = req.uri().path().to_string();

        // Clone the service for the async block
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let response = inner.call(req).await;
            let duration = start.elapsed();

            info!(
                method = %method,
                duration_ms = duration.as_millis() as u64,
                "Request completed"
            );

            response
        })
    }
}

/// Connection metadata layer.
///
/// Tracks connection information like client IP, connection time, etc.
#[derive(Clone)]
pub struct ConnectionInfoLayer;

impl Default for ConnectionInfoLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionInfoLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ConnectionInfoLayer {
    type Service = ConnectionInfoMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        ConnectionInfoMiddleware { inner: service }
    }
}

/// Connection info middleware service.
#[derive(Clone)]
pub struct ConnectionInfoMiddleware<S> {
    inner: S,
}

impl<S> Service<hyper::Request<BoxBody>> for ConnectionInfoMiddleware<S>
where
    S: Service<hyper::Request<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[instrument(skip(self, req), fields(uri = %req.uri()))]
    fn call(&mut self, req: hyper::Request<BoxBody>) -> Self::Future {
        // Future: extract connection info and add to tracing context
        self.inner.call(req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_layer_creation() {
        let _layer = TimingLayer::new();
    }

    #[test]
    fn test_connection_info_layer_creation() {
        let _layer = ConnectionInfoLayer::new();
    }
}
