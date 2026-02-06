//! Circuit visualization and compilation endpoints.

use std::sync::Arc;

use axum::{Json, extract::State};
use hiq_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use hiq_ir::Circuit;

use crate::dto::{
    CircuitVisualization, CompilationStats, CompileRequest, CompileResponse, VisualizeRequest,
};
use crate::error::ApiError;
use crate::state::AppState;

/// POST /api/circuits/visualize - Parse QASM3 and return visualization data.
pub async fn visualize(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<VisualizeRequest>,
) -> Result<Json<CircuitVisualization>, ApiError> {
    // Parse the QASM3 source
    let circuit = hiq_qasm3::parse(&req.qasm)?;

    // Convert to visualization format
    let visualization = CircuitVisualization::from_circuit(&circuit);

    Ok(Json(visualization))
}

/// POST /api/circuits/compile - Compile circuit for target and return before/after comparison.
pub async fn compile(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<CompileRequest>,
) -> Result<Json<CompileResponse>, ApiError> {
    // Parse the QASM3 source
    let circuit = hiq_qasm3::parse(&req.qasm)?;

    // Get original stats and visualization
    let before = CircuitVisualization::from_circuit(&circuit);
    let original_depth = circuit.depth();
    let gates_before = count_gates(&circuit);

    // Determine target coupling map and basis gates
    let (coupling_map, basis_gates) = get_target_config(&req.target, circuit.num_qubits())?;

    // Build pass manager
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(req.optimization_level)
        .with_target(coupling_map, basis_gates)
        .build();

    // Convert to DAG and run compilation
    let mut dag = circuit.into_dag();
    pm.run(&mut dag, &mut props)?;

    // Convert back to circuit
    let compiled = Circuit::from_dag(dag);

    // Get compiled stats and visualization
    let after = CircuitVisualization::from_circuit(&compiled);
    let compiled_depth = compiled.depth();
    let gates_after = count_gates(&compiled);

    // Emit compiled QASM
    let compiled_qasm = hiq_qasm3::emit(&compiled)?;

    Ok(Json(CompileResponse {
        before,
        after,
        compiled_qasm,
        stats: CompilationStats {
            original_depth,
            compiled_depth,
            gates_before,
            gates_after,
        },
    }))
}

/// Get coupling map and basis gates for a target backend.
fn get_target_config(
    target: &str,
    num_qubits: usize,
) -> Result<(CouplingMap, BasisGates), ApiError> {
    match target.to_lowercase().as_str() {
        "iqm" | "iqm5" => {
            // IQM 5-qubit star topology
            Ok((
                CouplingMap::star(5.max(num_qubits as u32)),
                BasisGates::iqm(),
            ))
        }
        "iqm20" => {
            // IQM 20-qubit device (simplified as star for now)
            Ok((CouplingMap::star(20), BasisGates::iqm()))
        }
        "ibm" | "ibm5" => {
            // IBM 5-qubit linear topology
            Ok((
                CouplingMap::linear(5.max(num_qubits as u32)),
                BasisGates::ibm(),
            ))
        }
        "ibm27" => {
            // IBM 27-qubit device (simplified as linear)
            Ok((CouplingMap::linear(27), BasisGates::ibm()))
        }
        "simulator" | "sim" => {
            // Simulator - fully connected, universal gates
            Ok((
                CouplingMap::full(num_qubits as u32),
                BasisGates::universal(),
            ))
        }
        "linear" => {
            // Generic linear topology
            Ok((
                CouplingMap::linear(num_qubits as u32),
                BasisGates::universal(),
            ))
        }
        "star" => {
            // Generic star topology
            Ok((
                CouplingMap::star(num_qubits as u32),
                BasisGates::universal(),
            ))
        }
        _ => Err(ApiError::BadRequest(format!(
            "Unknown target '{}'. Supported targets: iqm, iqm5, iqm20, ibm, ibm5, ibm27, simulator, linear, star",
            target
        ))),
    }
}

/// Count the number of gate operations in a circuit.
fn count_gates(circuit: &Circuit) -> usize {
    circuit.dag().num_ops()
}
