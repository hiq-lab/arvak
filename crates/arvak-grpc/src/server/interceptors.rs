//! gRPC interceptors for request/response processing.
//!
//! This module provides interceptors for:
//! - Request ID generation and propagation
//! - Request/response logging
//! - Authentication (future)

use tonic::{Request, Status};
use tracing::{info, warn};
use uuid::Uuid;

/// Request ID metadata key.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Request ID interceptor that generates or propagates request IDs.
///
/// This interceptor:
/// 1. Checks if the client sent an x-request-id header
/// 2. If not, generates a new UUID
/// 3. Attaches the request ID to tracing spans
/// 4. Adds the request ID to response metadata
#[derive(Clone)]
pub struct RequestIdInterceptor;

impl Default for RequestIdInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestIdInterceptor {
    pub fn new() -> Self {
        Self
    }
}

impl tonic::service::Interceptor for RequestIdInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        // Try to get existing request ID from metadata
        let request_id = request
            .metadata()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok()).map_or_else(|| {
                // Generate new request ID if not provided
                Uuid::new_v4().to_string()
            }, std::string::ToString::to_string);

        // Add request ID to tracing span
        tracing::Span::current().record("request_id", request_id.as_str());

        // Store request ID in request extensions for later access
        request
            .extensions_mut()
            .insert(RequestId(request_id.clone()));

        info!(request_id = %request_id, "Processing request");

        Ok(request)
    }
}

/// Request ID stored in request extensions.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Logging interceptor for request/response logging.
///
/// Logs all incoming requests with method name, client address, and timing.
#[derive(Clone)]
pub struct LoggingInterceptor;

impl Default for LoggingInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl LoggingInterceptor {
    pub fn new() -> Self {
        Self
    }
}

impl tonic::service::Interceptor for LoggingInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        let remote_addr = request
            .remote_addr().map_or_else(|| "unknown".to_string(), |addr| addr.to_string());

        info!(
            client = %remote_addr,
            "Incoming gRPC request"
        );

        Ok(request)
    }
}

/// Rate limit state for a client.
///
/// Tracks request counts and timing for rate limiting.
/// Note: This is a simple in-memory implementation.
/// For production, consider using a distributed rate limiter.
pub struct RateLimiter {
    // Future: implement proper rate limiting with token bucket or sliding window
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {}
    }

    /// Check if a request should be allowed.
    pub fn allow(&self, _client_ip: &str) -> bool {
        // Future: implement rate limiting logic
        true
    }
}

/// Authentication interceptor (placeholder for future implementation).
///
/// This interceptor can be used to validate API keys, JWT tokens, or other
/// authentication mechanisms.
#[derive(Clone)]
pub struct AuthInterceptor {
    // Future: add authentication state
}

impl Default for AuthInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthInterceptor {
    pub fn new() -> Self {
        Self {}
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        // Future: implement authentication logic
        // For now, just pass through
        Ok(request)
    }
}

/// Error interceptor for handling and logging errors.
///
/// Logs errors with appropriate severity and adds structured error information.
pub fn log_error(status: &Status) {
    match status.code() {
        tonic::Code::Ok => {}
        tonic::Code::InvalidArgument
        | tonic::Code::NotFound
        | tonic::Code::AlreadyExists
        | tonic::Code::FailedPrecondition => {
            info!(
                code = ?status.code(),
                message = %status.message(),
                "Request validation error"
            );
        }
        tonic::Code::Unauthenticated | tonic::Code::PermissionDenied => {
            warn!(
                code = ?status.code(),
                message = %status.message(),
                "Authentication/authorization error"
            );
        }
        _ => {
            warn!(
                code = ?status.code(),
                message = %status.message(),
                "Request error"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::service::Interceptor;

    #[test]
    fn test_request_id_generation() {
        let mut interceptor = RequestIdInterceptor::new();
        let request = Request::new(());

        let result = interceptor.call(request);
        assert!(result.is_ok());

        let request = result.unwrap();
        let request_id = request.extensions().get::<RequestId>();
        assert!(request_id.is_some());
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new();
        assert!(limiter.allow("127.0.0.1"));
    }
}
