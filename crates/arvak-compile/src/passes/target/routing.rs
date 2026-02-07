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
    fn name(&self) -> &str {
        "BasicRouting"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let coupling_map = properties
            .coupling_map
            .as_ref()
            .ok_or(CompileError::MissingCouplingMap)?;

        let layout = properties
            .layout
            .as_mut()
            .ok_or(CompileError::MissingLayout)?;

        // Collect operations that need routing
        let ops: Vec<_> = dag
            .topological_ops()
            .map(|(idx, inst)| (idx, inst.clone()))
            .collect();

        for (_node_idx, instruction) in ops {
            // Only process two-qubit gates
            if instruction.qubits.len() != 2 {
                continue;
            }

            let q0 = instruction.qubits[0];
            let q1 = instruction.qubits[1];

            let p0 = layout.get_physical(q0).ok_or(CompileError::MissingLayout)?;
            let p1 = layout.get_physical(q1).ok_or(CompileError::MissingLayout)?;

            // Check if qubits are connected
            if coupling_map.is_connected(p0, p1) {
                continue;
            }

            // Need to insert SWAPs to bring qubits together
            // Use a simple greedy approach: find path and insert SWAPs
            let path = find_path(coupling_map, p0, p1)?;

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

/// Find shortest path between two physical qubits.
fn find_path(
    coupling_map: &crate::property::CouplingMap,
    from: u32,
    to: u32,
) -> CompileResult<Vec<u32>> {
    use std::collections::{HashMap, VecDeque};

    if from == to {
        return Ok(vec![from]);
    }

    let mut visited = HashMap::new();
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
    fn test_find_path() {
        let coupling_map = CouplingMap::linear(5);

        let path = find_path(&coupling_map, 0, 4).unwrap();
        assert_eq!(path, vec![0, 1, 2, 3, 4]);

        let path = find_path(&coupling_map, 2, 2).unwrap();
        assert_eq!(path, vec![2]);
    }
}
