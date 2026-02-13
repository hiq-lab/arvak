//! VQE demo result endpoint.

use axum::Json;

use crate::error::ApiError;

/// Embedded VQE result from `results/vqe_result.json`.
const VQE_RESULT: &str = include_str!("../../../../demos/data/vqe_result.json");

/// GET /api/vqe/demo — return the pre-computed VQE H₂ ground-state result.
pub async fn vqe_demo() -> Result<Json<serde_json::Value>, ApiError> {
    let value: serde_json::Value = serde_json::from_str(VQE_RESULT)
        .map_err(|e| ApiError::Internal(format!("embedded VQE JSON parse error: {e}")))?;
    Ok(Json(value))
}
