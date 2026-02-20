//! Circuit parsing and compilation utilities shared across gRPC service modules.

use crate::error::Error;
use crate::error::Result;
use crate::proto::{CircuitPayload, circuit_payload};
use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_hal::backend::Backend;
use arvak_hal::capability::{Capabilities, TopologyKind};
use arvak_ir::circuit::Circuit;

/// Parse circuit from protobuf payload (static version for use in async contexts).
pub(crate) fn parse_circuit_static(payload: Option<CircuitPayload>) -> Result<Circuit> {
    let payload =
        payload.ok_or_else(|| Error::InvalidCircuit("Missing circuit payload".to_string()))?;

    match payload.format {
        Some(circuit_payload::Format::Qasm3(qasm)) => {
            let circuit = arvak_qasm3::parse(&qasm)?;
            Ok(circuit)
        }
        Some(circuit_payload::Format::ArvakIrJson(_json)) => Err(Error::InvalidCircuit(
            "Arvak IR JSON format not yet supported. Use OpenQASM 3 format instead.".to_string(),
        )),
        None => Err(Error::InvalidCircuit(
            "No circuit format specified".to_string(),
        )),
    }
}

/// Compile a circuit for a specific backend's capabilities.
///
/// If `optimization_level` is 0, returns the circuit unchanged (backwards compatible).
/// Levels 1-3 enable compilation with the corresponding optimization level.
pub(crate) async fn compile_for_backend(
    circuit: Circuit,
    backend: &dyn Backend,
    optimization_level: u32,
) -> std::result::Result<Circuit, tonic::Status> {
    if optimization_level == 0 {
        return Ok(circuit);
    }

    let caps = backend.capabilities();
    let coupling_map = build_coupling_map(caps);
    let basis_gates = build_basis_gates(caps);

    let level = u8::try_from(optimization_level.min(3)).unwrap_or(3);

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(level)
        .with_target(coupling_map, basis_gates)
        .build();

    let mut dag = circuit.into_dag();

    // Run compilation on blocking thread (CPU-bound work per CLAUDE.md rules)
    let (dag_result, _props) = tokio::task::spawn_blocking(move || {
        let result = pm.run(&mut dag, &mut props);
        (result.map(|()| dag), props)
    })
    .await
    .map_err(|e| tonic::Status::internal(format!("Compilation task failed: {e}")))?;

    let dag = dag_result
        .map_err(|e| tonic::Status::internal(format!("Circuit compilation failed: {e}")))?;

    Ok(Circuit::from_dag(dag))
}

/// Build a [`CouplingMap`] from backend capabilities.
fn build_coupling_map(caps: &Capabilities) -> CouplingMap {
    match &caps.topology.kind {
        TopologyKind::Linear => CouplingMap::linear(caps.num_qubits),
        TopologyKind::Star => CouplingMap::star(caps.num_qubits),
        TopologyKind::FullyConnected => CouplingMap::full(caps.num_qubits),
        TopologyKind::NeutralAtom { zones } => CouplingMap::zoned(caps.num_qubits, *zones),
        // Grid, Custom, and any future variants: build from edge list
        _ => {
            let mut map = CouplingMap::new(caps.num_qubits);
            for &(a, b) in &caps.topology.edges {
                map.add_edge(a, b);
            }
            map.rebuild_caches();
            map
        }
    }
}

/// Build [`BasisGates`] from backend capabilities.
///
/// Uses the `native` gate list when non-empty (hardware backends), so the
/// compiler decomposes non-native gates (e.g. `h` → `rz·sx·rz` on IBM Heron).
/// Falls back to all supported gates for simulators (empty `native` list).
fn build_basis_gates(caps: &Capabilities) -> BasisGates {
    let mut gates: Vec<String> = if caps.gate_set.native.is_empty() {
        // Simulator: all supported gates are native — no decomposition needed.
        let mut g: Vec<String> = caps.gate_set.single_qubit.iter().cloned().collect();
        g.extend(caps.gate_set.two_qubit.iter().cloned());
        g.extend(caps.gate_set.three_qubit.iter().cloned());
        g
    } else {
        // Hardware: compile only to truly native gates; non-native gates get
        // decomposed by the BasisTranslation pass.
        caps.gate_set.native.iter().cloned().collect()
    };
    // Always include measurement and barrier
    if !gates.iter().any(|g| g == "measure") {
        gates.push("measure".to_string());
    }
    if !gates.iter().any(|g| g == "barrier") {
        gates.push("barrier".to_string());
    }
    BasisGates::new(gates)
}
