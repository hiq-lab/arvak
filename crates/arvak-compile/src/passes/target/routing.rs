//! Routing passes for inserting SWAP gates.

use arvak_ir::{CircuitDag, Instruction, QubitId, StandardGate};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Basic routing pass.
///
/// Inserts SWAP gates to satisfy connectivity constraints.
/// Rebuilds the DAG from scratch in topological order, inserting
/// SWAP chains immediately before each non-adjacent two-qubit gate.
///
/// The output DAG uses **physical** qubit wire labels: every instruction's
/// qubit operands are remapped from logical IDs to physical positions via
/// the current layout. SWAP gates use physical wire labels directly so
/// the emitted circuit is ready for hardware execution.
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

        // Collect all instructions in topological order from the original DAG
        let ops: Vec<Instruction> = dag
            .topological_ops()
            .map(|(_, inst)| inst.clone())
            .collect();

        // Build a new DAG with physical qubit wires.
        // Each mapped logical qubit gets a wire labelled by its physical position.
        let mut new_dag = CircuitDag::new();
        for (_, physical) in layout.iter() {
            new_dag.add_qubit(QubitId(physical));
        }
        for clbit in dag.clbits().collect::<Vec<_>>() {
            new_dag.add_clbit(clbit);
        }

        for inst in ops {
            if inst.qubits.len() == 2 {
                let q0 = inst.qubits[0];
                let q1 = inst.qubits[1];
                let p0 = layout.get_physical(q0).ok_or(CompileError::MissingLayout)?;
                let p1 = layout.get_physical(q1).ok_or(CompileError::MissingLayout)?;

                if !coupling_map.is_connected(p0, p1) {
                    let path =
                        coupling_map
                            .shortest_path(p0, p1)
                            .ok_or(CompileError::RoutingFailed {
                                qubit1: p0,
                                qubit2: p1,
                            })?;

                    // Insert SWAPs along the path (except the last edge which is the gate).
                    // SWAPs use physical wire labels so they operate on the correct
                    // hardware qubits.
                    for i in 0..path.len() - 2 {
                        let swap_p1 = path[i];
                        let swap_p2 = path[i + 1];

                        // Ensure physical wires exist in the new DAG
                        new_dag.add_qubit(QubitId(swap_p1));
                        new_dag.add_qubit(QubitId(swap_p2));

                        new_dag
                            .apply(Instruction::two_qubit_gate(
                                StandardGate::Swap,
                                QubitId(swap_p1),
                                QubitId(swap_p2),
                            ))
                            .map_err(CompileError::Ir)?;
                        layout.swap(swap_p1, swap_p2);
                    }
                }
            }

            // Remap instruction qubits from logical to physical positions.
            let mut remapped = inst;
            remapped.qubits = remapped
                .qubits
                .iter()
                .map(|&q| {
                    let p = layout.get_physical(q).ok_or(CompileError::MissingLayout)?;
                    // Ensure physical wire exists (handles ancilla paths)
                    new_dag.add_qubit(QubitId(p));
                    Ok(QubitId(p))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            new_dag.apply(remapped).map_err(CompileError::Ir)?;
        }

        new_dag.set_global_phase(dag.global_phase());
        new_dag.set_level(dag.level());
        *dag = new_dag;

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
    use crate::property::{BasisGates, CouplingMap, Layout};
    use arvak_ir::{Circuit, InstructionKind, QubitId};

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
        // Create a circuit with a CX on non-adjacent qubits (q0, q2)
        // On linear(5): 0-1-2-3-4, so q0 and q2 are distance 2 apart.
        // The router should insert a SWAP before the CX.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        TrivialLayout.run(&mut dag, &mut props).unwrap();
        BasicRouting.run(&mut dag, &mut props).unwrap();

        // Should have inserted at least one SWAP
        assert!(dag.num_ops() > 1);

        // Verify SWAPs appear BEFORE the CX in topological order
        let ops: Vec<_> = dag
            .topological_ops()
            .filter_map(|(_, inst)| {
                if let InstructionKind::Gate(gate) = &inst.kind {
                    Some(gate.name().to_string())
                } else {
                    None
                }
            })
            .collect();

        // Find last SWAP and first CX — SWAPs must come before CX
        let last_swap = ops.iter().rposition(|name| name == "swap");
        let first_cx = ops.iter().position(|name| name == "cx");
        assert!(last_swap.is_some(), "expected at least one SWAP gate");
        assert!(first_cx.is_some(), "expected CX gate in output");
        assert!(
            last_swap.unwrap() < first_cx.unwrap(),
            "SWAP gates must appear before CX in topological order, got ops: {ops:?}"
        );

        // Verify all two-qubit gates use adjacent physical qubits.
        // Since the output uses physical wire labels, QubitId values
        // are physical positions — check adjacency directly.
        let coupling_map = props.coupling_map.as_ref().unwrap();
        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                assert!(
                    coupling_map.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "two-qubit gate on non-adjacent physical qubits ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0
                );
            }
        }
    }

    #[test]
    fn test_routing_bv_pattern() {
        // BV-style circuit: CX from q0 to q3 on linear(5)
        // Path: 0-1-2-3, needs 2 SWAPs to bring q0 adjacent to q3
        let mut circuit = Circuit::with_size("bv_test", 4, 0);
        circuit.cx(QubitId(0), QubitId(3)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        TrivialLayout.run(&mut dag, &mut props).unwrap();
        BasicRouting.run(&mut dag, &mut props).unwrap();

        // Since the output uses physical wire labels, check adjacency directly.
        let coupling_map = props.coupling_map.as_ref().unwrap();
        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                assert!(
                    coupling_map.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "two-qubit gate on non-adjacent physical qubits ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0
                );
            }
        }
    }

    #[test]
    fn test_routing_with_ancilla() {
        // Sparse coupling map: 0-1, 1-2, 2-3 (linear 4-qubit)
        // Circuit uses only q0 and q1 (mapped to physical 0 and 3, distance 3).
        // Path 0→1→2→3 traverses physical qubits 1 and 2 which have no
        // logical qubits mapped — the router must add physical wires.
        let mut circuit = Circuit::with_size("ancilla_test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        // Manually set up a layout where logical q0→physical 0, q1→physical 3
        // (skipping physical 1 and 2 — no logical qubits there)
        let mut props = PropertySet::new().with_target(CouplingMap::linear(4), BasisGates::iqm());
        let mut layout = Layout::new();
        layout.add(QubitId(0), 0);
        layout.add(QubitId(1), 3);
        props.layout = Some(layout);

        BasicRouting.run(&mut dag, &mut props).unwrap();

        // Should have added physical wires for intermediary qubits 1 and 2
        assert!(
            dag.num_qubits() > 2,
            "expected physical wires for SWAP path, got {} qubits",
            dag.num_qubits()
        );

        // Verify SWAPs were inserted
        let ops: Vec<_> = dag
            .topological_ops()
            .filter_map(|(_, inst)| {
                if let InstructionKind::Gate(gate) = &inst.kind {
                    Some(gate.name().to_string())
                } else {
                    None
                }
            })
            .collect();
        let swap_count = ops.iter().filter(|n| *n == "swap").count();
        assert!(
            swap_count >= 2,
            "expected at least 2 SWAPs, got {swap_count}"
        );
        assert!(ops.iter().any(|n| n == "cx"), "expected CX gate in output");

        // Verify all two-qubit gates are on adjacent physical qubits
        let coupling_map = props.coupling_map.as_ref().unwrap();
        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                assert!(
                    coupling_map.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "two-qubit gate on non-adjacent physical qubits ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0
                );
            }
        }
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
