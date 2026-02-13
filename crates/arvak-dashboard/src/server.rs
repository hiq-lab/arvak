//! Axum server setup and routing.

use std::sync::Arc;

use axum::{
    Router,
    http::{StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use crate::api;
use crate::state::AppState;

// Embed static files at compile time
const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const STYLE_CSS: &str = include_str!("../static/style.css");

/// Create the Axum router with all routes.
pub fn create_router(state: Arc<AppState>) -> Router {
    // API routes
    let api_routes = Router::new()
        .route("/health", get(api::health::health))
        .route("/circuits/visualize", post(api::circuits::visualize))
        .route("/circuits/compile", post(api::circuits::compile))
        .route("/backends", get(api::backends::list_backends))
        .route("/backends/{name}", get(api::backends::get_backend))
        // Job management routes
        .route(
            "/jobs",
            get(api::jobs::list_jobs).post(api::jobs::create_job),
        )
        .route(
            "/jobs/{id}",
            get(api::jobs::get_job).delete(api::jobs::delete_job),
        )
        .route("/jobs/{id}/result", get(api::jobs::get_job_result))
        .route("/vqe/demo", get(api::vqe::vqe_demo))
        // Evaluator route
        .route("/eval", post(api::eval::evaluate));

    // Static file routes
    let static_routes = Router::new()
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        .route("/app.js", get(serve_app_js))
        .route("/style.css", get(serve_style_css));

    // Combine all routes
    Router::new()
        .nest("/api", api_routes)
        .merge(static_routes)
        .fallback(serve_index) // SPA fallback
        .layer(CompressionLayer::new())
        // TODO: Make CORS configurable; restrict origins in production
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// Static file handlers

async fn serve_index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn serve_app_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript")],
        APP_JS,
    )
}

async fn serve_style_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css")],
        STYLE_CSS,
    )
}
