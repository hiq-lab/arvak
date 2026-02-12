//! Integration tests for the Arvak Dashboard API.

use std::sync::Arc;

use arvak_adapter_sim::SimulatorBackend;
use arvak_dashboard::{AppState, DashboardConfig, create_router};
use arvak_sched::SqliteStore;
use axum_test::TestServer;
use serde_json::{Value, json};

// ============================================================================
// Test helpers
// ============================================================================

fn test_state() -> Arc<AppState> {
    Arc::new(AppState::with_config(DashboardConfig::default()))
}

fn test_state_with_store() -> Arc<AppState> {
    let store = SqliteStore::in_memory().expect("sqlite in-memory");
    Arc::new(
        AppState::with_config(DashboardConfig::default()).with_store(Arc::new(store)),
    )
}

fn test_server(state: Arc<AppState>) -> TestServer {
    let router = create_router(state);
    TestServer::new(router).expect("test server")
}

const BELL_QASM: &str =
    "OPENQASM 3.0; qubit[2] q; bit[2] c; h q[0]; cx q[0], q[1]; c[0] = measure q[0]; c[1] = measure q[1];";

const SIMPLE_QASM: &str = "OPENQASM 3.0; qubit[2] q; h q[0]; cx q[0], q[1];";

// ============================================================================
// Health endpoint
// ============================================================================

#[tokio::test]
async fn test_health_returns_ok() {
    let server = test_server(test_state());
    let response = server.get("/api/health").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["status"], "ok");
    assert!(body["version"].as_str().is_some());
}

// ============================================================================
// Circuit visualization
// ============================================================================

#[tokio::test]
async fn test_visualize_bell_circuit() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/visualize")
        .json(&json!({ "qasm": BELL_QASM }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["num_qubits"], 2);
    assert_eq!(body["num_clbits"], 2);
    assert!(body["depth"].as_u64().unwrap() > 0);
    assert!(body["layers"].as_array().is_some());
}

#[tokio::test]
async fn test_visualize_invalid_qasm_returns_400() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/visualize")
        .json(&json!({ "qasm": "not valid qasm at all" }))
        .await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);

    let body: Value = response.json();
    assert_eq!(body["error"], "parse_error");
}

#[tokio::test]
async fn test_visualize_empty_body_returns_422() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/visualize")
        .json(&json!({}))
        .await;
    // Missing required field "qasm" → 422 Unprocessable Entity (axum deserialization)
    assert!(response.status_code().is_client_error());
}

// ============================================================================
// Circuit compilation
// ============================================================================

#[tokio::test]
async fn test_compile_for_simulator_target() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "simulator",
            "optimization_level": 1
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert!(body["before"]["num_qubits"].as_u64().is_some());
    assert!(body["after"]["num_qubits"].as_u64().is_some());
    assert!(body["compiled_qasm"].as_str().is_some());
    assert!(body["stats"]["compile_time_us"].as_u64().is_some());
    assert!(body["topology"].is_object());
    assert_eq!(body["topology"]["kind"], "fully_connected");
}

#[tokio::test]
async fn test_compile_for_star_topology() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "star"
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["topology"]["kind"], "star");
}

#[tokio::test]
async fn test_compile_for_linear_topology() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "linear"
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["topology"]["kind"], "linear");
}

#[tokio::test]
async fn test_compile_unknown_target_returns_400() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "nonexistent_quantum_computer"
        }))
        .await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);

    let body: Value = response.json();
    assert_eq!(body["error"], "bad_request");
}

#[tokio::test]
async fn test_compile_invalid_qasm_returns_400() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": "garbage input",
            "target": "iqm"
        }))
        .await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);

    let body: Value = response.json();
    assert_eq!(body["error"], "parse_error");
}

#[tokio::test]
async fn test_compile_returns_esp_data() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "simulator",
            "optimization_level": 1
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    // ESP should exist for non-empty circuits
    if let Some(esp) = body.get("esp") {
        assert!(esp["total_esp"].as_f64().unwrap() > 0.0);
        assert!(esp["total_esp"].as_f64().unwrap() <= 1.0);
    }
}

#[tokio::test]
async fn test_compile_returns_stats() {
    let server = test_server(test_state());
    let response = server
        .post("/api/circuits/compile")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "simulator",
            "optimization_level": 1
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    let stats = &body["stats"];
    assert!(stats["original_depth"].as_u64().is_some());
    assert!(stats["compiled_depth"].as_u64().is_some());
    assert!(stats["gates_before"].as_u64().is_some());
    assert!(stats["gates_after"].as_u64().is_some());
}

#[tokio::test]
async fn test_compile_optimization_levels() {
    let server = test_server(test_state());

    for level in [0, 1, 2, 3] {
        let response = server
            .post("/api/circuits/compile")
            .json(&json!({
                "qasm": SIMPLE_QASM,
                "target": "simulator",
                "optimization_level": level
            }))
            .await;
        response.assert_status_ok();
    }
}

// ============================================================================
// Backend endpoints
// ============================================================================

#[tokio::test]
async fn test_list_backends_empty() {
    let server = test_server(test_state());
    let response = server.get("/api/backends").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_backends_with_simulator() {
    let state = test_state();
    let sim = Arc::new(SimulatorBackend::new());
    state.register_backend(sim).await;

    let server = test_server(state);
    let response = server.get("/api/backends").await;
    response.assert_status_ok();

    let body: Value = response.json();
    let backends = body.as_array().unwrap();
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0]["name"], "simulator");
    assert_eq!(backends[0]["is_simulator"], true);
    assert!(backends[0]["num_qubits"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_get_backend_details() {
    let state = test_state();
    let sim = Arc::new(SimulatorBackend::new());
    state.register_backend(sim).await;

    let server = test_server(state);
    let response = server.get("/api/backends/simulator").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["name"], "simulator");
    assert_eq!(body["is_simulator"], true);
    assert!(body["gate_set"].is_object());
    assert!(body["topology"].is_object());
}

#[tokio::test]
async fn test_get_unknown_backend_returns_404() {
    let server = test_server(test_state());
    let response = server.get("/api/backends/nonexistent").await;
    response.assert_status(axum::http::StatusCode::NOT_FOUND);

    let body: Value = response.json();
    assert_eq!(body["error"], "not_found");
}

// ============================================================================
// Job management endpoints
// ============================================================================

#[tokio::test]
async fn test_list_jobs_empty() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server.get("/api/jobs").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_jobs_without_store() {
    // Without a store, should return empty array
    let server = test_server(test_state());
    let response = server.get("/api/jobs").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_create_job() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .post("/api/jobs")
        .json(&json!({
            "name": "test_bell",
            "qasm": BELL_QASM,
            "shots": 1024
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["name"], "test_bell");
    assert_eq!(body["shots"], 1024);
    assert!(body["id"].as_str().is_some());
    assert!(!body["status"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_job_with_backend() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .post("/api/jobs")
        .json(&json!({
            "name": "test_job",
            "qasm": BELL_QASM,
            "backend": "simulator"
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["backend"], "simulator");
}

#[tokio::test]
async fn test_create_job_invalid_qasm() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .post("/api/jobs")
        .json(&json!({
            "name": "bad_job",
            "qasm": "not valid qasm"
        }))
        .await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_and_get_job() {
    let state = test_state_with_store();
    let server = test_server(state);

    // Create
    let create_resp = server
        .post("/api/jobs")
        .json(&json!({
            "name": "roundtrip_test",
            "qasm": BELL_QASM
        }))
        .await;
    create_resp.assert_status_ok();
    let job_id = create_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Get
    let get_resp = server.get(&format!("/api/jobs/{job_id}")).await;
    get_resp.assert_status_ok();

    let body: Value = get_resp.json();
    assert_eq!(body["name"], "roundtrip_test");
    assert_eq!(body["id"], job_id);
    assert!(body["qasm"].as_str().is_some());
}

#[tokio::test]
async fn test_get_nonexistent_job() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .get("/api/jobs/00000000-0000-0000-0000-000000000000")
        .await;
    response.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_job_invalid_id() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server.get("/api/jobs/not-a-uuid").await;
    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_delete_job() {
    let state = test_state_with_store();
    let server = test_server(state);

    // Create
    let create_resp = server
        .post("/api/jobs")
        .json(&json!({
            "name": "to_delete",
            "qasm": BELL_QASM
        }))
        .await;
    let job_id = create_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete
    let del_resp = server.delete(&format!("/api/jobs/{job_id}")).await;
    del_resp.assert_status_ok();

    let body: Value = del_resp.json();
    assert_eq!(body["deleted"], true);

    // Verify gone
    let get_resp = server.get(&format!("/api/jobs/{job_id}")).await;
    get_resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_nonexistent_job() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .delete("/api/jobs/00000000-0000-0000-0000-000000000000")
        .await;
    response.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_job_result_before_completion() {
    let state = test_state_with_store();
    let server = test_server(state);

    // Create a pending job
    let create_resp = server
        .post("/api/jobs")
        .json(&json!({
            "name": "pending_job",
            "qasm": BELL_QASM
        }))
        .await;
    let job_id = create_resp.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Try to get result — should fail since job is pending
    let result_resp = server.get(&format!("/api/jobs/{job_id}/result")).await;
    response_is_client_error(&result_resp);
}

#[tokio::test]
async fn test_create_job_default_shots() {
    let state = test_state_with_store();
    let server = test_server(state);

    let response = server
        .post("/api/jobs")
        .json(&json!({
            "name": "defaults_test",
            "qasm": BELL_QASM
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body["shots"], 1024); // default
    assert_eq!(body["priority"], 100); // default
}

#[tokio::test]
async fn test_list_jobs_after_create() {
    let state = test_state_with_store();
    let server = test_server(state);

    // Create two jobs
    server
        .post("/api/jobs")
        .json(&json!({ "name": "job1", "qasm": BELL_QASM }))
        .await;
    server
        .post("/api/jobs")
        .json(&json!({ "name": "job2", "qasm": BELL_QASM }))
        .await;

    let response = server.get("/api/jobs").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert_eq!(body.as_array().unwrap().len(), 2);
}

// ============================================================================
// VQE demo endpoint
// ============================================================================

#[tokio::test]
async fn test_vqe_demo() {
    let server = test_server(test_state());
    let response = server.get("/api/vqe/demo").await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert!(body.is_object());
}

// ============================================================================
// Eval endpoint
// ============================================================================

#[tokio::test]
async fn test_eval_basic() {
    let server = test_server(test_state());
    let response = server
        .post("/api/eval")
        .json(&json!({
            "qasm": SIMPLE_QASM,
            "target": "simulator",
            "optimization_level": 1,
            "target_qubits": 5
        }))
        .await;
    response.assert_status_ok();

    let body: Value = response.json();
    assert!(body["schema_version"].as_str().is_some());
    assert!(body["input"]["num_qubits"].as_u64().is_some());
    assert!(body["compilation"]["num_passes"].as_u64().is_some());
}

#[tokio::test]
async fn test_eval_invalid_qasm() {
    let server = test_server(test_state());
    let response = server
        .post("/api/eval")
        .json(&json!({
            "qasm": "bad qasm",
            "target": "iqm"
        }))
        .await;
    assert!(response.status_code().is_client_error() || response.status_code().is_server_error());
}

// ============================================================================
// Static file serving
// ============================================================================

#[tokio::test]
async fn test_index_html() {
    let server = test_server(test_state());
    let response = server.get("/").await;
    response.assert_status_ok();
}

#[tokio::test]
async fn test_app_js() {
    let server = test_server(test_state());
    let response = server.get("/app.js").await;
    response.assert_status_ok();
}

#[tokio::test]
async fn test_style_css() {
    let server = test_server(test_state());
    let response = server.get("/style.css").await;
    response.assert_status_ok();
}

#[tokio::test]
async fn test_spa_fallback() {
    let server = test_server(test_state());
    // Any unknown path should serve index.html (SPA fallback)
    let response = server.get("/some/unknown/path").await;
    response.assert_status_ok();
}

// ============================================================================
// Error response format
// ============================================================================

#[tokio::test]
async fn test_error_response_format() {
    let server = test_server(test_state());
    let response = server.get("/api/backends/nonexistent").await;

    let body: Value = response.json();
    // All errors should have "error" and "message" fields
    assert!(body["error"].as_str().is_some());
    assert!(body["message"].as_str().is_some());
}

// ============================================================================
// Helper
// ============================================================================

fn response_is_client_error(response: &axum_test::TestResponse) {
    let status = response.status_code().as_u16();
    assert!(
        (400..500).contains(&status),
        "Expected 4xx status, got {status}"
    );
}
