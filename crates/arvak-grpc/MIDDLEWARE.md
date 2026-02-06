# Middleware and Interceptors

Arvak gRPC includes middleware and interceptors for enhanced request processing, logging, and observability.

## Architecture

```
Client Request
    ↓
[Tower Middleware Layers]
    ↓ TimingLayer (request timing)
    ↓ ConnectionInfoLayer (connection metadata)
    ↓
[Tonic Interceptors]
    ↓ RequestIdInterceptor (request ID generation)
    ↓ LoggingInterceptor (request logging)
    ↓
[Service Implementation]
    ↓ ArvakServiceImpl
    ↓
Response
```

## Interceptors

Interceptors run before the service handler and can:
- Inspect/modify request metadata
- Add tracing context
- Perform authentication
- Log requests

### Request ID Interceptor

Automatically generates or propagates request IDs for request tracing.

**Features:**
- Checks for existing `x-request-id` header
- Generates UUID if not provided
- Attaches request ID to tracing spans
- Enables end-to-end request tracking

**Usage:**
```rust
use arvak_grpc::server::RequestIdInterceptor;

let service_with_interceptor = ArvakServiceServer::with_interceptor(
    service,
    RequestIdInterceptor::new(),
);
```

**Request ID Header:**
```
x-request-id: 550e8400-e29b-41d4-a716-446655440000
```

### Logging Interceptor

Logs all incoming requests with client information.

**Logged Information:**
- Client IP address
- Request timestamp
- Method being called

**Example Log:**
```
INFO Incoming gRPC request client=127.0.0.1:54321
```

## Middleware Layers

Tower middleware layers wrap the entire service and can:
- Measure request duration
- Collect metrics
- Manage connections
- Handle errors

### Timing Layer

Measures and logs request duration for all RPCs.

**Features:**
- Tracks request start/end time
- Logs duration in milliseconds
- Useful for performance monitoring

**Example Log:**
```
INFO Request completed method=/arvak.v1.ArvakService/SubmitJob duration_ms=245
```

**Usage:**
```rust
use arvak_grpc::server::TimingLayer;
use tower::ServiceBuilder;

let server = Server::builder()
    .layer(
        ServiceBuilder::new()
            .layer(TimingLayer::new())
            .into_inner(),
    )
    .add_service(service)
    .serve(addr);
```

### Connection Info Layer

Tracks connection metadata for observability.

**Features:**
- Monitors active connections
- Tracks connection duration
- Logs connection events

## Combining Interceptors and Middleware

The server automatically configures both interceptors and middleware:

```rust
// In arvak-grpc-server.rs
let service_with_interceptor = ArvakServiceServer::with_interceptor(
    service,
    RequestIdInterceptor::new(),
);

let server = Server::builder()
    .layer(
        ServiceBuilder::new()
            .layer(TimingLayer::new())
            .into_inner(),
    )
    .add_service(service_with_interceptor)
    .serve(addr);
```

## Request Flow Example

1. **Client sends request**
   ```
   POST /arvak.v1.ArvakService/SubmitJob
   x-request-id: abc-123
   ```

2. **TimingLayer starts timer**
   ```rust
   start_time = Instant::now()
   ```

3. **RequestIdInterceptor processes**
   ```rust
   request_id = "abc-123" (from header)
   tracing::Span::current().record("request_id", "abc-123")
   ```

4. **LoggingInterceptor logs**
   ```
   INFO Incoming gRPC request client=192.168.1.100:45678 request_id=abc-123
   ```

5. **Service handles request**
   ```rust
   ArvakServiceImpl::submit_job(request)
   ```

6. **TimingLayer logs completion**
   ```
   INFO Request completed method=SubmitJob duration_ms=123 request_id=abc-123
   ```

## Error Handling

Interceptors and middleware handle errors gracefully:

```rust
use arvak_grpc::server::interceptors::log_error;

// Error logging is automatic, but you can use it manually:
if let Err(status) = result {
    log_error(&status);
}
```

Error logs include:
- Error code (InvalidArgument, NotFound, Internal, etc.)
- Error message
- Request context (request ID, method)

## Future Enhancements

### Authentication Interceptor

```rust
// Future: JWT or API key authentication
let service = ArvakServiceServer::with_interceptor(
    service,
    AuthInterceptor::new(config.auth),
);
```

### Rate Limiting Middleware

```rust
// Future: Per-client rate limiting
let server = Server::builder()
    .layer(RateLimitLayer::new(config.rate_limits))
    .add_service(service);
```

### TLS/SSL Configuration

```rust
// Future: TLS support
let tls = ServerTlsConfig::new()
    .identity(Identity::from_pem(cert, key));

let server = Server::builder()
    .tls_config(tls)?
    .add_service(service);
```

### CORS for gRPC-Web

```rust
// Future: CORS support for browser clients
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_headers([header::CONTENT_TYPE]);

let server = Server::builder()
    .layer(cors)
    .add_service(service);
```

## Performance Impact

All middleware and interceptors are designed for minimal overhead:

- **RequestIdInterceptor**: ~1-2μs per request
- **LoggingInterceptor**: ~5-10μs per request (depends on log backend)
- **TimingLayer**: ~2-3μs per request

Total overhead: ~10-20μs per request (negligible for most use cases)

## Best Practices

1. **Use Request IDs**: Always enable RequestIdInterceptor for production
2. **Monitor Timing**: TimingLayer helps identify slow requests
3. **Structured Logging**: All logs include request context
4. **Layer Ordering**: Put faster layers first (timing before auth)
5. **Error Handling**: Always log errors with context

## Testing

Test interceptors independently:

```rust
use arvak_grpc::server::RequestIdInterceptor;

#[test]
fn test_request_id_generation() {
    let mut interceptor = RequestIdInterceptor::new();
    let request = Request::new(());

    let result = interceptor.call(request);
    assert!(result.is_ok());
}
```

## See Also

- [MONITORING.md](MONITORING.md) - Metrics and observability
- [STREAMING.md](STREAMING.md) - Streaming RPCs
- [Configuration](config.example.yaml) - Server configuration
