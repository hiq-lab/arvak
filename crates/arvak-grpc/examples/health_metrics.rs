//! Example demonstrating health check and metrics endpoints.
//!
//! This example starts both the gRPC server and HTTP health/metrics server,
//! then shows how to query the health endpoints.
//!
//! Run with: cargo run --example health_metrics
//!
//! Then in another terminal:
//! - curl http://localhost:9090/health
//! - curl http://localhost:9090/health/ready
//! - curl http://localhost:9090/metrics

use arvak_grpc::{health, ArvakServiceImpl, HealthState, Metrics};
use arvak_grpc::proto::arvak_service_server::ArvakServiceServer;
use arvak_grpc::server::backend_registry::create_default_registry;
use std::sync::Arc;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Arvak gRPC Health & Metrics Example ===\n");

    // Create service components
    let service = ArvakServiceImpl::new();
    let backends = Arc::new(create_default_registry());
    let metrics = Metrics::new();

    // Initialize backend availability metrics
    for backend_id in backends.list() {
        metrics.set_backend_available(&backend_id, true);
        println!("Registered backend: {}", backend_id);
    }

    // Create health state
    let health_state = HealthState::new(backends.clone(), metrics.clone());

    // Start HTTP health/metrics server on port 9090
    let health_server = tokio::spawn(async move {
        health::start_health_server(9090, health_state)
            .await
            .expect("Failed to start health server");
    });

    println!("\nHTTP health/metrics server started on http://0.0.0.0:9090");
    println!("Available endpoints:");
    println!("  - GET http://localhost:9090/health");
    println!("  - GET http://localhost:9090/health/ready");
    println!("  - GET http://localhost:9090/metrics");

    // Start gRPC server on port 50051
    let grpc_addr = "0.0.0.0:50051".parse()?;
    println!("\ngRPC server started on {}", grpc_addr);

    println!("\n=== Server Running ===");
    println!("Try these commands in another terminal:");
    println!("  curl http://localhost:9090/health");
    println!("  curl http://localhost:9090/health/ready");
    println!("  curl http://localhost:9090/metrics");
    println!("\nPress Ctrl+C to stop");

    let grpc_server = Server::builder()
        .add_service(ArvakServiceServer::new(service))
        .serve(grpc_addr);

    // Run both servers
    tokio::select! {
        result = grpc_server => {
            if let Err(e) = result {
                eprintln!("gRPC server error: {}", e);
            }
        }
        result = health_server => {
            if let Err(e) = result {
                eprintln!("Health server error: {:?}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down servers...");
        }
    }

    Ok(())
}
