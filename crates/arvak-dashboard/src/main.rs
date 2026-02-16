//! Arvak Dashboard binary entry point.

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use arvak_dashboard::{AppState, DashboardConfig, create_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "arvak_dashboard=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create configuration
    let mut config = DashboardConfig::default();
    if let Ok(bind) = std::env::var("ARVAK_BIND") {
        config.bind_address = bind
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid ARVAK_BIND address '{bind}': {e}"))?;
    }
    let bind_addr = config.bind_address;

    // Create job store (in-memory SQLite)
    let store = Arc::new(
        arvak_sched::SqliteStore::in_memory().expect("Failed to create in-memory job store"),
    );
    tracing::info!("Initialized in-memory job store");

    // Create application state
    let state = Arc::new(AppState::with_config(config).with_store(store));

    // Optionally register the simulator backend if the feature is enabled
    #[cfg(feature = "with-simulator")]
    {
        use arvak_adapter_sim::SimulatorBackend;
        let sim = Arc::new(SimulatorBackend::new());
        state.register_backend(sim).await;
        tracing::info!("Registered simulator backend");
    }

    // Start background job processor
    let processor_state = state.clone();
    tokio::spawn(async move {
        arvak_dashboard::processor::run_job_processor(processor_state).await;
    });

    // Create the router
    let app = create_router(state);

    // Start the server
    tracing::info!("Starting Arvak Dashboard at http://{}", bind_addr);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
