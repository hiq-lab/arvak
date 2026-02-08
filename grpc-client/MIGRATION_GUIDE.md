# Phase 2 Migration Guide

Guide for migrating from Phase 1 (sync-only) to Phase 2 (async + advanced features).

## Overview

Phase 2 adds powerful new capabilities while maintaining 100% backward compatibility:

- **AsyncArvakClient**: Async/await API with connection pooling
- **JobFuture**: Promise-like non-blocking job results
- **RetryPolicy**: Automatic retry with exponential backoff
- **CircuitBreaker**: Prevent cascading failures
- **BatchJobManager**: Enhanced concurrent batch execution

**Your existing Phase 1 code continues to work without changes.**

## Quick Start

### Installing Phase 2

```bash
# If you already have arvak_grpc installed
pip install --upgrade arvak-grpc

# Or install from source
cd python && pip install -e .
```

### Import Changes

Phase 2 adds new imports (Phase 1 imports unchanged):

```python
# Phase 1 (still works)
from arvak_grpc import ArvakClient

# Phase 2 additions
from arvak_grpc import (
    AsyncArvakClient,      # Async client
    JobFuture,             # Non-blocking results
    RetryPolicy,           # Retry configuration
    CircuitBreaker,        # Circuit breaker
    ResilientClient,       # Combined retry + circuit breaker
    BatchJobManager,       # Enhanced batch operations
)
```

## Migration Patterns

### Pattern 1: Sync → Async (Performance)

**Phase 1 (blocking):**
```python
client = ArvakClient("localhost:50051")

job_id = client.submit_qasm(qasm, "simulator", shots=1000)
result = client.wait_for_job(job_id)

client.close()
```

**Phase 2 (async, better throughput):**
```python
import asyncio

async def main():
    async with AsyncArvakClient("localhost:50051") as client:
        job_id = await client.submit_qasm(qasm, "simulator", shots=1000)
        result = await client.wait_for_job(job_id)

asyncio.run(main())
```

**Benefits:**
- Non-blocking I/O
- Concurrent submissions with `asyncio.gather()`
- Connection pooling reduces overhead
- Better for high-throughput workloads

### Pattern 2: Blocking → Non-blocking (Responsiveness)

**Phase 1 (blocks until complete):**
```python
client = ArvakClient("localhost:50051")

job_id = client.submit_qasm(qasm, "simulator", shots=1000)
# Blocked here until job completes
result = client.wait_for_job(job_id)
```

**Phase 2 (non-blocking with callbacks):**
```python
client = ArvakClient("localhost:50051")

# Get future immediately
future = client.submit_qasm_future(qasm, "simulator", shots=1000)

# Register callback
future.add_done_callback(lambda f: print(f"Done: {f.result()}"))

# Do other work while job runs
do_other_work()

# Block only when needed
result = future.result(timeout=30)
```

**Benefits:**
- Immediate return from submission
- Event-driven with callbacks
- Better UI responsiveness
- Parallel work while jobs execute

### Pattern 3: Manual Retry → Automatic Retry (Reliability)

**Phase 1 (manual retry):**
```python
client = ArvakClient("localhost:50051")

max_retries = 3
for attempt in range(max_retries):
    try:
        job_id = client.submit_qasm(qasm, "simulator", shots=1000)
        break
    except grpc.RpcError as e:
        if attempt == max_retries - 1:
            raise
        time.sleep(2 ** attempt)  # Exponential backoff
```

**Phase 2 (automatic retry):**
```python
from arvak_grpc import ResilientClient, RetryPolicy

client = ResilientClient(
    ArvakClient("localhost:50051"),
    retry_policy=RetryPolicy(max_attempts=3)
)

# Automatic retry with exponential backoff
job_id = client.submit_qasm(qasm, "simulator", shots=1000)
```

**Benefits:**
- Automatic exponential backoff
- Configurable retry policies
- Handles transient failures
- Less boilerplate code

### Pattern 4: Sequential → Concurrent Batch (Speed)

**Phase 1 (sequential):**
```python
client = ArvakClient("localhost:50051")

results = []
for circuit, shots in circuits:
    job_id = client.submit_qasm(circuit, "simulator", shots)
    result = client.wait_for_job(job_id)
    results.append(result)
```

**Phase 2 (concurrent with progress):**
```python
from arvak_grpc import BatchJobManager, print_progress_bar

with BatchJobManager(client, max_workers=10) as manager:
    result = manager.execute_batch(
        circuits,
        "simulator",
        progress_callback=print_progress_bar
    )

    print(f"Completed {result.success_count} jobs")
```

**Benefits:**
- Parallel execution (10x+ faster)
- Progress tracking
- Partial failure handling
- Throughput metrics

## Feature Comparison

| Feature | Phase 1 | Phase 2 | When to Use |
|---------|---------|---------|-------------|
| **Sync Client** | ✅ | ✅ | Simple scripts, notebooks |
| **Async Client** | ❌ | ✅ | High throughput, web servers |
| **JobFuture** | ❌ | ✅ | Non-blocking UIs, event-driven |
| **Retry Logic** | Manual | Automatic | Unreliable networks |
| **Circuit Breaker** | ❌ | ✅ | Prevent cascade failures |
| **Batch Manager** | Basic | Enhanced | Large-scale batch jobs |
| **Connection Pool** | ❌ | ✅ | Reduce connection overhead |
| **Progress Tracking** | ❌ | ✅ | Long-running batches |

## Common Use Cases

### Use Case 1: Jupyter Notebook

**Recommendation:** Stick with Phase 1 sync client (simplest)

```python
# Phase 1 - Perfect for notebooks
from arvak_grpc import ArvakClient

client = ArvakClient("localhost:50051")
job_id = client.submit_qasm(qasm, "simulator", shots=1000)
result = client.wait_for_job(job_id)

print(result.counts)
```

### Use Case 2: Web Application

**Recommendation:** Use Phase 2 async client (non-blocking)

```python
# Phase 2 - Async for web frameworks
from fastapi import FastAPI
from arvak_grpc import AsyncArvakClient

app = FastAPI()
client = AsyncArvakClient("localhost:50051")

@app.post("/submit")
async def submit_job(qasm: str):
    job_id = await client.submit_qasm(qasm, "simulator", 1000)
    return {"job_id": job_id}

@app.get("/result/{job_id}")
async def get_result(job_id: str):
    result = await client.get_job_result(job_id)
    return {"counts": result.counts}
```

### Use Case 3: Large Batch Processing

**Recommendation:** Use Phase 2 BatchJobManager (speed + tracking)

```python
# Phase 2 - Batch manager for large jobs
from arvak_grpc import ArvakClient, BatchJobManager

client = ArvakClient("localhost:50051")

with BatchJobManager(client, max_workers=20) as manager:
    # 100 jobs in parallel
    result = manager.execute_batch(
        circuits,
        "simulator",
        progress_callback=lambda p: print(f"{p.percent_complete:.1f}%")
    )
```

### Use Case 4: Production Service

**Recommendation:** Use Phase 2 with retry + circuit breaker (reliability)

```python
# Phase 2 - Full resilience stack
from arvak_grpc import (
    ArvakClient,
    ResilientClient,
    RetryPolicy,
    CircuitBreakerConfig,
)

client = ResilientClient(
    ArvakClient("localhost:50051"),
    retry_policy=RetryPolicy(max_attempts=5, initial_backoff=1.0),
    circuit_breaker_config=CircuitBreakerConfig(failure_threshold=10)
)

# Automatic retry + circuit breaker protection
job_id = client.submit_qasm(qasm, "simulator", shots=1000)
```

## Performance Impact

### Throughput Comparison

**Scenario:** Submit 50 jobs, 100 shots each

| Approach | Time | Throughput | Code Complexity |
|----------|------|------------|-----------------|
| Phase 1 Sequential | 25.0s | 2.0 jobs/s | Simple |
| Phase 1 with threads | 5.2s | 9.6 jobs/s | Medium |
| Phase 2 AsyncClient | 3.8s | 13.2 jobs/s | Medium |
| Phase 2 BatchManager | 3.1s | 16.1 jobs/s | Simple |

**Recommendation:** Use BatchJobManager for best performance with simple code.

### Memory Usage

- **Phase 1:** ~5MB per client
- **Phase 2 Sync:** ~5MB (same as Phase 1)
- **Phase 2 Async:** ~8MB (connection pool overhead)
- **Phase 2 Batch (10 workers):** ~12MB (thread pool)

All values are approximate and depend on workload.

## Breaking Changes

**None!** Phase 2 is 100% backward compatible.

All Phase 1 code continues to work without modification.

## Troubleshooting

### Issue: "No module named 'arvak_grpc.async_client'"

**Solution:** Upgrade to Phase 2

```bash
pip install --upgrade arvak-grpc
```

### Issue: Async client slower than sync

**Possible causes:**
1. Not using concurrent operations (use `asyncio.gather()`)
2. Pool size too small (increase `pool_size` parameter)
3. Network latency dominant (async helps less)

**Solution:** Use AsyncClient with concurrent submission:

```python
async with AsyncArvakClient("localhost:50051", pool_size=20) as client:
    tasks = [client.submit_qasm(qasm, "simulator") for _ in range(50)]
    job_ids = await asyncio.gather(*tasks)
```

### Issue: Circuit breaker opens too frequently

**Solution:** Tune thresholds:

```python
CircuitBreakerConfig(
    failure_threshold=10,  # Increase threshold
    timeout=60.0,          # Longer timeout before retry
    success_threshold=3,   # More successes to close
)
```

## Best Practices

### 1. Choose the Right Client

- **Notebooks/Scripts:** `ArvakClient` (Phase 1)
- **Web Apps:** `AsyncArvakClient` (Phase 2)
- **Batch Jobs:** `BatchJobManager` (Phase 2)

### 2. Always Use Context Managers

```python
# Good
with ArvakClient("localhost:50051") as client:
    result = client.submit_qasm(qasm, "simulator", 1000)

# Bad
client = ArvakClient("localhost:50051")
result = client.submit_qasm(qasm, "simulator", 1000)
# Forgot to call client.close()!
```

### 3. Handle Partial Failures in Batches

```python
result = manager.execute_batch(circuits, "simulator")

if result.status == BatchStatus.PARTIAL:
    print(f"Warning: {result.failure_count} jobs failed")
    for job_id, error in result.failures:
        print(f"  {job_id}: {error}")
```

### 4. Tune Retry Policies for Your Network

```python
# Fast local network
RetryPolicy(max_attempts=2, initial_backoff=0.1)

# Slow/unreliable network
RetryPolicy(max_attempts=5, initial_backoff=2.0, max_backoff=60.0)
```

## Next Steps

1. **Try Phase 2 features** in a test environment
2. **Benchmark** your workload (Phase 1 vs Phase 2)
3. **Gradually migrate** hot paths to async/batch
4. **Add resilience** to production services

## Examples

Complete examples available in `grpc-client/arvak_grpc/examples/`:

- `async_submit.py` - Async client usage
- `job_future_example.py` - JobFuture patterns
- `resilience_example.py` - Retry and circuit breaker
- `batch_manager_example.py` - Batch operations

## Getting Help

- **GitHub Issues:** https://github.com/hiq-lab/arvak/issues
- **Examples:** `grpc-client/arvak_grpc/examples/`
- **Tests:** `grpc-client/tests/`

---

**Phase 2 Version:** 1.2.0
**Backward Compatible:** Yes ✅
**Migration Required:** No ✅
