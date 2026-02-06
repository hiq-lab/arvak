//! Arvak gRPC server binary.

use arvak_grpc::server::ArvakServiceImpl;
use arvak_grpc::proto::arvak_service_server::ArvakServiceServer;
use tonic::transport::Server;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Create service
    let service = ArvakServiceImpl::new();

    // Parse address from environment or use default
    let addr = std::env::var("ARVAK_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
        .parse()?;

    tracing::info!("Arvak gRPC server listening on {}", addr);

    // Start server
    Server::builder()
        .add_service(ArvakServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
