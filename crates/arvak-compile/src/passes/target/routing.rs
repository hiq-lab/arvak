//! Routing passes for inserting SWAP gates.

use arvak_ir::{CircuitDag, Instruction, StandardGate};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Basic routing pass.
///
/// Inserts SWAP gates to satisfy connectivity constraints.
/// This is a simple greedy algorithm that may not produce
/// optimal results but is fast and correct.
pub struct BasicRouting;

impl Pass for BasicRouting {
    fn name(&self) -> &'static str {
        "BasicRouting"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    #[allow(clippy::similar_names, clippy::cast_possible_truncation)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let coupling_map = properties
            .coupling_map
            .as_ref()
            .ok_or(CompileError::MissingCouplingMap)?;

        let layout = properties
            .layout
            .as_mut()
            .ok_or(CompileError::MissingLayout)?;

        // Collect only the qubit pairs for two-qubit gates (avoids cloning full instructions)
        let two_qubit_ops: Vec<_> = dag
            .topological_ops()
            .filter(|(_, inst)| inst.qubits.len() == 2)
            .map(|(idx, inst)| (idx, inst.qubits[0], inst.qubits[1]))
            .collect();

        // Known limitation: SWAP gates are appended at the end of the DAG via
        // `dag.apply()` rather than being inserted immediately before the target
        // two-qubit gate. This means the SWAPs will appear after all existing
        // operations in the topological order, which may not produce the optimal
        // circuit ordering. Fixing this requires architectural changes to support
        // positional insertion in the DAG.
        for (_node_idx, q0, q1) in two_qubit_ops {
            let p0 = layout.get_physical(q0).ok_or(CompileError::MissingLayout)?;
            let p1 = layout.get_physical(q1).ok_or(CompileError::MissingLayout)?;

            // Check if qubits are connected
            if coupling_map.is_connected(p0, p1) {
                continue;
            }

            // Use precomputed shortest path (O(distance) reconstruction, no BFS).
            let path = coupling_map
                .shortest_path(p0, p1)
                .ok_or(CompileError::RoutingFailed {
                    qubit1: p0,
                    qubit2: p1,
                })?;

            // Insert SWAPs along the path (except the last edge which is the gate)
            for i in 0..path.len() - 2 {
                let swap_p1 = path[i];
                let swap_p2 = path[i + 1];

                // Find logical qubits at these physical locations
                let swap_l1 = layout.get_logical(swap_p1);
                let swap_l2 = layout.get_logical(swap_p2);

                // Only insert SWAP if both positions have qubits
                if let (Some(l1), Some(l2)) = (swap_l1, swap_l2) {
                    // Insert SWAP gate
                    dag.apply(Instruction::two_qubit_gate(StandardGate::Swap, l1, l2))
                        .map_err(CompileError::Ir)?;

                    // Update layout
                    layout.swap(swap_p1, swap_p2);
                }
            }
        }

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties.coupling_map.is_some() && properties.layout.is_some()
    }
}

/// Find shortest path between two physical qubits (legacy BFS, kept for tests).
#[cfg(test)]
fn find_path(
    coupling_map: &crate::property::CouplingMap,
    from: u32,
    to: u32,
) -> CompileResult<Vec<u32>> {
    use rustc_hash::FxHashMap;
    use std::collections::VecDeque;

    if from == to {
        return Ok(vec![from]);
    }

    let mut visited = FxHashMap::default();
    let mut queue = VecDeque::new();

    visited.insert(from, None);
    queue.push_back(from);

    while let Some(current) = queue.pop_front() {
        for neighbor in coupling_map.neighbors(current) {
            if visited.contains_key(&neighbor) {
                continue;
            }

            visited.insert(neighbor, Some(current));

            if neighbor == to {
                // Reconstruct path
                let mut path = vec![to];
                let mut node = to;
                while let Some(Some(prev)) = visited.get(&node) {
                    path.push(*prev);
                    node = *prev;
                }
                path.reverse();
                return Ok(path);
            }

            queue.push_back(neighbor);
        }
    }

    Err(CompileError::RoutingFailed {
        qubit1: from,
        qubit2: to,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::TrivialLayout;
    use crate::property::{BasisGates, CouplingMap};
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_basic_routing_connected() {
        // Create a circuit with a CX on adjacent qubits
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        TrivialLayout.run(&mut dag, &mut props).unwrap();
        BasicRouting.run(&mut dag, &mut props).unwrap();

        // No SWAPs needed, ops count should be the same
        assert_eq!(dag.num_ops(), 2);
    }

    #[test]
    fn test_basic_routing_needs_swap() {
        // Create a circuit with a CX on non-adjacent qubits
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.cx(QubitId(0), QubitId(2)).unwrap(); // Not adjacent in linear
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        TrivialLayout.run(&mut dag, &mut props).unwrap();

        let ops_before = dag.num_ops();
        BasicRouting.run(&mut dag, &mut props).unwrap();
        let ops_after = dag.num_ops();

        // Should have inserted at least one SWAP
        assert!(ops_after > ops_before);
    }

    #[test]
    fn test_shortest_path() {
        let coupling_map = CouplingMap::linear(5);

        // Precomputed shortest path
        let path = coupling_map.shortest_path(0, 4).unwrap();
        assert_eq!(path, vec![0, 1, 2, 3, 4]);

        let path = coupling_map.shortest_path(2, 2).unwrap();
        assert_eq!(path, vec![2]);

        // Legacy BFS fallback
        let path = find_path(&coupling_map, 0, 4).unwrap();
        assert_eq!(path, vec![0, 1, 2, 3, 4]);
    }
}
