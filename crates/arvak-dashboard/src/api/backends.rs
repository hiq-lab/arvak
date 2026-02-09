//! Backend status and capabilities endpoints.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};

use crate::dto::{BackendDetails, BackendSummary, GateSetView, TopologyView};
use crate::error::ApiError;
use crate::state::AppState;

/// GET /api/backends - List all configured backends.
pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BackendSummary>>, ApiError> {
    let backends = state.backends.read().await;
    let mut summaries = Vec::with_capacity(backends.len());

    for (name, backend) in backends.iter() {
        let available = backend.is_available().await.unwrap_or(false);
        let capabilities = backend.capabilities().await.ok();

        summaries.push(BackendSummary {
            name: name.clone(),
            is_simulator: capabilities.as_ref().is_some_and(|c| c.is_simulator),
            num_qubits: capabilities.as_ref().map_or(0, |c| c.num_qubits),
            available,
            native_gates: capabilities
                .as_ref()
                .map(|c| c.gate_set.native.clone())
                .unwrap_or_default(),
        });
    }

    Ok(Json(summaries))
}

/// GET /api/backends/:name - Get detailed backend information.
pub async fn get_backend(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<BackendDetails>, ApiError> {
    let backends = state.backends.read().await;
    let backend = backends
        .get(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Backend '{name}' not found")))?;

    let available = backend.is_available().await.unwrap_or(false);
    let capabilities = backend
        .capabilities()
        .await
        .map_err(|e| ApiError::BackendError(e.to_string()))?;

    let topology_kind = match &capabilities.topology.kind {
        arvak_hal::TopologyKind::FullyConnected => "fully_connected",
        arvak_hal::TopologyKind::Linear => "linear",
        arvak_hal::TopologyKind::Star => "star",
        arvak_hal::TopologyKind::Grid { .. } => "grid",
        arvak_hal::TopologyKind::Custom => "custom",
        _ => "unknown",
    };

    Ok(Json(BackendDetails {
        name: name.clone(),
        is_simulator: capabilities.is_simulator,
        num_qubits: capabilities.num_qubits,
        max_shots: capabilities.max_shots,
        available,
        gate_set: GateSetView {
            single_qubit: capabilities.gate_set.single_qubit.clone(),
            two_qubit: capabilities.gate_set.two_qubit.clone(),
            native: capabilities.gate_set.native.clone(),
        },
        topology: TopologyView {
            kind: topology_kind.to_string(),
            edges: capabilities.topology.edges.clone(),
            num_qubits: capabilities.num_qubits,
        },
    }))
}
