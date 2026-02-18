//! Arvak REST gateway binary.
//!
//! Exposes the Arvak compilation and execution pipeline over HTTP/JSON.
//! Designed to sit behind an nginx reverse proxy that terminates TLS.
//!
//! # Configuration (environment variables)
//!
//! - `ARVAK_API_KEY`       — Bearer token for authentication (optional but recommended)
//! - `ARVAK_CORS_ORIGINS`  — Comma-separated allowed origins, or `*` (default `*`)
//! - `ARVAK_REST_ADDRESS`  — Listen address (default `127.0.0.1:8080`)
//! - `ARVAK_LOG_LEVEL`     — Tracing filter (default `info`)
//!
//! # Usage
//!
//! ```bash
//! ARVAK_API_KEY=secret ARVAK_REST_ADDRESS=127.0.0.1:8080 arvak-rest-gateway
//! ```

use arvak_grpc::rest::{AppState, auth::AuthState, rest_router};
use arvak_grpc::server::JobStore;
use arvak_grpc::server::backend_registry::create_default_registry;
use arvak_grpc::{Metrics, init_default_tracing};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env if present
    let _ = dotenvy::dotenv();

    // Initialize tracing
    init_default_tracing()?;

    info!("Starting Arvak REST gateway");

    // Read configuration from environment
    let api_key = std::env::var("ARVAK_API_KEY").ok();
    let cors_origins = std::env::var("ARVAK_CORS_ORIGINS").unwrap_or_else(|_| "*".to_string());
    let listen_addr =
        std::env::var("ARVAK_REST_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    if api_key.is_some() {
        info!("API key authentication enabled");
    } else {
        warn!(
            "No API key configured (ARVAK_API_KEY). Server is unauthenticated. \
             Deploy behind an authenticating reverse proxy."
        );
    }

    // Build shared state — reuses the same components as the gRPC server
    let job_store = Arc::new(JobStore::new());
    let backends = Arc::new(create_default_registry());
    let metrics = Metrics::new();

    for backend_id in backends.list() {
        metrics.set_backend_available(&backend_id, true);
    }

    let state = AppState {
        job_store,
        backends,
        metrics,
        resources: None,
        abort_handles: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        auth: AuthState {
            api_key: api_key.map(Arc::new),
        },
    };

    let app = rest_router(state, &cors_origins);

    // Graceful shutdown
    let shutdown = Arc::new(Notify::new());
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_clone.notify_one();
    });

    let addr: std::net::SocketAddr = listen_addr.parse()?;
    info!("REST gateway listening on {addr}");
    info!("CORS origins: {cors_origins}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.notified().await;
            info!("Shutdown signal received");
        })
        .await?;

    info!("REST gateway shut down");
    Ok(())
}

async fn shutdown_signal() {
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
        () = ctrl_c => info!("Received SIGINT"),
        () = terminate => info!("Received SIGTERM"),
    }
}
