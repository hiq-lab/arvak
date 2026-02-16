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

    // Build CORS layer: use ARVAK_CORS_ORIGIN env var in production,
    // fall back to permissive for local development.
    let cors = match std::env::var("ARVAK_CORS_ORIGIN") {
        Ok(origin) if origin == "*" => CorsLayer::permissive(),
        Ok(origin) => match origin.parse::<axum::http::HeaderValue>() {
            Ok(hv) => CorsLayer::new()
                .allow_origin(hv)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::DELETE,
                ])
                .allow_headers(Any),
            Err(e) => {
                tracing::warn!(
                    "Invalid ARVAK_CORS_ORIGIN '{origin}': {e}; falling back to localhost-only CORS"
                );
                CorsLayer::new()
                    .allow_origin([
                        "http://localhost:3000".parse().unwrap(),
                        "http://127.0.0.1:3000".parse().unwrap(),
                    ])
                    .allow_methods([
                        axum::http::Method::GET,
                        axum::http::Method::POST,
                        axum::http::Method::DELETE,
                    ])
                    .allow_headers(Any)
            }
        },
        Err(_) => {
            tracing::warn!("ARVAK_CORS_ORIGIN not set. Defaulting to localhost-only CORS.");
            CorsLayer::new()
                .allow_origin([
                    "http://localhost:3000".parse().unwrap(),
                    "http://127.0.0.1:3000".parse().unwrap(),
                ])
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::DELETE,
                ])
                .allow_headers(Any)
        }
    };

    // Combine all routes
    Router::new()
        .nest("/api", api_routes)
        .merge(static_routes)
        .fallback(serve_index) // SPA fallback
        .layer(CompressionLayer::new())
        .layer(cors)
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
