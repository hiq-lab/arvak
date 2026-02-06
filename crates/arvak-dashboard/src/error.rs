//! Error types for the dashboard API.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// API error type that converts to HTTP responses.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Compilation error: {0}")]
    CompileError(String),

    #[error("Backend error: {0}")]
    BackendError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::ParseError(_) => (StatusCode::BAD_REQUEST, "parse_error"),
            ApiError::CompileError(_) => (StatusCode::BAD_REQUEST, "compile_error"),
            ApiError::BackendError(_) => (StatusCode::BAD_GATEWAY, "backend_error"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let body = Json(ErrorResponse {
            error: error_type.to_string(),
            message: self.to_string(),
        });

        (status, body).into_response()
    }
}

impl From<hiq_qasm3::ParseError> for ApiError {
    fn from(e: hiq_qasm3::ParseError) -> Self {
        ApiError::ParseError(e.to_string())
    }
}

impl From<hiq_compile::CompileError> for ApiError {
    fn from(e: hiq_compile::CompileError) -> Self {
        ApiError::CompileError(e.to_string())
    }
}

impl From<hiq_hal::HalError> for ApiError {
    fn from(e: hiq_hal::HalError) -> Self {
        ApiError::BackendError(e.to_string())
    }
}
