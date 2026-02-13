// //! Simple Arvak gRPC client example.
//!
//! This example demonstrates how to:
//! 1. Connect to the Arvak gRPC server
//! 2. List available backends
//! 3. Submit a quantum circuit
//! 4. Poll for job status
//! 5. Retrieve results

use arvak_grpc::proto::{
    CircuitPayload, GetJobResultRequest, GetJobStatusRequest, JobState, ListBackendsRequest,
    SubmitJobRequest, arvak_service_client::ArvakServiceClient, circuit_payload,
};
use tonic::Request;

const BELL_STATE: &str = r"
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let mut client = ArvakServiceClient::connect("http://localhost:50051").await?;
    println!("Connected to Arvak gRPC server");

    // List backends
    println!("\nAvailable backends:");
    let backends_response = client
        .list_backends(Request::new(ListBackendsRequest {}))
        .await?;

    for backend in backends_response.into_inner().backends {
        println!(
            "  - {} ({}): {} qubits, max {} shots",
            backend.backend_id, backend.name, backend.max_qubits, backend.max_shots
        );
    }

    // Submit job
    println!("\nSubmitting Bell state circuit...");
    let submit_request = Request::new(SubmitJobRequest {
        circuit: Some(CircuitPayload {
            format: Some(circuit_payload::Format::Qasm3(BELL_STATE.to_string())),
        }),
        backend_id: "simulator".to_string(),
        shots: 1000,
    });

    let submit_response = client.submit_job(submit_request).await?;
    let job_id = submit_response.into_inner().job_id;
    println!("Job submitted: {job_id}");

    // Poll for completion
    println!("\nWaiting for job to complete...");
    loop {
        let status_request = Request::new(GetJobStatusRequest {
            job_id: job_id.clone(),
        });

        let status_response = client.get_job_status(status_request).await?;
        let job = status_response.into_inner().job.unwrap();

        let state = JobState::try_from(job.state).unwrap_or(JobState::Unspecified);

        match state {
            JobState::Completed => {
                println!("Job completed!");
                break;
            }
            JobState::Failed => {
                println!("Job failed: {}", job.error_message);
                return Ok(());
            }
            JobState::Canceled => {
                println!("Job was canceled");
                return Ok(());
            }
            _ => {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout())?;
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    }

    // Get results
    println!("\nRetrieving results...");
    let result_request = Request::new(GetJobResultRequest {
        job_id: job_id.clone(),
    });

    let result_response = client.get_job_result(result_request).await?;
    let result = result_response.into_inner().result.unwrap();

    println!("\nMeasurement counts:");
    let mut counts: Vec<_> = result.counts.iter().collect();
    counts.sort_by_key(|(bitstring, _)| *bitstring);

    for (bitstring, count) in counts {
        let prob = *count as f64 / f64::from(result.shots);
        println!("  {bitstring}: {count} ({prob:.3})");
    }

    if result.execution_time_ms > 0 {
        println!("\nExecution time: {} ms", result.execution_time_ms);
    }

    println!("\nDone!");

    Ok(())
}
