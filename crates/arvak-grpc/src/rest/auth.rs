//! Bearer token authentication middleware for the REST gateway.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Shared authentication state.
#[derive(Clone)]
pub struct AuthState {
    /// Expected API key. `None` means authentication is disabled.
    pub api_key: Option<Arc<String>>,
}

/// Constant-time string comparison to prevent timing side-channel attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Axum middleware that validates `Authorization: Bearer <token>`.
pub async fn bearer_auth(request: Request, next: Next) -> Response {
    let auth_state = request.extensions().get::<AuthState>().cloned();

    let expected_key = match auth_state.as_ref().and_then(|s| s.api_key.as_ref()) {
        Some(key) => key,
        None => return next.run(request).await, // auth disabled
    };

    let header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let provided = match header.and_then(|h| h.strip_prefix("Bearer ")) {
        Some(token) if !token.is_empty() => token,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(super::types::ErrorResponse {
                    error: "Missing or malformed Authorization header".to_string(),
                    code: 401,
                }),
            )
                .into_response();
        }
    };

    if constant_time_eq(provided.as_bytes(), expected_key.as_bytes()) {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(super::types::ErrorResponse {
                error: "Invalid API key".to_string(),
                code: 401,
            }),
        )
            .into_response()
    }
}
