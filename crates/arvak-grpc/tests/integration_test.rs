// //! Integration tests for Arvak gRPC service.

mod strict_backend;

use std::sync::Arc;

use arvak_grpc::proto::{arvak_service_client::ArvakServiceClient, *};
use arvak_grpc::server::{ArvakServiceImpl, BackendRegistry, JobStore};
use tonic::Request;
use tonic::transport::Server;

use strict_backend::StrictBackend;

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
            ..Default::default()
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
            ..Default::default()
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
                    ..Default::default()
                },
                BatchJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    shots: 1000,
                    ..Default::default()
                },
                BatchJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    shots: 1500,
                    ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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

// =============================================================================
// Stress & Edge-Case Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_100_jobs() {
    let addr = start_test_server().await;

    let mut handles = Vec::new();
    for i in 0..100 {
        let addr = addr.clone();
        handles.push(tokio::spawn(async move {
            let mut client = ArvakServiceClient::connect(addr).await.unwrap();

            // Submit
            let response = client
                .submit_job(Request::new(SubmitJobRequest {
                    circuit: Some(CircuitPayload {
                        format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
                    }),
                    backend_id: "simulator".to_string(),
                    shots: 100,
                    ..Default::default()
                }))
                .await
                .unwrap();

            let job_id = response.into_inner().job_id;

            // Poll until completed
            for _ in 0..30 {
                let response = client
                    .get_job_status(Request::new(GetJobStatusRequest {
                        job_id: job_id.clone(),
                    }))
                    .await
                    .unwrap();

                let job = response.into_inner().job.unwrap();
                let state = JobState::try_from(job.state).unwrap();
                if state == JobState::Completed {
                    return (i, job_id, true);
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            (i, job_id, false)
        }));
    }

    let mut completed_count = 0;
    for handle in handles {
        let (idx, _job_id, completed) = handle.await.unwrap();
        assert!(completed, "Job {idx} did not complete within timeout");
        completed_count += 1;
    }
    assert_eq!(completed_count, 100);
}

#[tokio::test]
async fn test_malformed_qasm_variants() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let malformed_variants = vec![
        ("empty string", ""),
        ("only whitespace", "   \n\t\n  "),
        ("missing semicolons", "OPENQASM 3.0\nqubit[2] q\nh q[0]"),
        (
            "invalid gate name",
            "OPENQASM 3.0;\nqubit[2] q;\nfoobar q[0];",
        ),
        (
            "unclosed block",
            "OPENQASM 3.0;\nqubit[2] q;\nif (true) { h q[0];",
        ),
        (
            "negative qubit index",
            "OPENQASM 3.0;\nqubit[2] q;\nh q[-1];",
        ),
        ("binary garbage", "\x00\x01\x02\x03\x04\x05"),
    ];

    for (name, qasm) in malformed_variants {
        let result = client
            .submit_job(Request::new(SubmitJobRequest {
                circuit: Some(CircuitPayload {
                    format: Some(circuit_payload::Format::Qasm3(qasm.to_string())),
                }),
                backend_id: "simulator".to_string(),
                shots: 100,
                ..Default::default()
            }))
            .await;

        assert!(
            result.is_err(),
            "Malformed QASM variant '{name}' should be rejected, but was accepted"
        );
    }
}

#[tokio::test]
async fn test_missing_circuit_payload() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // No circuit at all
    let result = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: None,
            backend_id: "simulator".to_string(),
            shots: 1000,
            ..Default::default()
        }))
        .await;

    assert!(result.is_err());

    // Empty circuit payload (no format)
    let result = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload { format: None }),
            backend_id: "simulator".to_string(),
            shots: 1000,
            ..Default::default()
        }))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_ir_json_format_returns_unimplemented() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let result = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::ArvakIrJson(
                    r#"{"name":"test","qubits":2}"#.to_string(),
                )),
            }),
            backend_id: "simulator".to_string(),
            shots: 100,
            ..Default::default()
        }))
        .await;

    assert!(result.is_err());
    let err = result.err().unwrap();
    // Should return an error indicating IR JSON is not supported
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("not yet supported"),
        "Error message should mention 'not yet supported', got: {}",
        err.message()
    );
}

#[tokio::test]
async fn test_cancel_race_condition() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    // Submit and immediately try to cancel — the simulator is fast so this
    // exercises the race between completion and cancellation.
    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 100,
            ..Default::default()
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;

    // Immediately cancel
    let cancel_response = client
        .cancel_job(Request::new(CancelJobRequest {
            job_id: job_id.clone(),
        }))
        .await
        .unwrap();

    let cancel_result = cancel_response.into_inner();
    // Either successfully canceled or already completed — both are valid
    assert!(
        cancel_result.success || cancel_result.message.contains("terminal state"),
        "Cancel should either succeed or report terminal state"
    );

    // Verify job is in a terminal state
    let status_response = client
        .get_job_status(Request::new(GetJobStatusRequest {
            job_id: job_id.clone(),
        }))
        .await
        .unwrap();

    let job = status_response.into_inner().job.unwrap();
    let state = JobState::try_from(job.state).unwrap();
    assert!(
        state == JobState::Completed || state == JobState::Canceled,
        "Job should be in terminal state after cancel race, got: {:?}",
        state
    );
}

// =============================================================================
// Compilation Smoke Tests
// =============================================================================

/// Start a test server with the strict backend (IQM-like: prx + cz only).
async fn start_strict_test_server() -> String {
    let mut registry = BackendRegistry::new();
    registry.register("strict".to_string(), Arc::new(StrictBackend::new()));

    let service = ArvakServiceImpl::with_components(JobStore::new(), registry);
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

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    format!("http://{addr}")
}

/// Submit h+cx circuit to strict backend WITH compilation (optimization_level=1).
/// The compiler should translate h+cx → prx+cz so the strict backend accepts it.
#[tokio::test]
async fn test_submit_with_compilation() {
    let addr = start_strict_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "strict".to_string(),
            shots: 1024,
            optimization_level: 1,
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;
    assert!(!job_id.is_empty());

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
        assert!(
            state != JobState::Failed,
            "Job failed unexpectedly: {}",
            job.error_message
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(completed, "Compiled job should complete on strict backend");
}

/// Submit h+cx circuit to strict backend WITHOUT compilation (optimization_level=0).
/// The strict backend should reject the unsupported h and cx gates.
#[tokio::test]
async fn test_submit_without_compilation_rejects_unsupported_gates() {
    let addr = start_strict_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "strict".to_string(),
            shots: 1024,
            optimization_level: 0, // No compilation
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;

    // Poll until terminal state
    let mut failed = false;
    for _ in 0..20 {
        let response = client
            .get_job_status(Request::new(GetJobStatusRequest {
                job_id: job_id.clone(),
            }))
            .await
            .unwrap();

        let job = response.into_inner().job.unwrap();
        let state = JobState::try_from(job.state).unwrap();

        if state == JobState::Failed {
            failed = true;
            assert!(
                job.error_message.contains("Unsupported gate") || job.error_message.contains('h'),
                "Error should mention unsupported gate 'h', got: {}",
                job.error_message
            );
            break;
        }
        assert!(
            state != JobState::Completed,
            "Job should have failed — strict backend should reject the h gate"
        );

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    assert!(
        failed,
        "Job with unsupported gates should fail on strict backend"
    );
}

/// Submit to simulator without setting optimization_level (default=0).
/// Backwards compatible — simulator accepts all gates, so it should work.
#[tokio::test]
async fn test_submit_default_backwards_compatible() {
    let addr = start_test_server().await;
    let mut client = ArvakServiceClient::connect(addr).await.unwrap();

    let response = client
        .submit_job(Request::new(SubmitJobRequest {
            circuit: Some(CircuitPayload {
                format: Some(circuit_payload::Format::Qasm3(TEST_QASM.to_string())),
            }),
            backend_id: "simulator".to_string(),
            shots: 100,
            optimization_level: 0, // Explicit default — no compilation
        }))
        .await
        .unwrap();

    let job_id = response.into_inner().job_id;
    assert!(!job_id.is_empty());

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

    assert!(
        completed,
        "Default submission to simulator should still work"
    );
}
