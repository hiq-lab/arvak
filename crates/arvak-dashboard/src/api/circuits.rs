//! Circuit visualization and compilation endpoints.

use std::sync::Arc;
use std::time::Instant;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_ir::Circuit;
use axum::{Json, extract::State};

use crate::dto::{
    CircuitVisualization, CompilationStats, CompileRequest, CompileResponse, EspData,
    QubitMapEntry, QubitMapping, TopologyView, VisualizeRequest,
};
use crate::error::ApiError;
use crate::state::AppState;

/// POST /api/circuits/visualize - Parse QASM3 and return visualization data.
pub async fn visualize(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<VisualizeRequest>,
) -> Result<Json<CircuitVisualization>, ApiError> {
    // Parse the QASM3 source
    let circuit = arvak_qasm3::parse(&req.qasm)?;

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
    let circuit = arvak_qasm3::parse(&req.qasm)?;

    // Get original stats and visualization
    let before = CircuitVisualization::from_circuit(&circuit);
    let original_depth = circuit.depth();
    let gates_before = count_gates(&circuit);

    // Determine target coupling map and basis gates
    let (coupling_map, basis_gates) = get_target_config(&req.target, circuit.num_qubits())?;

    // Build topology view before moving coupling_map into the pass manager
    let topology = Some(TopologyView {
        kind: topology_kind(&req.target),
        edges: coupling_map.edges().to_vec(),
        num_qubits: coupling_map.num_qubits(),
    });

    // Build pass manager
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(req.optimization_level)
        .with_target(coupling_map, basis_gates)
        .build();

    // Convert to DAG and run compilation
    let mut dag = circuit.into_dag();
    let compile_start = Instant::now();
    pm.run(&mut dag, &mut props)?;
    let compile_time = compile_start.elapsed();

    // Extract qubit mapping from layout (set by layout pass)
    let qubit_mapping = props.layout.as_ref().map(|layout| {
        let mut mappings: Vec<QubitMapEntry> = layout
            .iter()
            .map(|(logical, physical)| QubitMapEntry {
                logical: logical.0,
                physical,
            })
            .collect();
        mappings.sort_by_key(|e| e.logical);
        QubitMapping { mappings }
    });

    // Convert back to circuit
    let compiled = Circuit::from_dag(dag);

    // Get compiled stats and visualization
    let after = CircuitVisualization::from_circuit(&compiled);
    let compiled_depth = compiled.depth();
    let gates_after = count_gates(&compiled);

    // Compute ESP from compiled circuit layers
    let esp = compute_esp(&after);

    // Compute throughput
    let compile_time_us = compile_time.as_micros() as u64;
    let throughput_gates_per_sec = if compile_time.as_nanos() > 0 {
        (gates_after as f64 / compile_time.as_secs_f64()) as u64
    } else {
        0
    };

    // Emit compiled QASM
    let compiled_qasm = arvak_qasm3::emit(&compiled)?;

    Ok(Json(CompileResponse {
        before,
        after,
        compiled_qasm,
        stats: CompilationStats {
            original_depth,
            compiled_depth,
            gates_before,
            gates_after,
            compile_time_us,
            throughput_gates_per_sec,
        },
        qubit_mapping,
        esp,
        topology,
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
            "Unknown target '{target}'. Supported targets: iqm, iqm5, iqm20, ibm, ibm5, ibm27, simulator, linear, star"
        ))),
    }
}

/// Count the number of gate operations in a circuit.
fn count_gates(circuit: &Circuit) -> usize {
    circuit.dag().num_ops()
}

/// Determine the topology kind string for a target backend.
fn topology_kind(target: &str) -> String {
    match target.to_lowercase().as_str() {
        "iqm" | "iqm5" | "iqm20" | "star" => "star".to_string(),
        "ibm" | "ibm5" | "ibm27" | "linear" => "linear".to_string(),
        "simulator" | "sim" => "fully_connected".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Compute Estimated Success Probability from compiled circuit layers.
///
/// Uses a simple error model: 1-qubit gate fidelity = 0.999, 2-qubit gate fidelity = 0.99.
/// Measurements and barriers do not contribute to error.
fn compute_esp(circuit: &CircuitVisualization) -> Option<EspData> {
    if circuit.layers.is_empty() {
        return None;
    }

    const FIDELITY_1Q: f64 = 0.999;
    const FIDELITY_2Q: f64 = 0.99;

    let mut layer_esp = Vec::with_capacity(circuit.layers.len());
    let mut cumulative_esp = Vec::with_capacity(circuit.layers.len());
    let mut running = 1.0_f64;

    for layer in &circuit.layers {
        let mut layer_fidelity = 1.0_f64;
        for op in &layer.operations {
            if op.is_measurement || op.is_barrier {
                continue;
            }
            let gate_fidelity = if op.num_qubits >= 2 {
                FIDELITY_2Q
            } else {
                FIDELITY_1Q
            };
            layer_fidelity *= gate_fidelity;
        }
        layer_esp.push(layer_fidelity);
        running *= layer_fidelity;
        cumulative_esp.push(running);
    }

    Some(EspData {
        layer_esp,
        cumulative_esp,
        total_esp: running,
    })
}
