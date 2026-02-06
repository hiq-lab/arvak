# Arvak gRPC Streaming Guide

This guide covers the three streaming patterns available in Arvak gRPC for real-time job monitoring, large dataset handling, and batch processing.

## Overview

Arvak gRPC provides three types of streaming RPCs:

1. **WatchJob** - Server streaming for real-time job status updates
2. **StreamResults** - Server streaming for paginated result delivery
3. **SubmitBatchStream** - Bidirectional streaming for batch job processing

## 1. WatchJob - Real-time Status Updates

Monitor job execution in real-time without polling.

### Server Streaming Pattern

```
Client --[WatchJobRequest]--> Server
Client <--[JobStatusUpdate]-- Server (stream)
       <--[JobStatusUpdate]-- Server
       <--[JobStatusUpdate]-- Server (final)
```

### Usage (Rust)

```rust
use arvak_grpc::proto::*;

let request = WatchJobRequest {
    job_id: "job-123".to_string(),
};

let mut stream = client.watch_job(request).await?.into_inner();

while let Some(update) = stream.message().await? {
    println!("Job {} is now {:?}", update.job_id, update.state);

    // Stream closes automatically when job reaches terminal state
    if is_terminal_state(update.state) {
        break;
    }
}
```

### JobStatusUpdate Fields

| Field | Type | Description |
|-------|------|-------------|
| `job_id` | string | Job identifier |
| `state` | JobState | Current job state (Queued, Running, Completed, Failed, Canceled) |
| `timestamp` | int64 | Unix timestamp when update was sent |
| `error_message` | string | Error details if state is Failed |

### Features

- **Low latency**: 500ms polling interval
- **Automatic cleanup**: Stream closes when job completes
- **Reconnectable**: Can reconnect if connection drops
- **No polling**: Server pushes updates proactively

## 2. StreamResults - Paginated Result Delivery

Stream large result sets in chunks to avoid memory issues.

### Server Streaming Pattern

```
Client --[StreamResultsRequest]--> Server
Client <--[ResultChunk]----------- Server (chunk 1)
       <--[ResultChunk]----------- Server (chunk 2)
       <--[ResultChunk]----------- Server (final chunk)
```

### Usage (Rust)

```rust
let request = StreamResultsRequest {
    job_id: "job-123".to_string(),
    chunk_size: 1000, // Results per chunk
};

let mut stream = client.stream_results(request).await?.into_inner();

let mut all_counts = HashMap::new();

while let Some(chunk) = stream.message().await? {
    println!("Chunk {}/{}", chunk.chunk_index + 1, chunk.total_chunks);

    // Merge counts from this chunk
    all_counts.extend(chunk.counts);

    if chunk.is_final {
        break;
    }
}

println!("Total entries: {}", all_counts.len());
```

### ResultChunk Fields

| Field | Type | Description |
|-------|------|-------------|
| `job_id` | string | Job identifier |
| `counts` | map<string, uint64> | Partial measurement counts in this chunk |
| `is_final` | bool | True if this is the last chunk |
| `chunk_index` | uint32 | Zero-based index of this chunk |
| `total_chunks` | uint32 | Total number of chunks |

### Configuration

- **Default chunk size**: 1000 entries
- **Recommended for**: Result sets > 10,000 entries
- **Memory efficient**: Processes results incrementally
- **Progress tracking**: Client knows total chunks upfront

## 3. SubmitBatchStream - Batch Processing

Submit multiple jobs with real-time feedback as they complete.

### Bidirectional Streaming Pattern

```
Client --[BatchJobSubmission]--> Server
Client --[BatchJobSubmission]--> Server
Client <--[BatchJobResult]------ Server (job 1 submitted)
Client --[BatchJobSubmission]--> Server
Client <--[BatchJobResult]------ Server (job 1 completed)
Client <--[BatchJobResult]------ Server (job 2 submitted)
Client <--[BatchJobResult]------ Server (job 3 submitted)
       <--[BatchJobResult]------ Server (job 2 completed)
       <--[BatchJobResult]------ Server (job 3 completed)
```

### Usage (Rust)

```rust
use async_stream::stream;

// Create submission stream
let submissions = stream! {
    for i in 1..=10 {
        yield BatchJobSubmission {
            circuit: Some(CircuitPayload { /* ... */ }),
            backend_id: "simulator".to_string(),
            shots: 1000,
            client_request_id: format!("job-{}", i),
        };
    }
};

// Send and receive simultaneously
let mut results = client
    .submit_batch_stream(Request::new(submissions))
    .await?
    .into_inner();

while let Some(result) = results.message().await? {
    match result.result {
        Some(batch_job_result::Result::Submitted(msg)) => {
            println!("✓ {} submitted: {}", result.client_request_id, result.job_id);
        }
        Some(batch_job_result::Result::Completed(job_result)) => {
            println!("✓ {} completed with {} counts",
                result.client_request_id, job_result.counts.len());
        }
        Some(batch_job_result::Result::Error(err)) => {
            eprintln!("✗ {} failed: {}", result.client_request_id, err);
        }
        None => {}
    }
}
```

### BatchJobSubmission Fields

| Field | Type | Description |
|-------|------|-------------|
| `circuit` | CircuitPayload | Circuit to execute (QASM3 or JSON) |
| `backend_id` | string | Target backend |
| `shots` | uint32 | Number of measurement shots |
| `client_request_id` | string | Optional client-provided tracking ID |

### BatchJobResult Fields

| Field | Type | Description |
|-------|------|-------------|
| `job_id` | string | Server-assigned job ID |
| `client_request_id` | string | Echoed from request |
| `result` | oneof | One of: submitted, completed, or error |

### Result States

1. **submitted**: Job accepted and queued
   - Sent immediately when job is created
   - Contains confirmation message

2. **completed**: Job finished successfully
   - Sent when job execution completes
   - Contains full JobResult with counts

3. **error**: Job submission or execution failed
   - Sent if circuit parsing, backend lookup, or execution fails
   - Contains error message string

### Features

- **Concurrent execution**: Jobs run in parallel
- **Real-time feedback**: Get updates as jobs complete
- **Request tracking**: Use client_request_id to correlate submissions
- **Error isolation**: One job failure doesn't stop others
- **Metrics**: All jobs tracked in Prometheus metrics

## Python Client (Future Work)

The Python client will support streaming with async generators:

```python
# WatchJob
async for update in client.watch_job(job_id):
    print(f"Status: {update.state}")
    if is_terminal(update.state):
        break

# StreamResults
async for chunk in client.stream_results(job_id, chunk_size=1000):
    all_counts.update(chunk.counts)

# SubmitBatchStream
async def submit_jobs():
    for circuit in circuits:
        yield BatchJobSubmission(circuit=circuit, ...)

async for result in client.submit_batch_stream(submit_jobs()):
    if result.HasField('completed'):
        print(f"Job done: {result.job_id}")
```

## Performance Considerations

### WatchJob

- **Polling interval**: 500ms (configurable in future)
- **Memory**: Minimal (one update at a time)
- **Connections**: One per watched job
- **Use when**: You need real-time updates

### StreamResults

- **Chunk size**: Balance between roundtrips and memory
  - Small chunks (100-500): More roundtrips, less memory
  - Large chunks (5000-10000): Fewer roundtrips, more memory
- **Default (1000)**: Good balance for most use cases
- **Use when**: Result sets > 10K entries

### SubmitBatchStream

- **Concurrency**: All jobs execute in parallel
- **Throughput**: Limited by backend capacity
- **Memory**: O(concurrent_jobs), not O(total_jobs)
- **Use when**: Submitting > 10 jobs with real-time feedback

## Error Handling

All streaming RPCs return gRPC status codes on error:

```rust
match stream.message().await {
    Ok(Some(update)) => { /* process update */ }
    Ok(None) => { /* stream closed normally */ }
    Err(status) => {
        match status.code() {
            tonic::Code::NotFound => println!("Job not found"),
            tonic::Code::Unavailable => println!("Server unavailable"),
            tonic::Code::Internal => println!("Server error: {}", status.message()),
            _ => println!("Error: {}", status),
        }
    }
}
```

## Examples

### Complete Example

See `examples/streaming_demo.rs` for a comprehensive demonstration:

```bash
# Start server
cargo run --bin arvak-grpc-server

# Run streaming demo (in another terminal)
cargo run --example streaming_demo
```

### Integration Tests

```bash
# Run streaming integration tests
cargo test --test streaming_integration
```

## Best Practices

1. **Always handle stream closure**: Check for `None` from `message().await`
2. **Set reasonable chunk sizes**: Don't go below 100 or above 10,000
3. **Use client_request_id**: Helps correlate batch submissions with results
4. **Handle errors gracefully**: One stream error doesn't mean all jobs failed
5. **Don't block the stream**: Process updates quickly or spawn tasks
6. **Monitor metrics**: Watch for high queue depths or failure rates

## Comparison with Unary RPCs

| Feature | Unary (GetJobStatus) | Streaming (WatchJob) |
|---------|---------------------|---------------------|
| Latency | Requires polling | Real-time updates |
| Network | Multiple roundtrips | Single connection |
| Server load | O(polls) | O(1) per job |
| Client code | Simple polling loop | Async stream processing |
| Use case | Occasional checks | Continuous monitoring |

| Feature | Unary (GetJobResult) | Streaming (StreamResults) |
|---------|---------------------|--------------------------|
| Memory | O(result_size) | O(chunk_size) |
| Large datasets | May timeout/OOM | Handles any size |
| Progress | All-or-nothing | Incremental |
| Use case | Small results | Large results |

## Troubleshooting

### Stream disconnects unexpectedly

- Check network stability
- Verify server hasn't restarted
- Check job hasn't reached terminal state (for WatchJob)

### High memory usage with StreamResults

- Reduce chunk_size
- Process chunks incrementally instead of accumulating

### Batch jobs not completing

- Check server logs for backend errors
- Verify backend is available
- Check resource limits (max_concurrent_jobs)

### Missing updates in WatchJob

- Updates may be coalesced if job transitions rapidly
- Check polling interval setting

## Further Reading

- [gRPC Streaming Concepts](https://grpc.io/docs/what-is-grpc/core-concepts/#server-streaming-rpc)
- [Tonic Streaming Guide](https://github.com/hyperium/tonic/blob/master/examples/src/streaming)
- [Arvak API Reference](../README.md)
