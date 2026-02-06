//! Arvak gRPC server binary.
//!
//! This binary starts the Arvak gRPC service with full configuration support,
//! metrics, health checks, and distributed tracing.
//!
//! # Configuration
//!
//! The server can be configured via:
//! 1. Configuration file: `--config path/to/config.yaml`
//! 2. Environment variables: `ARVAK_*`
//! 3. .env file in working directory
//!
//! Environment variables override configuration file settings.
//!
//! # Usage
//!
//! ```bash
//! # Use default configuration
//! arvak-grpc-server
//!
//! # Use configuration file
//! arvak-grpc-server --config config.yaml
//!
//! # Override with environment variables
//! ARVAK_GRPC_ADDRESS=127.0.0.1:9090 arvak-grpc-server
//! ```

use arvak_grpc::{
    init_tracing, start_health_server, ArvakServiceImpl, Config, HealthState, Metrics,
    TracingConfig, TracingFormat,
};
use arvak_grpc::proto::arvak_service_server::ArvakServiceServer;
use tonic::transport::Server;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let config_file = parse_config_arg(&args);

    // Load configuration
    let config = Config::load(config_file.as_deref())?;

    // Initialize tracing/logging based on configuration
    let tracing_config = TracingConfig {
        log_level: config.observability.logging.level.clone(),
        format: match config.observability.logging.format.as_str() {
            "json" => TracingFormat::Json,
            _ => TracingFormat::Console,
        },
        service_name: config.observability.tracing.service_name.clone(),
        otlp_endpoint: if config.observability.tracing.enabled {
            config.observability.tracing.otlp_endpoint.clone()
        } else {
            None
        },
    };

    init_tracing(tracing_config)?;

    info!("Starting Arvak gRPC server");
    info!(
        "Configuration loaded from {}",
        config_file
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("defaults")
    );

    // Create service with resource limits
    use arvak_grpc::server::{JobStore, backend_registry::create_default_registry};
    let service = ArvakServiceImpl::with_limits(
        JobStore::new(),
        create_default_registry(),
        config.limits.clone(),
    );
    let backend_registry = service.backends();

    // Start HTTP server for health checks and metrics (if enabled)
    if config.observability.http_server.health_enabled
        || config.observability.http_server.metrics_enabled
    {
        let http_addr = config.http_address()?;
        let health_state = HealthState {
            backends: backend_registry.clone(),
            metrics: Metrics,
        };

        info!("Starting HTTP server on {}", http_addr);
        if config.observability.http_server.health_enabled {
            info!("  Health endpoints: /health, /health/ready");
        }
        if config.observability.http_server.metrics_enabled {
            info!("  Metrics endpoint: /metrics");
        }

        let health_handle = tokio::spawn(async move {
            if let Err(e) = start_health_server(http_addr.port(), health_state).await {
                error!("HTTP server error: {}", e);
            }
        });

        // Store handle to keep health server running
        std::mem::forget(health_handle);
    }

    // Get gRPC server address
    let grpc_addr = config.grpc_address()?;

    info!("gRPC server listening on {}", grpc_addr);
    info!("Storage backend: {}", config.storage.backend);
    info!(
        "Resource limits: {} concurrent jobs, {} queued",
        config.limits.max_concurrent_jobs, config.limits.max_queued_jobs
    );

    // Build and start gRPC server
    let server = Server::builder()
        .timeout(std::time::Duration::from_secs(config.server.timeout_seconds))
        .tcp_keepalive(Some(std::time::Duration::from_secs(
            config.server.keepalive_seconds,
        )))
        .add_service(ArvakServiceServer::new(service))
        .serve(grpc_addr);

    info!("Arvak gRPC server started successfully");

    // Run server
    server.await?;

    Ok(())
}

/// Parse --config argument from command line.
fn parse_config_arg(args: &[String]) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == "--config" || args[i] == "-c" {
            if i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
        }
    }
    None
}
