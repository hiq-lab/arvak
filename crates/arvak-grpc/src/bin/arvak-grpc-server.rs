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
//!
//! # Graceful Shutdown
//!
//! The server responds to SIGTERM and SIGINT signals for graceful shutdown:
//! - Stops accepting new requests
//! - Waits for in-flight requests to complete (with timeout)
//! - Shuts down gRPC and HTTP servers cleanly

use arvak_grpc::{
    init_tracing, start_health_server, ArvakServiceImpl, Config, HealthState, Metrics,
    TracingConfig, TracingFormat,
};
use arvak_grpc::proto::arvak_service_server::ArvakServiceServer;
use arvak_grpc::server::{RequestIdInterceptor, TimingLayer};
use std::sync::Arc;
use tokio::sync::Notify;
use tonic::transport::Server;
use tower::ServiceBuilder;
use tracing::{error, info, warn};

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
        config_file.as_deref()
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

    // Set up graceful shutdown
    let shutdown_signal = Arc::new(Notify::new());
    let shutdown_signal_clone = shutdown_signal.clone();

    // Spawn signal handler
    tokio::spawn(async move {
        shutdown_signal_handler().await;
        shutdown_signal_clone.notify_one();
    });

    // Start HTTP server for health checks and metrics (if enabled)
    let http_shutdown = shutdown_signal.clone();
    let http_handle = if config.observability.http_server.health_enabled
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

        // HTTP server doesn't have graceful shutdown built-in, just let it run
        let handle = tokio::spawn(async move {
            if let Err(e) = start_health_server(http_addr.port(), health_state).await {
                error!("HTTP server error: {}", e);
            }
            // Wait for shutdown signal
            http_shutdown.notified().await;
            info!("HTTP server shutting down");
        });

        Some(handle)
    } else {
        None
    };

    // Get gRPC server address
    let grpc_addr = config.grpc_address()?;
    let shutdown_timeout = config.server.shutdown_timeout_seconds;

    info!("gRPC server listening on {}", grpc_addr);
    info!("Storage backend: {}", config.storage.backend);
    info!(
        "Resource limits: {} concurrent jobs, {} queued",
        config.limits.max_concurrent_jobs, config.limits.max_queued_jobs
    );
    info!("Graceful shutdown timeout: {}s", shutdown_timeout);

    // Build gRPC server with middleware and interceptors
    let service_with_interceptor = ArvakServiceServer::with_interceptor(
        service,
        RequestIdInterceptor::new(),
    );

    // Enable gRPC reflection for tools like grpcurl
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(arvak_grpc::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let server = Server::builder()
        .timeout(std::time::Duration::from_secs(config.server.timeout_seconds))
        .tcp_keepalive(Some(std::time::Duration::from_secs(
            config.server.keepalive_seconds,
        )))
        .layer(
            ServiceBuilder::new()
                .layer(TimingLayer::new())
                .into_inner(),
        )
        .add_service(reflection_service)
        .add_service(service_with_interceptor)
        .serve_with_shutdown(grpc_addr, async move {
            shutdown_signal.notified().await;
            info!("Shutdown signal received, initiating graceful shutdown");
        });

    info!("Arvak gRPC server started successfully");

    // Run gRPC server
    let result = tokio::time::timeout(
        std::time::Duration::MAX, // No timeout on normal operation
        server,
    )
    .await;

    match result {
        Ok(Ok(())) => {
            info!("gRPC server shut down successfully");
        }
        Ok(Err(e)) => {
            error!("gRPC server error: {}", e);
        }
        Err(_) => {
            warn!("gRPC server shutdown timed out");
        }
    }

    // Wait for HTTP server to shut down (with timeout)
    if let Some(handle) = http_handle {
        info!("Waiting for HTTP server to shut down");
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(shutdown_timeout),
            handle,
        )
        .await;
    }

    info!("Server shutdown complete");
    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT).
async fn shutdown_signal_handler() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C)");
        }
        _ = terminate => {
            info!("Received SIGTERM");
        }
    }
}

/// Parse --config argument from command line.
fn parse_config_arg(args: &[String]) -> Option<String> {
    for i in 0..args.len() {
        if (args[i] == "--config" || args[i] == "-c")
            && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
    }
    None
}
