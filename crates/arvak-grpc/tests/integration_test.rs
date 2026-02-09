// //! Integration tests for Arvak gRPC service.

use arvak_grpc::proto::{arvak_service_client::ArvakServiceClient, *};
use arvak_grpc::server::ArvakServiceImpl;
use tonic::Request;
use tonic::transport::Server;

const TEST_QASM: &str = r"
OPENQASM 3.0;
qubit[2] q;
h q[0];
cx q[0], q[1];
";

/// Start a test server on a random port and return the address.
async fn start_test_server() -> String {
    let service = ArvakServiceImpl::new();
    let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(arvak_grpc::proto::arvak_service_server::ArvakServiceServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give the server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    format!("http://{addr}")
}

#[tokio::test]
async fn test_list_backends() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let response = client
        .list_backends(Request::new(ListBackendsRequest {}))
        .await
        .unwrap();

    let backends = response.into_inner().backends;
    assert!(!backends.is_empty(), "Should have at least one backend");

    // Check simulator backend exists (enabled by default)
    let simulator = backends.iter().find(|b| b.backend_id == "simulator");
    assert!(simulator.is_some(), "Simulator backend should be available");

    let sim = simulator.unwrap();
    assert!(sim.is_available);
    assert!(sim.max_qubits > 0);
    assert!(sim.max_shots > 0);
}

#[tokio::test]
async fn test_get_backend_info() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let response = client
        .get_backend_info(Request::new(GetBackendInfoRequest {
            backend_id: "simulator".to_string(),
        }))
        .await
        .unwrap();

    let backend = response.into_inner().backend.unwrap();
    assert_eq!(backend.backend_id, "simulator");
    assert!(backend.is_available);
    assert!(!backend.supported_gates.is_empty());
}

#[tokio::test]
async fn test_submit_and_get_status() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // Submit job
    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 1000,
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;
    assert!(!job_id.is_empty());

    // Get status
    let response = client
        .get_job_status(Request::new(GetJobStatusRequest {
            job_id: job_id.clone(),
        }))
        .await
        .unwrap();

    let job = response.into_inner().job.unwrap();
    assert_eq!(job.job_id, job_id);
    assert_eq!(job.backend_id, "simulator");
    assert_eq!(job.shots, 1000);
}

#[tokio::test]
async fn test_full_job_lifecycle() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // Submit job
    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 1000,
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;

    // Poll until completed
    let mut completed = false;
    for _ in 0..20 {
        let response = client
            .get_job_status(Request::new(GetJobStatusRequest {
                job_id: job_id.clone(),
            }))
            .await
            .unwrap();

        let job = response.into_inner().job.unwrap();
        let state = JobState::try_from(job.state).unwrap();

        if state == JobState::Completed {
            completed = true;
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(completed, "Job should complete within 2 seconds");

    // Get result
    let response = client
        .get_job_result(Request::new(GetJobResultRequest {
            job_id: job_id.clone(),
        }))
        .await
        .unwrap();

    let result = response.into_inner().result.unwrap();
    assert_eq!(result.job_id, job_id);
    assert_eq!(result.shots, 1000);
    assert!(!result.counts.is_empty());

    // Bell state should produce 00 and 11
    let total: u64 = result.counts.values().sum();
    assert_eq!(total, 1000);
}

#[tokio::test]
async fn test_submit_batch() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // Submit batch of 3 jobs
    let response = client
        .submit_batch(Request::new(SubmitBatchRequest {
            backend_id: "simulator".to_string(),
            jobs: vec![
                BatchJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    shots: 500,
                },
                BatchJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    shots: 1000,
                },
                BatchJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    shots: 1500,
                },
            ],
        }))
        .await
        .unwrap();

    let job_ids = response.into_inner().job_ids;
    assert_eq!(job_ids.len(), 3);

    // Wait for all to complete
    for job_id in &job_ids {
        let mut completed = false;
        for _ in 0..20 {
            let response = client
                .get_job_status(Request::new(GetJobStatusRequest {
                    job_id: job_id.clone(),
                }))
                .await
                .unwrap();

            let job = response.into_inner().job.unwrap();
            let state = JobState::try_from(job.state).unwrap();

            if state == JobState::Completed {
                completed = true;
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        assert!(completed, "Batch job should complete");
    }
}

#[tokio::test]
async fn test_invalid_backend() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let result = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "nonexistent".to_string(),
            shots: 1000,
        }))
        .await;

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_invalid_circuit() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let result = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3("invalid qasm".to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 1000,
        }))
        .await;

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_job_not_found() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let result = client
        .get_job_status(Request::new(GetJobStatusRequest {
            job_id: "nonexistent-job-id".to_string(),
        }))
        .await;

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_cancel_job() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // Submit a job
    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 1000,
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;

    // Try to cancel (may already be completed due to speed)
    let response = client
        .cancel_job(Request::new(CancelJobRequest {
            job_id: job_id.clone(),
        }))
        .await
        .unwrap();

    let cancel_result = response.into_inner();
    // Either successfully canceled or already in terminal state
    assert!(cancel_result.success || cancel_result.message.contains("terminal state"));
}
