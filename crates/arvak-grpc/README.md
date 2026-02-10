# Arvak gRPC Service API

Production-ready gRPC service for remote quantum circuit submission and execution with comprehensive observability, resource management, and streaming support.

## Overview

The Arvak gRPC service provides a language-agnostic API for executing quantum circuits on various backends. It enables:

- **Remote Execution**: Submit circuits via gRPC from any language
- **Asynchronous Processing**: Non-blocking job execution
- **Multi-Backend Support**: Simulator, IQM, IBM, and custom backends
- **Streaming RPCs**: Real-time job monitoring and batch processing
- **Production Features**: Metrics, tracing, resource limits, graceful shutdown
- **Flexible Configuration**: YAML files, environment variables, .env support

## Features

### Core RPCs

**Unary RPCs:**
1. **SubmitJob**: Submit a single circuit for execution
2. **SubmitBatch**: Submit multiple circuits in one call
3. **GetJobStatus**: Check job execution status
4. **GetJobResult**: Retrieve measurement counts
5. **CancelJob**: Cancel a pending or running job
6. **ListBackends**: Get all available backends
7. **GetBackendInfo**: Get detailed backend capabilities

**Streaming RPCs:**
8. **WatchJob**: Server streaming for real-time job status updates
9. **StreamResults**: Server streaming for paginated result delivery
10. **SubmitBatchStream**: Bidirectional streaming for batch processing with live feedback

See [STREAMING.md](STREAMING.md) for streaming patterns and examples.

### Production Features

✅ **Configuration Management**
- YAML configuration files
- Environment variable overrides
- .env file support
- Command-line arguments

✅ **Resource Management**
- Queue capacity limits (max queued jobs)
- Per-client rate limiting
- Job timeout enforcement
- Result size validation

✅ **Observability**
- Prometheus metrics (9 metrics)
- Health check endpoints
- OpenTelemetry distributed tracing
- Structured logging (console or JSON)
- Grafana dashboard

✅ **Middleware & Interceptors**
- Request ID generation and propagation
- Request/response logging
- Request timing and latency tracking
- Client IP tracking

✅ **Operational Excellence**
- Graceful shutdown (SIGTERM/SIGINT)
- Configurable timeouts and keep-alive
- Connection management
- Error handling with structured logging

✅ **Storage**
- Pluggable storage backend architecture
- In-memory storage (default)
- SQLite/PostgreSQL support (future)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Arvak gRPC Service                        │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Middleware Layers                                   │   │
│  │  - TimingLayer (request duration tracking)          │   │
│  │  - ConnectionInfoLayer (connection metadata)        │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Interceptors                                        │   │
│  │  - RequestIdInterceptor (UUID generation)           │   │
│  │  - LoggingInterceptor (request logging)             │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ▼                                   │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  ArvakServiceImpl                                    │   │
│  │  - Resource limits enforcement                       │   │
│  │  - Job submission & execution                        │   │
│  │  - Streaming RPC handlers                            │   │
│  └─────────────────────────────────────────────────────┘   │
│                           ▼                                   │
│  ┌──────────────┬──────────────────┬──────────────────┐   │
│  │  JobStore    │  BackendRegistry │  ResourceManager │   │
│  │  (Storage)   │  (Backends)      │  (Limits)        │   │
│  └──────────────┴──────────────────┴──────────────────┘   │
│                                                               │
│  HTTP Server (Port 8080):                                    │
│  - /health, /health/ready (Health checks)                   │
│  - /metrics (Prometheus metrics)                             │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │ gRPC (HTTP/2)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Client Libraries                         │
│  - Python: arvak_grpc package                               │
│  - Rust: Use generated tonic client                         │
│  - Other: Generate from proto/arvak.proto                   │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### Installation

```bash
# Clone repository
git clone https://github.com/hiq-lab/arvak.git
cd arvak/crates/arvak-grpc

# Build
cargo build --release
```

### Running the Server

**Default configuration:**
```bash
cargo run --release --bin arvak-grpc-server
```

**With configuration file:**
```bash
# Copy example config
cp config.example.yaml config.yaml

# Edit config.yaml as needed, then:
cargo run --release --bin arvak-grpc-server -- --config config.yaml
```

**With environment variables:**
```bash
# Copy example .env
cp .env.example .env

# Edit .env, then:
ARVAK_GRPC_ADDRESS=127.0.0.1:9090 \
ARVAK_LOG_LEVEL=debug \
cargo run --release --bin arvak-grpc-server
```

### Configuration

The server supports multiple configuration sources with the following precedence:
1. Environment variables (highest)
2. Configuration file
3. Default values (lowest)

**Configuration file (config.yaml):**
```yaml
server:
  address: "0.0.0.0:50051"
  timeout_seconds: 60
  shutdown_timeout_seconds: 30

storage:
  backend: "memory"

observability:
  http_server:
    address: "0.0.0.0:8080"
    metrics_enabled: true
    health_enabled: true
  logging:
    level: "info"
    format: "console"  # or "json"
  tracing:
    enabled: false
    # otlp_endpoint: "http://localhost:4317"

limits:
  max_concurrent_jobs: 100
  max_queued_jobs: 1000
  job_timeout_seconds: 3600
  rate_limit_rps: 100
```

**Environment variables:**
```bash
ARVAK_GRPC_ADDRESS=0.0.0.0:50051      # gRPC server address
ARVAK_HTTP_ADDRESS=0.0.0.0:8080       # HTTP server address
ARVAK_LOG_LEVEL=info                   # trace, debug, info, warn, error
ARVAK_LOG_FORMAT=console               # console or json
ARVAK_STORAGE_TYPE=memory              # memory, sqlite, postgres
ARVAK_MAX_CONCURRENT_JOBS=100          # Resource limits
ARVAK_OTLP_ENDPOINT=http://localhost:4317  # OpenTelemetry
```

See [config.example.yaml](config.example.yaml) and [.env.example](.env.example) for all options.

## Client Examples

### Python Client

```python
from arvak_grpc import ArvakClient

# Create client
client = ArvakClient("localhost:50051")

# Submit single job
qasm = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""
job_id = client.submit_qasm(qasm, "simulator", shots=1000)

# Wait for completion
result = client.wait_for_job(job_id)
print(f"Counts: {result.counts}")
print(f"Probabilities: {result.probabilities()}")

# Real-time monitoring with streaming
for update in client.watch_job(job_id):
    print(f"Status: {update.state}")
    if update.state in (JobState.COMPLETED, JobState.FAILED):
        break

client.close()
```

### Rust Client

```rust
use arvak_grpc::proto::{arvak_service_client::ArvakServiceClient, *};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ArvakServiceClient::connect("http://localhost:50051").await?;

    // Submit job
    let request = tonic::Request::new(SubmitJobRequest {
        circuit: Some(CircuitPayload {
            format: Some(circuit_payload::Format::Qasm3(
                "OPENQASM 3.0;\nqubit[2] q;\nh q[0];\ncx q[0], q[1];\n".into()
            )),
        }),
        backend_id: "simulator".into(),
        shots: 1000,
    });

    let response = client.submit_job(request).await?;
    let job_id = response.into_inner().job_id;
    println!("Job submitted: {}", job_id);

    // Watch job status (streaming)
    let watch_request = tonic::Request::new(WatchJobRequest {
        job_id: job_id.clone(),
    });

    let mut stream = client.watch_job(watch_request).await?.into_inner();
    while let Some(update) = stream.message().await? {
        println!("Status: {:?}", update.state);
    }

    Ok(())
}
```

## Monitoring

### Health Checks

```bash
# Basic health check
curl http://localhost:8080/health

# Readiness check (checks backend availability)
curl http://localhost:8080/health/ready
```

### Metrics

```bash
# Prometheus metrics
curl http://localhost:8080/metrics
```

**Available metrics:**
- `arvak_jobs_submitted_total` - Total jobs submitted
- `arvak_jobs_completed_total` - Total jobs completed
- `arvak_jobs_failed_total` - Total jobs failed
- `arvak_job_duration_milliseconds` - Job execution duration
- `arvak_queue_time_milliseconds` - Time in queue
- `arvak_active_jobs` - Currently running jobs
- `arvak_queued_jobs` - Jobs waiting to execute
- `arvak_backend_available` - Backend availability status
- `arvak_rpc_duration_milliseconds` - RPC call duration

### Grafana Dashboard

A pre-built Grafana dashboard is available:

```bash
# Start monitoring stack
docker-compose -f docker-compose.monitoring.yml up -d

# Access Grafana at http://localhost:3000
# Default credentials: admin/admin
```

See [MONITORING.md](MONITORING.md) for complete monitoring setup.

## Examples

Run the included examples:

```bash
# Simple client example
cargo run --example simple_client

# Custom storage backend
cargo run --example custom_storage

# Health and metrics
cargo run --example health_metrics

# Streaming demo (all 3 streaming patterns)
cargo run --example streaming_demo

# Configuration example
cargo run --example config_example
```

## Documentation

- **[STREAMING.md](STREAMING.md)** - Streaming RPC patterns and usage
- **[MONITORING.md](MONITORING.md)** - Metrics, health checks, and observability
- **[MIDDLEWARE.md](MIDDLEWARE.md)** - Interceptors and middleware guide
- **[config.example.yaml](config.example.yaml)** - Complete configuration reference
- **[.env.example](.env.example)** - Environment variable reference

## API Reference

### Job Lifecycle

```
┌──────────┐
│ QUEUED   │  ← Job submitted
└────┬─────┘
     │
     ▼
┌──────────┐
│ RUNNING  │  ← Backend executing
└────┬─────┘
     │
     ├─ Success ──▶ ┌───────────┐
     │              │ COMPLETED │
     │              └───────────┘
     │
     └─ Failure ──▶ ┌──────────┐
                    │ FAILED   │
                    └──────────┘
```

### Circuit Formats

- **OpenQASM 3**: Standard quantum assembly language
- **Arvak IR JSON**: Native Arvak intermediate representation (future)

### Error Handling

gRPC status codes:
- `NOT_FOUND`: Job or backend not found
- `INVALID_ARGUMENT`: Invalid circuit or parameters
- `FAILED_PRECONDITION`: Job not in correct state
- `RESOURCE_EXHAUSTED`: Queue full or rate limit exceeded
- `INTERNAL`: Internal server error

## Development

### Building

```bash
# Build everything
cargo build -p arvak-grpc

# Build with specific features
cargo build -p arvak-grpc --features simulator

# Build release
cargo build -p arvak-grpc --release
```

### Testing

```bash
# Run all tests
cargo test -p arvak-grpc

# Run specific test suite
cargo test -p arvak-grpc --lib resource_manager

# Run with logging
RUST_LOG=debug cargo test -p arvak-grpc
```

### Protobuf

The protobuf code is generated automatically during build from `proto/arvak.proto`.

## Performance

- **Non-blocking submission**: Jobs return immediately
- **Concurrent execution**: Multiple jobs run in parallel
- **Thread-safe**: Safe for concurrent client access
- **Throughput**: 100+ jobs/second submission rate
- **Middleware overhead**: ~10-20μs per request
- **Memory efficient**: Streaming for large results

## Deployment

### Docker

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p arvak-grpc

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/arvak-grpc-server /usr/local/bin/
COPY config.yaml /etc/arvak/config.yaml
CMD ["arvak-grpc-server", "--config", "/etc/arvak/config.yaml"]
```

### Kubernetes

```yaml
apiVersion: v1
kind: Service
metadata:
  name: arvak-grpc
spec:
  ports:
  - name: grpc
    port: 50051
  - name: http
    port: 8080
  selector:
    app: arvak-grpc
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: arvak-grpc
spec:
  replicas: 3
  selector:
    matchLabels:
      app: arvak-grpc
  template:
    metadata:
      labels:
        app: arvak-grpc
    spec:
      containers:
      - name: arvak-grpc
        image: arvak-grpc:latest
        ports:
        - containerPort: 50051
        - containerPort: 8080
        env:
        - name: ARVAK_LOG_FORMAT
          value: "json"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
```

## Graceful Shutdown

The server responds to SIGTERM and SIGINT signals:

```bash
# Send SIGTERM
kill -TERM <pid>

# Or use Ctrl+C (SIGINT)
```

Shutdown process:
1. Stop accepting new connections
2. Wait for in-flight requests to complete (default: 30s timeout)
3. Shut down gRPC and HTTP servers
4. Clean exit

Configure shutdown timeout:
```yaml
server:
  shutdown_timeout_seconds: 30
```

## Security Considerations

**Current (Development):**
- No authentication/authorization
- Plain HTTP/2 (no TLS)
- Open access

**Production Recommendations:**
1. Deploy behind a reverse proxy (e.g., Envoy, nginx)
2. Enable TLS for gRPC
3. Add authentication (API keys, JWT, mTLS)
4. Configure rate limiting per client
5. Use network policies in Kubernetes
6. Enable audit logging

## Roadmap

### Phase 1-4: Complete ✅
- ✅ Core gRPC API (7 unary + 3 streaming RPCs)
- ✅ Python client library (sync + async with streaming)
- ✅ Configuration system (YAML + env vars)
- ✅ Resource management and quotas
- ✅ Observability (Prometheus, OpenTelemetry, health checks)
- ✅ Middleware and interceptors
- ✅ Graceful shutdown
- ✅ Pluggable storage architecture
- ✅ SQLite and PostgreSQL storage backends

### Phase 5: Production Security & Resilience (In Planning)
- [ ] TLS/SSL support with mTLS
- [ ] Authentication (API keys, JWT, mTLS)
- [ ] Authorization and access control (RBAC)
- [ ] Job persistence and recovery
- [ ] Distributed execution (Redis-backed)
- [ ] Advanced scheduling (priorities, fair scheduling)

### Phase 6: Advanced Features (Planned)
- [ ] Circuit optimization pipeline
- [ ] Result caching and deduplication
- [ ] Cost estimation and billing
- [ ] Multi-region deployment

### Phase 7: Enterprise Features (Planned)
- [ ] LDAP/Active Directory integration
- [ ] SAML SSO support
- [ ] Advanced audit logging
- [ ] Compliance reporting (SOC2, HIPAA)

## License

Apache License 2.0

## Contributing

See the main Arvak repository for contribution guidelines.

## Support

- **Issues**: https://github.com/hiq-lab/arvak/issues
- **Documentation**: See markdown files in this directory
- **Examples**: See `examples/` directory
