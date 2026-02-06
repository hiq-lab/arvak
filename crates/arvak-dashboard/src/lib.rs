//! HIQ Dashboard - Local web interface for quantum circuit visualization and monitoring.
//!
//! This crate provides a web-based dashboard for the HIQ quantum compilation platform.
//! It enables researchers to:
//!
//! - Visualize quantum circuits (before and after compilation)
//! - Monitor backend status and capabilities
//! - Track job execution (Phase 2)
//! - Analyze execution results (Phase 3)
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use hiq_dashboard::{AppState, DashboardConfig, create_router};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = DashboardConfig::default();
//!     let state = Arc::new(AppState::with_config(config.clone()));
//!
//!     let app = create_router(state);
//!     let listener = tokio::net::TcpListener::bind(config.bind_address).await.unwrap();
//!     axum::serve(listener, app).await.unwrap();
//! }
//! ```

pub mod api;
pub mod dto;
pub mod error;
pub mod server;
pub mod state;
pub mod ws;

pub use dto::{
    BackendDetails, BackendSummary, CircuitVisualization, CompilationStats, CompileRequest,
    CompileResponse, HealthResponse, VisualizeRequest,
};
pub use error::ApiError;
pub use server::create_router;
pub use state::{AppState, DashboardConfig};
