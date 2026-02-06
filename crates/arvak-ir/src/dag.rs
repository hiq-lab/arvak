//! DAG-based circuit representation.

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex as PetNodeIndex};
use petgraph::visit::EdgeRef;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::error::{IrError, IrResult};
use crate::instruction::{Instruction, InstructionKind};
use crate::qubit::{ClbitId, QubitId};

/// Node index type for the circuit DAG.
pub type NodeIndex = PetNodeIndex<u32>;

/// A node in the circuit DAG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DagNode {
    /// Input node for a wire.
    In(WireId),
    /// Output node for a wire.
    Out(WireId),
    /// Operation node containing an instruction.
    Op(Instruction),
}

impl DagNode {
    /// Check if this is an input node.
    pub fn is_input(&self) -> bool {
        matches!(self, DagNode::In(_))
    }

    /// Check if this is an output node.
    pub fn is_output(&self) -> bool {
        matches!(self, DagNode::Out(_))
    }

    /// Check if this is an operation node.
    pub fn is_op(&self) -> bool {
        matches!(self, DagNode::Op(_))
    }

    /// Get the instruction if this is an operation node.
    pub fn instruction(&self) -> Option<&Instruction> {
        match self {
            DagNode::Op(inst) => Some(inst),
            _ => None,
        }
    }

    /// Get mutable reference to the instruction.
    pub fn instruction_mut(&mut self) -> Option<&mut Instruction> {
        match self {
            DagNode::Op(inst) => Some(inst),
            _ => None,
        }
    }
}

/// Identifier for a wire in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WireId {
    /// A quantum wire.
    Qubit(QubitId),
    /// A classical wire.
    Clbit(ClbitId),
}

impl From<QubitId> for WireId {
    fn from(q: QubitId) -> Self {
        WireId::Qubit(q)
    }
}

impl From<ClbitId> for WireId {
    fn from(c: ClbitId) -> Self {
        WireId::Clbit(c)
    }
}

/// An edge in the circuit DAG representing a wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DagEdge {
    /// The wire this edge represents.
    pub wire: WireId,
}

/// DAG-based circuit representation.
///
/// The circuit is represented as a directed acyclic graph where:
/// - Nodes are either input nodes, output nodes, or operation nodes
/// - Edges represent wires (quantum or classical)
/// - Each wire has exactly one input and one output node
/// - Operations are connected to wires in topological order
pub struct CircuitDag {
    /// The underlying graph.
    graph: DiGraph<DagNode, DagEdge, u32>,
    /// Map from qubit to its input node.
    qubit_inputs: FxHashMap<QubitId, NodeIndex>,
    /// Map from qubit to its current output node.
    qubit_outputs: FxHashMap<QubitId, NodeIndex>,
    /// Map from classical bit to its input node.
    clbit_inputs: FxHashMap<ClbitId, NodeIndex>,
    /// Map from classical bit to its current output node.
    clbit_outputs: FxHashMap<ClbitId, NodeIndex>,
    /// Global phase of the circuit.
    global_phase: f64,
}

impl CircuitDag {
    /// Create a new empty circuit DAG.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::default(),
            qubit_inputs: FxHashMap::default(),
            qubit_outputs: FxHashMap::default(),
            clbit_inputs: FxHashMap::default(),
            clbit_outputs: FxHashMap::default(),
            global_phase: 0.0,
        }
    }

    /// Add a qubit to the circuit.
    pub fn add_qubit(&mut self, qubit: QubitId) {
        if self.qubit_inputs.contains_key(&qubit) {
            return;
        }
        let in_node = self.graph.add_node(DagNode::In(WireId::Qubit(qubit)));
        let out_node = self.graph.add_node(DagNode::Out(WireId::Qubit(qubit)));
        self.graph.add_edge(
            in_node,
            out_node,
            DagEdge {
                wire: WireId::Qubit(qubit),
            },
        );
        self.qubit_inputs.insert(qubit, in_node);
        self.qubit_outputs.insert(qubit, out_node);
    }

    /// Add a classical bit to the circuit.
    pub fn add_clbit(&mut self, clbit: ClbitId) {
        if self.clbit_inputs.contains_key(&clbit) {
            return;
        }
        let in_node = self.graph.add_node(DagNode::In(WireId::Clbit(clbit)));
        let out_node = self.graph.add_node(DagNode::Out(WireId::Clbit(clbit)));
        self.graph.add_edge(
            in_node,
            out_node,
            DagEdge {
                wire: WireId::Clbit(clbit),
            },
        );
        self.clbit_inputs.insert(clbit, in_node);
        self.clbit_outputs.insert(clbit, out_node);
    }

    /// Apply an instruction to the circuit.
    pub fn apply(&mut self, instruction: Instruction) -> IrResult<NodeIndex> {
        // Get gate name for error context
        let gate_name = match &instruction.kind {
            InstructionKind::Gate(gate) => Some(gate.name().to_string()),
            _ => None,
        };

        // Validate gate arity matches qubit count
        if let InstructionKind::Gate(gate) = &instruction.kind {
            let expected = gate.num_qubits() as usize;
            let got = instruction.qubits.len();
            if expected != got {
                return Err(IrError::QubitCountMismatch {
                    gate_name: gate.name().to_string(),
                    expected: expected as u32,
                    got: got as u32,
                });
            }
        }

        // Validate qubits exist
        for &qubit in &instruction.qubits {
            if !self.qubit_inputs.contains_key(&qubit) {
                return Err(IrError::QubitNotFound {
                    qubit,
                    gate_name: gate_name.clone(),
                });
            }
        }

        // Validate classical bits exist
        for &clbit in &instruction.clbits {
            if !self.clbit_inputs.contains_key(&clbit) {
                return Err(IrError::ClbitNotFound {
                    clbit,
                    gate_name: gate_name.clone(),
                });
            }
        }

        // Check for duplicate qubits in the instruction
        let mut seen = FxHashMap::default();
        for &qubit in &instruction.qubits {
            if seen.insert(qubit, ()).is_some() {
                return Err(IrError::DuplicateQubit {
                    qubit,
                    gate_name: gate_name.clone(),
                });
            }
        }

        // Add the operation node
        let op_node = self.graph.add_node(DagNode::Op(instruction.clone()));

        // Connect quantum wires
        for &qubit in &instruction.qubits {
            let out_node = self.qubit_outputs[&qubit];
            let wire = WireId::Qubit(qubit);

            // Find the edge going into the output node
            let incoming: Vec<_> = self
                .graph
                .edges_directed(out_node, Direction::Incoming)
                .filter(|e| e.weight().wire == wire)
                .map(|e| (e.source(), e.id()))
                .collect();

            if let Some((prev_node, edge_id)) = incoming.first() {
                // Remove edge from prev to output
                self.graph.remove_edge(*edge_id);
                // Add edge from prev to op
                self.graph.add_edge(*prev_node, op_node, DagEdge { wire });
                // Add edge from op to output
                self.graph.add_edge(op_node, out_node, DagEdge { wire });
            }
        }

        // Connect classical wires
        for &clbit in &instruction.clbits {
            let out_node = self.clbit_outputs[&clbit];
            let wire = WireId::Clbit(clbit);

            let incoming: Vec<_> = self
                .graph
                .edges_directed(out_node, Direction::Incoming)
                .filter(|e| e.weight().wire == wire)
                .map(|e| (e.source(), e.id()))
                .collect();

            if let Some((prev_node, edge_id)) = incoming.first() {
                self.graph.remove_edge(*edge_id);
                self.graph.add_edge(*prev_node, op_node, DagEdge { wire });
                self.graph.add_edge(op_node, out_node, DagEdge { wire });
            }
        }

        Ok(op_node)
    }

    /// Iterate over operations in topological order.
    pub fn topological_ops(&self) -> impl Iterator<Item = (NodeIndex, &Instruction)> {
        let sorted: Vec<_> = petgraph::algo::toposort(&self.graph, None)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|idx| {
                if let DagNode::Op(inst) = &self.graph[idx] {
                    Some((idx, inst))
                } else {
                    None
                }
            })
            .collect();

        // Sort is already topological, but let's make it deterministic
        sorted.into_iter()
    }

    /// Get an instruction by node index.
    pub fn get_instruction(&self, node: NodeIndex) -> Option<&Instruction> {
        self.graph.node_weight(node).and_then(|n| n.instruction())
    }

    /// Get a mutable instruction by node index.
    pub fn get_instruction_mut(&mut self, node: NodeIndex) -> Option<&mut Instruction> {
        self.graph
            .node_weight_mut(node)
            .and_then(|n| n.instruction_mut())
    }

    /// Remove an operation node from the DAG.
    pub fn remove_op(&mut self, node: NodeIndex) -> IrResult<Instruction> {
        let dag_node = self
            .graph
            .node_weight(node)
            .ok_or(IrError::InvalidNode)?
            .clone();

        let instruction = match dag_node {
            DagNode::Op(inst) => inst,
            _ => {
                return Err(IrError::InvalidDag(
                    "Cannot remove non-operation node".into(),
                ));
            }
        };

        // For each wire through this node, reconnect predecessors to successors
        let incoming: Vec<_> = self
            .graph
            .edges_directed(node, Direction::Incoming)
            .map(|e| (e.source(), e.weight().wire))
            .collect();

        let outgoing: Vec<_> = self
            .graph
            .edges_directed(node, Direction::Outgoing)
            .map(|e| (e.target(), e.weight().wire))
            .collect();

        // Remove the node (this removes all its edges)
        self.graph.remove_node(node);

        // Reconnect wires
        for (pred, wire) in &incoming {
            for (succ, succ_wire) in &outgoing {
                if wire == succ_wire {
                    self.graph.add_edge(*pred, *succ, DagEdge { wire: *wire });
                }
            }
        }

        Ok(instruction)
    }

    /// Substitute a node with a sequence of instructions.
    pub fn substitute_node(
        &mut self,
        node: NodeIndex,
        replacement: impl IntoIterator<Item = Instruction>,
    ) -> IrResult<Vec<NodeIndex>> {
        let _old = self.remove_op(node)?;
        let mut new_nodes = vec![];
        for inst in replacement {
            new_nodes.push(self.apply(inst)?);
        }
        Ok(new_nodes)
    }

    /// Get the number of qubits.
    pub fn num_qubits(&self) -> usize {
        self.qubit_inputs.len()
    }

    /// Get the number of classical bits.
    pub fn num_clbits(&self) -> usize {
        self.clbit_inputs.len()
    }

    /// Get the number of operations.
    pub fn num_ops(&self) -> usize {
        self.graph
            .node_indices()
            .filter(|&idx| matches!(self.graph[idx], DagNode::Op(_)))
            .count()
    }

    /// Calculate the circuit depth.
    pub fn depth(&self) -> usize {
        // Calculate the longest path through the DAG
        let mut depths: FxHashMap<NodeIndex, usize> = FxHashMap::default();

        for node in petgraph::algo::toposort(&self.graph, None).unwrap_or_default() {
            let max_pred_depth = self
                .graph
                .edges_directed(node, Direction::Incoming)
                .map(|e| depths.get(&e.source()).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);

            let node_depth = if matches!(self.graph[node], DagNode::Op(_)) {
                max_pred_depth + 1
            } else {
                max_pred_depth
            };

            depths.insert(node, node_depth);
        }

        depths.values().copied().max().unwrap_or(0)
    }

    /// Iterate over qubits.
    pub fn qubits(&self) -> impl Iterator<Item = QubitId> + '_ {
        self.qubit_inputs.keys().copied()
    }

    /// Iterate over classical bits.
    pub fn clbits(&self) -> impl Iterator<Item = ClbitId> + '_ {
        self.clbit_inputs.keys().copied()
    }

    /// Get the global phase.
    pub fn global_phase(&self) -> f64 {
        self.global_phase
    }

    /// Set the global phase.
    pub fn set_global_phase(&mut self, phase: f64) {
        self.global_phase = phase;
    }

    /// Get a reference to the underlying graph.
    pub fn graph(&self) -> &DiGraph<DagNode, DagEdge, u32> {
        &self.graph
    }
}

impl Default for CircuitDag {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CircuitDag {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            qubit_inputs: self.qubit_inputs.clone(),
            qubit_outputs: self.qubit_outputs.clone(),
            clbit_inputs: self.clbit_inputs.clone(),
            clbit_outputs: self.clbit_outputs.clone(),
            global_phase: self.global_phase,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::StandardGate;

    #[test]
    fn test_empty_dag() {
        let dag = CircuitDag::new();
        assert_eq!(dag.num_qubits(), 0);
        assert_eq!(dag.num_clbits(), 0);
        assert_eq!(dag.num_ops(), 0);
        assert_eq!(dag.depth(), 0);
    }

    #[test]
    fn test_add_qubits() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_qubit(QubitId(1));
        assert_eq!(dag.num_qubits(), 2);
    }

    #[test]
    fn test_apply_gate() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));

        let inst = Instruction::single_qubit_gate(StandardGate::H, QubitId(0));
        dag.apply(inst).unwrap();

        assert_eq!(dag.num_ops(), 1);
        assert_eq!(dag.depth(), 1);
    }

    #[test]
    fn test_bell_state_depth() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_qubit(QubitId(1));

        dag.apply(Instruction::single_qubit_gate(StandardGate::H, QubitId(0)))
            .unwrap();
        dag.apply(Instruction::two_qubit_gate(
            StandardGate::CX,
            QubitId(0),
            QubitId(1),
        ))
        .unwrap();

        assert_eq!(dag.num_ops(), 2);
        assert_eq!(dag.depth(), 2);
    }

    #[test]
    fn test_parallel_gates_depth() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_qubit(QubitId(1));

        // Two parallel H gates
        dag.apply(Instruction::single_qubit_gate(StandardGate::H, QubitId(0)))
            .unwrap();
        dag.apply(Instruction::single_qubit_gate(StandardGate::H, QubitId(1)))
            .unwrap();

        assert_eq!(dag.num_ops(), 2);
        // Parallel gates have depth 1
        assert_eq!(dag.depth(), 1);
    }

    #[test]
    fn test_gate_arity_mismatch() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_qubit(QubitId(1));

        // Try to apply a 2-qubit gate with only 1 qubit
        let inst = Instruction::gate(StandardGate::CX, [QubitId(0)]);
        let result = dag.apply(inst);

        assert!(result.is_err());
        match result {
            Err(IrError::QubitCountMismatch {
                gate_name,
                expected,
                got,
            }) => {
                assert_eq!(gate_name, "cx");
                assert_eq!(expected, 2);
                assert_eq!(got, 1);
            }
            _ => panic!("Expected QubitCountMismatch error"),
        }
    }

    #[test]
    fn test_qubit_not_found_with_context() {
        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));

        // Try to apply a gate with a non-existent qubit
        let inst = Instruction::two_qubit_gate(StandardGate::CX, QubitId(0), QubitId(99));
        let result = dag.apply(inst);

        assert!(result.is_err());
        match result {
            Err(IrError::QubitNotFound { qubit, gate_name }) => {
                assert_eq!(qubit, QubitId(99));
                assert_eq!(gate_name, Some("cx".to_string()));
            }
            _ => panic!("Expected QubitNotFound error"),
        }
    }
}
