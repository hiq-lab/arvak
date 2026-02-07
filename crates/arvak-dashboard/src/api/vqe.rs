//! VQE demo result endpoint.

use axum::Json;

/// Embedded VQE result from `results/vqe_result.json`.
const VQE_RESULT: &str = include_str!("../../../../results/vqe_result.json");

/// GET /api/vqe/demo — return the pre-computed VQE H₂ ground-state result.
pub async fn vqe_demo() -> Json<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(VQE_RESULT).expect("embedded VQE JSON is valid");
    Json(value)
}
