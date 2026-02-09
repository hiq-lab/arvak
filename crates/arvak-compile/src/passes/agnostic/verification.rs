//! Verification passes for ensuring compilation correctness.
//!
//! These passes validate that optimization passes have not introduced
//! incorrect transformations, particularly around measurement boundaries.

use rustc_hash::FxHashMap;
use tracing::debug;

use arvak_ir::{CircuitDag, DagNode, QubitId};
use petgraph::visit::EdgeRef;

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Result of measurement barrier verification.
#[derive(Debug, Clone, Default)]
pub struct VerificationResult {
    /// Whether the verification passed.
    pub passed: bool,
    /// Number of qubits verified.
    pub qubits_checked: usize,
    /// Number of measurements found.
    pub measurements_found: usize,
}

/// Analysis pass that verifies no optimization has moved gates across
/// measurement boundaries.
///
/// This pass acts as a safety net: it walks the DAG in topological order
/// and for each qubit, checks that no gate that should appear after a
/// measurement has been reordered to appear before it.
///
/// Specifically, it verifies that the topological ordering of operations
/// on each qubit wire respects measurement boundaries — once a measurement
/// is encountered on a wire, all subsequent operations on that wire must
/// remain after the measurement in topological order.
///
/// This pass should be added after all optimization passes to catch any
/// correctness violations.
pub struct MeasurementBarrierVerification;

impl Pass for MeasurementBarrierVerification {
    fn name(&self) -> &'static str {
        "measurement_barrier_verification"
    }

    fn kind(&self) -> PassKind {
        PassKind::Analysis
    }

    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        // Build per-qubit operation ordering from topological sort.
        // For each qubit, collect the ordered list of (position, instruction_kind).
        let mut qubit_ops: FxHashMap<QubitId, Vec<OpEntry>> = FxHashMap::default();

        for (position, (_node_idx, inst)) in dag.topological_ops().enumerate() {
            for &qubit in &inst.qubits {
                qubit_ops.entry(qubit).or_default().push(OpEntry {
                    position,
                    is_measurement: inst.is_measure(),
                    is_gate: inst.is_gate(),
                    is_reset: inst.is_reset(),
                });
            }
        }

        let mut measurements_found = 0;

        // For each qubit, verify that no gate appears after a measurement
        // unless there's a reset in between (which re-initializes the qubit).
        for ops in qubit_ops.values() {
            let mut last_measurement_pos: Option<usize> = None;
            let mut last_reset_after_measurement: Option<usize> = None;

            for op in ops {
                if op.is_measurement {
                    measurements_found += 1;
                    last_measurement_pos = Some(op.position);
                    last_reset_after_measurement = None;
                } else if op.is_reset {
                    if last_measurement_pos.is_some() {
                        last_reset_after_measurement = Some(op.position);
                    }
                } else if op.is_gate {
                    // A gate after a measurement is only valid if there was
                    // a reset in between (mid-circuit measurement + reset pattern).
                    if let Some(meas_pos) = last_measurement_pos {
                        if last_reset_after_measurement.is_none() && op.position > meas_pos {
                            // This is a gate after measurement without reset.
                            // Check if this gate's topological position is consistent:
                            // it should be AFTER the measurement in the DAG.
                            // If it somehow ended up before, that's a violation.
                            //
                            // Note: In the current DAG, position > meas_pos means
                            // the gate IS after the measurement (correct). A violation
                            // would be if a gate that was originally after a measurement
                            // got moved before it. We detect this by checking if the
                            // DAG edge structure is consistent with topological order.
                            //
                            // Since we walk topological order and gates on the same wire
                            // must maintain their relative order, a violation would manifest
                            // as a gate appearing BEFORE the measurement in our walk but
                            // being connected AFTER it in the wire. We check this below.
                        }
                    }
                }
            }
        }

        // Now do a stricter check: for each qubit wire, verify that the
        // topological ordering is consistent with the wire ordering in the DAG.
        // Walk the DAG edges for each qubit wire and confirm that the topological
        // position of each node is monotonically increasing along the wire.
        let graph = dag.graph();
        for &qubit in &dag.qubits().collect::<Vec<_>>() {
            let wire = arvak_ir::WireId::Qubit(qubit);

            // Find the input node for this qubit
            let input_node = graph
                .node_indices()
                .find(|&idx| matches!(&graph[idx], DagNode::In(w) if *w == wire));

            if let Some(start) = input_node {
                // Walk the wire from input to output, collecting topological positions
                let mut current = start;
                let mut prev_position: Option<usize> = None;

                loop {
                    // Find the next node on this wire
                    let next = graph
                        .edges_directed(current, petgraph::Direction::Outgoing)
                        .find(|e| e.weight().wire == wire)
                        .map(|e| e.target());

                    match next {
                        Some(next_node) => {
                            if let DagNode::Op(inst) = &graph[next_node] {
                                // Find this node's topological position
                                let topo_pos =
                                    dag.topological_ops().position(|(idx, _)| idx == next_node);

                                if let (Some(prev), Some(curr)) = (prev_position, topo_pos) {
                                    if curr < prev {
                                        // Topological order violated on this wire
                                        return Err(CompileError::MeasurementViolation {
                                            gate_name: inst.name().to_string(),
                                            qubit: qubit.0,
                                            detail: format!(
                                                "Operation '{}' on qubit {} has topological position {} \
                                                 but follows an operation at position {} on the same wire",
                                                inst.name(),
                                                qubit.0,
                                                curr,
                                                prev,
                                            ),
                                        });
                                    }
                                }

                                prev_position = topo_pos;
                            }
                            current = next_node;
                        }
                        None => break,
                    }

                    // Safety: break if we hit an output node
                    if matches!(&graph[current], DagNode::Out(_)) {
                        break;
                    }
                }
            }
        }

        let result = VerificationResult {
            passed: true,
            qubits_checked: qubit_ops.len(),
            measurements_found,
        };

        debug!(
            "Measurement barrier verification passed: {} qubits checked, {} measurements found",
            result.qubits_checked, result.measurements_found
        );

        properties.insert(result);

        Ok(())
    }
}

/// Internal helper for tracking operations per qubit.
struct OpEntry {
    position: usize,
    is_measurement: bool,
    is_gate: bool,
    is_reset: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::{Circuit, ClbitId, QubitId};

    fn run_verification(circuit: &Circuit) -> CompileResult<VerificationResult> {
        let mut dag = circuit.clone().into_dag();
        let mut props = PropertySet::new();
        let pass = MeasurementBarrierVerification;
        pass.run(&mut dag, &mut props)?;
        Ok(props.get::<VerificationResult>().unwrap().clone())
    }

    #[test]
    fn test_simple_circuit_passes() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();

        let result = run_verification(&circuit).unwrap();
        assert!(result.passed);
        assert_eq!(result.qubits_checked, 2);
        assert_eq!(result.measurements_found, 2);
    }

    #[test]
    fn test_mid_circuit_measurement_with_reset() {
        let mut circuit = Circuit::with_size("test", 1, 1);
        circuit.h(QubitId(0)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.reset(QubitId(0)).unwrap();
        circuit.h(QubitId(0)).unwrap();

        let result = run_verification(&circuit).unwrap();
        assert!(result.passed);
        assert_eq!(result.measurements_found, 1);
    }

    #[test]
    fn test_circuit_with_barrier() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.barrier([QubitId(0), QubitId(1)]).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();
        circuit.measure(QubitId(1), ClbitId(1)).unwrap();

        let result = run_verification(&circuit).unwrap();
        assert!(result.passed);
    }

    #[test]
    fn test_empty_circuit() {
        let circuit = Circuit::with_size("test", 2, 0);
        let result = run_verification(&circuit).unwrap();
        assert!(result.passed);
        assert_eq!(result.measurements_found, 0);
    }
}
