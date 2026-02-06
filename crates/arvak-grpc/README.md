# Arvak gRPC Service API

Production-ready gRPC service for remote quantum circuit submission and execution.

## Overview

The Arvak gRPC service provides a language-agnostic API for executing quantum circuits on various backends. It enables:

- Remote circuit submission via gRPC
- Asynchronous job execution
- Multi-backend support (simulator, IQM, IBM, etc.)
- Circuit format flexibility (OpenQASM 3, Arvak IR)
- Job status tracking and result retrieval

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Arvak gRPC Service                        │
│                                                               │
│  - ArvakServiceImpl: Main service implementation            │
│  - JobStore: Thread-safe in-memory job storage             │
│  - BackendRegistry: Feature-gated backend management        │
│  - Async execution: Non-blocking job processing            │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │ gRPC (HTTP/2)
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Client Libraries                         │
│  - Python: arvak_grpc package                               │
│  - Rust: Use generated tonic client                         │
│  - Other: Generate from proto file                          │
└─────────────────────────────────────────────────────────────┘
```

## Features

### Supported RPCs

1. **SubmitJob**: Submit a single circuit for execution
2. **SubmitBatch**: Submit multiple circuits in one call
3. **GetJobStatus**: Check job execution status
4. **GetJobResult**: Retrieve measurement counts
5. **CancelJob**: Cancel a pending or running job
6. **ListBackends**: Get all available backends
7. **GetBackendInfo**: Get detailed backend capabilities

### Circuit Formats

- **OpenQASM 3**: Standard quantum assembly language
- **Arvak IR JSON**: Native Arvak intermediate representation

### Job States

- `QUEUED`: Job accepted, waiting to execute
- `RUNNING`: Job currently executing
- `COMPLETED`: Job finished successfully
- `FAILED`: Job execution failed
- `CANCELED`: Job was canceled

## Getting Started

### Running the Server

```bash
# Build the server
cargo build --release -p arvak-grpc

# Run with default settings (0.0.0.0:50051)
cargo run --release --bin arvak-grpc-server

# Run with custom address
ARVAK_GRPC_ADDR="127.0.0.1:8080" cargo run --release --bin arvak-grpc-server

# Enable logging
RUST_LOG=info cargo run --release --bin arvak-grpc-server
```

### Python Client

```python
from arvak_grpc import ArvakClient

# Create client
client = ArvakClient("localhost:50051")

# Submit circuit
qasm = """
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"""

job_id = client.submit_qasm(qasm, "simulator", shots=1000)

# Wait for results
result = client.wait_for_job(job_id)
print(f"Counts: {result.counts}")

# Get probabilities
probs = result.probabilities()
print(f"Probabilities: {probs}")

client.close()
```

### Rust Client

```rust
use arvak_grpc::proto::{arvak_service_client::ArvakServiceClient, *};
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ArvakServiceClient::connect("http://localhost:50051").await?;

    let request = Request::new(SubmitJobRequest {
        circuit: Some(CircuitPayload {
            format: Some(circuit_payload::Format::Qasm3(
                "OPENQASM 3.0;\nqubit[2] q;\nh q[0];\n".to_string()
            )),
        }),
        backend_id: "simulator".to_string(),
        shots: 1000,
    });

    let response = client.submit_job(request).await?;
    println!("Job ID: {}", response.into_inner().job_id);

    Ok(())
}
```

## Configuration

### Feature Flags

- `simulator`: Enable local simulator backend (default)
- Future: `iqm`, `ibm`, etc.

```toml
[dependencies]
arvak-grpc = { version = "1.1", features = ["simulator"] }
```

### Server Configuration

Environment variables:

- `ARVAK_GRPC_ADDR`: Server bind address (default: `0.0.0.0:50051`)
- `RUST_LOG`: Logging level (e.g., `info`, `debug`)

## Development

### Building

```bash
# Build library and server
cargo build -p arvak-grpc

# Run tests
cargo test -p arvak-grpc

# Run with specific features
cargo build -p arvak-grpc --features simulator
```

### Regenerating Protobuf Code

The protobuf code is generated automatically during build. To regenerate manually:

```bash
cd crates/arvak-grpc
cargo clean
cargo build
```

## API Reference

### Job Lifecycle

1. **Submit** → Job created with state `QUEUED`
2. **Execute** → State changes to `RUNNING`, circuit executes asynchronously
3. **Complete** → State changes to `COMPLETED` (or `FAILED`)
4. **Retrieve** → Results available via `GetJobResult`

### Error Handling

gRPC status codes:

- `NOT_FOUND`: Job or backend not found
- `INVALID_ARGUMENT`: Invalid circuit or parameters
- `FAILED_PRECONDITION`: Job not in correct state (e.g., not completed)
- `INTERNAL`: Internal server error

## Examples

See the `examples/` directory for complete examples:

- **Python**: `python/arvak_grpc/examples/`
  - `submit_job.py`: Single job submission
  - `batch_jobs.py`: Batch submission with multiple circuits

## Performance

- **Non-blocking**: Job submission returns immediately
- **Concurrent**: Multiple jobs execute in parallel
- **Thread-safe**: Safe for concurrent client access
- **Target**: 100+ jobs/second submission rate

## Limitations (Phase 1)

- **In-memory storage**: Jobs not persisted across restarts
- **No authentication**: Open access (add proxy for production)
- **Single server**: No distributed execution
- **Basic error recovery**: No automatic retry

## Roadmap

### Phase 2: Enhanced Features
- Python async API with JobFuture objects
- Batch operations with concurrent.futures
- Connection pooling and retry logic

### Phase 3: Async Execution
- JobFuture.wait() / JobFuture.result()
- Callback registration for job events
- Server-side async job queue with priorities

### Phase 4: Data Formats
- Apache Arrow result export
- Parquet file serialization
- Zero-copy pandas conversion

## License

Apache License 2.0

## Contributing

See the main Arvak repository for contribution guidelines.
