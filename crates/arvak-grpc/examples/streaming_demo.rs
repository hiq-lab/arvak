//! Comprehensive streaming demonstration.
//!
//! This example demonstrates all three streaming patterns:
//! 1. WatchJob - Server streaming for job status updates
//! 2. StreamResults - Server streaming for large result sets
//! 3. SubmitBatchStream - Bidirectional streaming for batch jobs
//!
//! Run with: cargo run --example streaming_demo

use arvak_grpc::proto::arvak_service_client::ArvakServiceClient;
use arvak_grpc::proto::*;
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Arvak gRPC Streaming Demo ===\n");

    // Connect to server
    let mut client = ArvakServiceClient::connect("http://localhost:50051").await?;
    println!("Connected to server\n");

    // Example 1: Watch Job (Server Streaming)
    println!("1. WatchJob - Real-time status updates");
    println!("   Submitting a job...");

    let qasm = r#"
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
"#;

    let submit_req = SubmitJobRequest {
        circuit: Some(CircuitPayload {
            format: Some(circuit_payload::Format::Qasm3(qasm.to_string())),
        }),
        backend_id: "simulator".to_string(),
        shots: 1000,
    };

    let response = client.submit_job(submit_req).await?;
    let job_id = response.into_inner().job_id.clone();
    println!("   Job submitted: {}", job_id);

    // Watch the job status
    println!("   Watching job status...");
    let watch_req = WatchJobRequest {
        job_id: job_id.clone(),
    };

    let mut stream = client.watch_job(watch_req).await?.into_inner();

    while let Some(update) = stream.message().await? {
        let state_name = match JobState::try_from(update.state) {
            Ok(JobState::Queued) => "Queued",
            Ok(JobState::Running) => "Running",
            Ok(JobState::Completed) => "Completed",
            Ok(JobState::Failed) => "Failed",
            Ok(JobState::Canceled) => "Canceled",
            _ => "Unknown",
        };

        println!("   Status: {} at {}", state_name, update.timestamp);

        if matches!(
            JobState::try_from(update.state),
            Ok(JobState::Completed) | Ok(JobState::Failed) | Ok(JobState::Canceled)
        ) {
            break;
        }
    }
    println!("   ✓ Job watch complete\n");

    // Example 2: Stream Results (Server Streaming)
    println!("2. StreamResults - Paginated result streaming");

    let stream_req = StreamResultsRequest {
        job_id: job_id.clone(),
        chunk_size: 100, // Small chunks for demo
    };

    let mut result_stream = client.stream_results(stream_req).await?.into_inner();

    let mut total_counts = 0u64;
    while let Some(chunk) = result_stream.message().await? {
        total_counts += chunk.counts.len() as u64;
        println!(
            "   Chunk {}/{}: {} entries",
            chunk.chunk_index + 1,
            chunk.total_chunks,
            chunk.counts.len()
        );

        if chunk.is_final {
            println!("   ✓ Received all {} result entries\n", total_counts);
        }
    }

    // Example 3: Submit Batch Stream (Bidirectional Streaming)
    println!("3. SubmitBatchStream - Real-time batch processing");

    let batch_stream = async_stream::stream! {
        // Submit 3 jobs via the stream
        for i in 1..=3 {
            let qasm = format!(r#"
OPENQASM 3.0;
qubit[1] q;
h q[0];
"#);

            yield BatchJobSubmission {
                circuit: Some(CircuitPayload {
                    format: Some(circuit_payload::Format::Qasm3(qasm)),
                }),
                backend_id: "simulator".to_string(),
                shots: 100,
                client_request_id: format!("batch-job-{}", i),
            };

            // Small delay between submissions
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    };

    let mut response_stream = client
        .submit_batch_stream(Request::new(batch_stream))
        .await?
        .into_inner();

    println!("   Streaming batch jobs...");
    let mut submitted_count = 0;
    let mut completed_count = 0;

    while let Some(result) = response_stream.message().await? {
        match result.result {
            Some(batch_job_result::Result::Submitted(msg)) => {
                submitted_count += 1;
                println!(
                    "   ✓ Submitted {}: {} ({})",
                    result.client_request_id, result.job_id, msg
                );
            }
            Some(batch_job_result::Result::Completed(job_result)) => {
                completed_count += 1;
                println!(
                    "   ✓ Completed {}: {} counts, {} shots",
                    result.client_request_id,
                    job_result.counts.len(),
                    job_result.shots
                );
            }
            Some(batch_job_result::Result::Error(err)) => {
                println!("   ✗ Error {}: {}", result.client_request_id, err);
            }
            None => {}
        }
    }

    println!(
        "   ✓ Batch complete: {} submitted, {} completed\n",
        submitted_count, completed_count
    );

    println!("=== All Streaming Examples Complete ===");
    Ok(())
}
