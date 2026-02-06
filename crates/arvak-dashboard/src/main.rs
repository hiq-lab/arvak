//! HIQ Dashboard binary entry point.

use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hiq_dashboard::{AppState, DashboardConfig, create_router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hiq_dashboard=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Create configuration
    let config = DashboardConfig::default();
    let bind_addr = config.bind_address;

    // Create application state
    let state = Arc::new(AppState::with_config(config));

    // Optionally register the simulator backend if the feature is enabled
    #[cfg(feature = "with-simulator")]
    {
        use hiq_adapter_sim::SimulatorBackend;
        let sim = Arc::new(SimulatorBackend::new());
        state.register_backend(sim).await;
        tracing::info!("Registered simulator backend");
    }

    // Create the router
    let app = create_router(state);

    // Start the server
    tracing::info!("Starting HIQ Dashboard at http://{}", bind_addr);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
