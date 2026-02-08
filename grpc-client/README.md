# Arvak gRPC Python Client

Python client library for the Arvak gRPC quantum computing service.

## Features

### Phase 1: Core Functionality
- ✅ **Synchronous client** for simple use cases
- ✅ **7 gRPC RPCs**: Submit, status, result, cancel, batch, list/get backends
- ✅ **OpenQASM 3** circuit format support
- ✅ **Error handling** with custom exceptions
- ✅ **Type hints** throughout

### Phase 2: Advanced Features
- ✅ **Async/await API** with `AsyncArvakClient`
- ✅ **Connection pooling** for better performance
- ✅ **JobFuture** for non-blocking results
- ✅ **Automatic retry** with exponential backoff
- ✅ **Circuit breaker** pattern
- ✅ **BatchJobManager** for concurrent execution
- ✅ **Progress tracking** with callbacks

## Installation

```bash
# From PyPI (when published)
pip install arvak-grpc

# From source
cd python
pip install -e .
```

## Quick Start

### Basic Usage (Phase 1)

```python
from arvak_grpc import ArvakClient

# Connect to server
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

client.close()
```

### Async API (Phase 2)

```python
import asyncio
from arvak_grpc import AsyncArvakClient

async def main():
    async with AsyncArvakClient("localhost:50051") as client:
        # Submit multiple jobs concurrently
        tasks = [
            client.submit_qasm(qasm, "simulator", shots=1000)
            for _ in range(10)
        ]

        job_ids = await asyncio.gather(*tasks)

        # Wait for all results
        results = await asyncio.gather(*[
            client.wait_for_job(job_id) for job_id in job_ids
        ])

        print(f"Completed {len(results)} jobs")

asyncio.run(main())
```

### Non-blocking with JobFuture (Phase 2)

```python
from arvak_grpc import ArvakClient

client = ArvakClient("localhost:50051")

# Get future immediately
future = client.submit_qasm_future(qasm, "simulator", shots=1000)

# Register callback
future.add_done_callback(lambda f: print(f"Done: {f.result()}"))

# Do other work...
do_other_work()

# Block only when needed
result = future.result(timeout=30)
```

### Resilient Client (Phase 2)

```python
from arvak_grpc import ArvakClient, ResilientClient, RetryPolicy

client = ResilientClient(
    ArvakClient("localhost:50051"),
    retry_policy=RetryPolicy(max_attempts=5, initial_backoff=1.0)
)

# Automatic retry on transient failures
job_id = client.submit_qasm(qasm, "simulator", shots=1000)
```

### Batch Operations (Phase 2)

```python
from arvak_grpc import ArvakClient, BatchJobManager, print_progress_bar

client = ArvakClient("localhost:50051")

with BatchJobManager(client, max_workers=10) as manager:
    circuits = [(qasm, 1000) for _ in range(50)]

    result = manager.execute_batch(
        circuits,
        "simulator",
        progress_callback=print_progress_bar
    )

    print(f"Completed {result.success_count} jobs in {result.total_time:.2f}s")
```

## API Reference

### ArvakClient (Sync)

```python
client = ArvakClient(address="localhost:50051", timeout=30.0)

# Submit operations
job_id = client.submit_qasm(qasm_code, backend_id, shots=1024)
job_id = client.submit_circuit_json(json_code, backend_id, shots=1024)
job_ids = client.submit_batch(circuits, backend_id, format="qasm3")

# Status and results
job = client.get_job_status(job_id)
result = client.get_job_result(job_id)
result = client.wait_for_job(job_id, poll_interval=1.0, max_wait=None)

# Job control
success, message = client.cancel_job(job_id)

# Backend operations
backends = client.list_backends()
backend = client.get_backend_info(backend_id)

# JobFuture support (Phase 2)
future = client.submit_qasm_future(qasm_code, backend_id, shots=1024)
futures = client.submit_batch_future(circuits, backend_id)
```

### AsyncArvakClient (Async)

```python
client = AsyncArvakClient(address="localhost:50051", timeout=30.0, pool_size=10)

# All methods are async (use with await)
job_id = await client.submit_qasm(qasm_code, backend_id, shots=1024)
result = await client.wait_for_job(job_id)
backends = await client.list_backends()

# Connection pooling automatically managed
```

### JobFuture

```python
future = client.submit_qasm_future(qasm, "simulator", shots=1000)

# Check status
is_done = future.done()
is_cancelled = future.cancelled()
is_running = future.running()

# Get result (blocks)
result = future.result(timeout=30)

# Wait without result
success = future.wait(timeout=30)

# Cancel
cancelled = future.cancel()

# Callbacks
future.add_done_callback(lambda f: print(f.result()))

# Convert to concurrent.futures.Future
concurrent_future = future.as_concurrent_future()
```

### RetryPolicy

```python
from arvak_grpc import RetryPolicy, RetryStrategy

policy = RetryPolicy(
    max_attempts=3,
    initial_backoff=1.0,
    max_backoff=60.0,
    backoff_multiplier=2.0,
    jitter=True,
    strategy=RetryStrategy.EXPONENTIAL_BACKOFF
)
```

### CircuitBreaker

```python
from arvak_grpc import CircuitBreaker, CircuitBreakerConfig

breaker = CircuitBreaker(
    CircuitBreakerConfig(
        failure_threshold=5,
        success_threshold=2,
        timeout=60.0
    )
)
```

### BatchJobManager

```python
manager = BatchJobManager(client, max_workers=10)

# Execute batch
result = manager.execute_batch(
    circuits,
    backend_id,
    timeout=None,
    progress_callback=print_progress_bar,
    fail_fast=False
)

# Submit and process as completed
futures = manager.submit_many(circuits, backend_id)
for future in manager.as_completed(futures):
    result = future.result()
    process(result)

# Map function over results
results = manager.map(extract_data, futures, timeout=60)
```

## Data Types

### Job

```python
@dataclass
class Job:
    job_id: str
    state: JobState  # QUEUED, RUNNING, COMPLETED, FAILED, CANCELED
    submitted_at: datetime
    backend_id: str
    shots: int
    started_at: Optional[datetime]
    completed_at: Optional[datetime]
    error_message: Optional[str]
```

### JobResult

```python
@dataclass
class JobResult:
    job_id: str
    counts: Dict[str, int]
    shots: int
    execution_time_ms: Optional[int]
    metadata: Optional[Dict]

    def probabilities() -> Dict[str, float]
    def most_frequent() -> Optional[tuple[str, float]]
```

### BackendInfo

```python
@dataclass
class BackendInfo:
    backend_id: str
    name: str
    is_available: bool
    max_qubits: int
    max_shots: int
    description: str
    supported_gates: list[str]
    topology: Optional[Dict]
```

## Examples

Complete examples in `examples/`:

- `submit_job.py` - Basic job submission
- `batch_jobs.py` - Batch submission
- `async_submit.py` - Async client with concurrency
- `job_future_example.py` - JobFuture patterns
- `resilience_example.py` - Retry and circuit breaker
- `batch_manager_example.py` - Batch manager

## Testing

```bash
# Install test dependencies
pip install pytest pytest-asyncio

# Run tests (requires server running on localhost:50051)
pytest tests/ -v

# Run specific test file
pytest tests/test_client.py -v
pytest tests/test_async_client.py -v
```

## Migration Guide

See [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) for upgrading from Phase 1 to Phase 2.

## Performance

### Throughput Comparison

50 jobs, 100 shots each:

| Method | Time | Throughput | Notes |
|--------|------|------------|-------|
| Sequential (Phase 1) | 25.0s | 2.0 jobs/s | Simple but slow |
| Async (Phase 2) | 3.8s | 13.2 jobs/s | Best for I/O |
| BatchManager (Phase 2) | 3.1s | 16.1 jobs/s | Best overall |

### Recommendations

- **Single jobs:** Use sync client (Phase 1)
- **< 10 concurrent jobs:** Use async client
- **> 10 concurrent jobs:** Use BatchJobManager
- **Production:** Add ResilientClient wrapper

## Error Handling

```python
from arvak_grpc.exceptions import (
    ArvakError,
    ArvakJobNotFoundError,
    ArvakBackendNotFoundError,
    ArvakInvalidCircuitError,
    ArvakJobNotCompletedError,
)

try:
    job_id = client.submit_qasm(qasm, "simulator", 1000)
except ArvakInvalidCircuitError as e:
    print(f"Invalid circuit: {e}")
except ArvakBackendNotFoundError as e:
    print(f"Backend not found: {e}")
except ArvakError as e:
    print(f"Error: {e}")
```

## Requirements

- Python 3.9+
- grpcio >= 1.60.0
- protobuf >= 4.25.0

## Development

```bash
# Install in development mode
pip install -e .

# Install development dependencies
pip install -e ".[dev]"

# Generate protobuf code (if proto files change)
python -m grpc_tools.protoc \
    -I ../crates/arvak-grpc/proto \
    --python_out=arvak_grpc \
    --grpc_python_out=arvak_grpc \
    ../crates/arvak-grpc/proto/arvak.proto
```

## License

Apache License 2.0

## Contributing

See the main [Arvak repository](https://github.com/hiq-lab/arvak) for contribution guidelines.

---

**Version:** 1.2.0
**Phase:** 2 (Advanced Features)
**Python:** 3.9+
