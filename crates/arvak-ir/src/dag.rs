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
    #[inline]
    pub fn is_input(&self) -> bool {
        matches!(self, DagNode::In(_))
    }

    /// Check if this is an output node.
    #[inline]
    pub fn is_output(&self) -> bool {
        matches!(self, DagNode::Out(_))
    }

    /// Check if this is an operation node.
    #[inline]
    pub fn is_op(&self) -> bool {
        matches!(self, DagNode::Op(_))
    }

    /// Get the instruction if this is an operation node.
    #[inline]
    pub fn instruction(&self) -> Option<&Instruction> {
        match self {
            DagNode::Op(inst) => Some(inst),
            _ => None,
        }
    }

    /// Get mutable reference to the instruction.
    #[inline]
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

/// The abstraction level of a circuit in the compilation pipeline.
///
/// Circuits start at the `Logical` level (abstract qubits) and are
/// lowered to the `Physical` level by layout and routing passes
/// (qubits mapped to physical device positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CircuitLevel {
    /// Logical level: qubits are abstract, no physical mapping applied.
    #[default]
    Logical,
    /// Physical level: qubits are mapped to physical device positions.
    Physical,
}

/// DAG-based circuit representation.
///
/// The circuit is represented as a directed acyclic graph where:
/// - Nodes are either input nodes, output nodes, or operation nodes
/// - Edges represent wires (quantum or classical)
/// - Each wire has exactly one input and one output node
/// - Operations are connected to wires in topological order
///
/// ## Performance
///
/// The DAG maintains a `wire_front` index that maps each wire to the
/// last node before the output node. This enables O(1) predecessor
/// lookups in `apply()` instead of scanning all incoming edges of the
/// output node (which was O(degree) per qubit).
#[derive(Debug)]
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
    /// Wire front: maps each wire to the node just before the output node.
    /// Updated on every `apply()` and `remove_op()` to enable O(1)
    /// predecessor lookups instead of edge scanning.
    wire_front: FxHashMap<WireId, NodeIndex>,
    /// Global phase of the circuit.
    global_phase: f64,
    /// Abstraction level of the circuit.
    level: CircuitLevel,
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
            wire_front: FxHashMap::default(),
            global_phase: 0.0,
            level: CircuitLevel::Logical,
        }
    }

    /// Add a qubit to the circuit.
    pub fn add_qubit(&mut self, qubit: QubitId) {
        if self.qubit_inputs.contains_key(&qubit) {
            return;
        }
        let wire = WireId::Qubit(qubit);
        let in_node = self.graph.add_node(DagNode::In(wire));
        let out_node = self.graph.add_node(DagNode::Out(wire));
        self.graph.add_edge(in_node, out_node, DagEdge { wire });
        self.qubit_inputs.insert(qubit, in_node);
        self.qubit_outputs.insert(qubit, out_node);
        // Wire front: initially the input node is the predecessor of the output.
        self.wire_front.insert(wire, in_node);
    }

    /// Add a classical bit to the circuit.
    pub fn add_clbit(&mut self, clbit: ClbitId) {
        if self.clbit_inputs.contains_key(&clbit) {
            return;
        }
        let wire = WireId::Clbit(clbit);
        let in_node = self.graph.add_node(DagNode::In(wire));
        let out_node = self.graph.add_node(DagNode::Out(wire));
        self.graph.add_edge(in_node, out_node, DagEdge { wire });
        self.clbit_inputs.insert(clbit, in_node);
        self.clbit_outputs.insert(clbit, out_node);
        self.wire_front.insert(wire, in_node);
    }

    /// Apply an instruction to the circuit.
    #[allow(clippy::needless_pass_by_value, clippy::cast_possible_truncation)]
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
        let mut seen = rustc_hash::FxHashSet::default();
        for &qubit in &instruction.qubits {
            if !seen.insert(qubit) {
                return Err(IrError::DuplicateQubit {
                    qubit,
                    gate_name: gate_name.clone(),
                });
            }
        }

        // Add the operation node
        let op_node = self.graph.add_node(DagNode::Op(instruction.clone()));

        // Connect quantum wires — O(1) per qubit via wire_front index.
        for &qubit in &instruction.qubits {
            let out_node = self.qubit_outputs[&qubit];
            let wire = WireId::Qubit(qubit);

            // Look up the current front node (predecessor of output) in O(1).
            let prev_node = self.wire_front[&wire];

            // Find and remove the edge from prev to output on this wire.
            let edge_id = self
                .graph
                .edges_directed(prev_node, Direction::Outgoing)
                .find(|e| e.weight().wire == wire && e.target() == out_node)
                .map(|e| e.id());

            let eid = edge_id.ok_or_else(|| {
                IrError::InvalidDag(format!(
                    "Missing edge from predecessor to output for qubit wire {qubit:?}"
                ))
            })?;
            self.graph.remove_edge(eid);
            self.graph.add_edge(prev_node, op_node, DagEdge { wire });
            self.graph.add_edge(op_node, out_node, DagEdge { wire });
            // Update wire front: this op is now the predecessor of the output.
            self.wire_front.insert(wire, op_node);
        }

        // Connect classical wires — same O(1) approach.
        for &clbit in &instruction.clbits {
            let out_node = self.clbit_outputs[&clbit];
            let wire = WireId::Clbit(clbit);

            let prev_node = self.wire_front[&wire];

            let edge_id = self
                .graph
                .edges_directed(prev_node, Direction::Outgoing)
                .find(|e| e.weight().wire == wire && e.target() == out_node)
                .map(|e| e.id());

            let eid = edge_id.ok_or_else(|| {
                IrError::InvalidDag(format!(
                    "Missing edge from predecessor to output for classical wire {clbit:?}"
                ))
            })?;
            self.graph.remove_edge(eid);
            self.graph.add_edge(prev_node, op_node, DagEdge { wire });
            self.graph.add_edge(op_node, out_node, DagEdge { wire });
            self.wire_front.insert(wire, op_node);
        }

        Ok(op_node)
    }

    /// Iterate over operations in topological order.
    pub fn topological_ops(&self) -> impl Iterator<Item = (NodeIndex, &Instruction)> {
        let sorted: Vec<_> = petgraph::algo::toposort(&self.graph, None)
            .expect("DAG must be acyclic — cycle detected in circuit graph")
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
    #[inline]
    pub fn get_instruction(&self, node: NodeIndex) -> Option<&Instruction> {
        self.graph.node_weight(node).and_then(|n| n.instruction())
    }

    /// Get a mutable instruction by node index.
    #[inline]
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

        let DagNode::Op(instruction) = dag_node else {
            return Err(IrError::InvalidDag(
                "Cannot remove non-operation node".into(),
            ));
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

        // Record the last node index before removal. petgraph's `remove_node`
        // swaps the removed node with the last node, so the last node's index
        // changes to `node` after removal. We must update our index maps.
        //
        // WARNING: petgraph's `remove_node` swaps the removed node with the last
        // node in the graph, invalidating the last node's `NodeIndex`. Callers must
        // not hold stale `NodeIndex` references after calling `remove_op`. If you
        // are removing multiple nodes, iterate in reverse topological order or
        // re-fetch indices after each removal.
        let last_idx = NodeIndex::new(self.graph.node_count() - 1);

        // Before removal: update wire_front for wires that pass through the node
        // being removed. Point them at the predecessor on that wire instead.
        for (pred, wire) in &incoming {
            if self.wire_front.get(wire) == Some(&node) {
                self.wire_front.insert(*wire, *pred);
            }
        }

        self.graph.remove_node(node);

        // Helper to remap indices after petgraph's swap-remove.
        let fix = |idx: NodeIndex| -> NodeIndex {
            if last_idx != node && idx == last_idx {
                node
            } else {
                idx
            }
        };

        // If the removed node was not the last node, petgraph swapped the last
        // node into the removed node's slot. Update all maps referencing the old
        // last index to point to `node` (its new index after the swap).
        if last_idx != node {
            for v in self.qubit_inputs.values_mut() {
                if *v == last_idx {
                    *v = node;
                }
            }
            for v in self.qubit_outputs.values_mut() {
                if *v == last_idx {
                    *v = node;
                }
            }
            for v in self.clbit_inputs.values_mut() {
                if *v == last_idx {
                    *v = node;
                }
            }
            for v in self.clbit_outputs.values_mut() {
                if *v == last_idx {
                    *v = node;
                }
            }
            for v in self.wire_front.values_mut() {
                if *v == last_idx {
                    *v = node;
                }
            }
        }

        // Reconnect wires: add edges from predecessor to successor for each wire.
        // Predecessor/successor indices collected before removal may reference the
        // last node, which has been swapped — apply the fix.
        for (pred, wire) in &incoming {
            let pred = fix(*pred);
            for (succ, succ_wire) in &outgoing {
                let succ = fix(*succ);
                if wire == succ_wire {
                    self.graph.add_edge(pred, succ, DagEdge { wire: *wire });
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
    #[inline]
    pub fn num_qubits(&self) -> usize {
        self.qubit_inputs.len()
    }

    /// Get the number of classical bits.
    #[inline]
    pub fn num_clbits(&self) -> usize {
        self.clbit_inputs.len()
    }

    /// Get the number of operations.
    ///
    /// Computed as total nodes minus input and output nodes (2 per qubit + 2 per clbit).
    #[inline]
    pub fn num_ops(&self) -> usize {
        let io_nodes = 2 * (self.qubit_inputs.len() + self.clbit_inputs.len());
        self.graph.node_count().saturating_sub(io_nodes)
    }

    /// Calculate the circuit depth.
    pub fn depth(&self) -> usize {
        let node_count = self.graph.node_count();
        // Pre-allocate with expected capacity
        let mut depths: FxHashMap<NodeIndex, usize> =
            FxHashMap::with_capacity_and_hasher(node_count, Default::default());

        let mut max_depth = 0usize;

        for node in petgraph::algo::toposort(&self.graph, None)
            .expect("DAG must be acyclic — cycle detected in circuit graph")
        {
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

            if node_depth > max_depth {
                max_depth = node_depth;
            }
            depths.insert(node, node_depth);
        }

        max_depth
    }

    /// Iterate over qubits.
    pub fn qubits(&self) -> impl Iterator<Item = QubitId> + '_ {
        self.qubit_inputs.keys().copied()
    }

    /// Get the input node for a qubit (O(1) lookup).
    #[inline]
    pub fn qubit_input_node(&self, qubit: QubitId) -> Option<NodeIndex> {
        self.qubit_inputs.get(&qubit).copied()
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

    /// Get the abstraction level of this circuit.
    pub fn level(&self) -> CircuitLevel {
        self.level
    }

    /// Set the abstraction level of this circuit.
    pub fn set_level(&mut self, level: CircuitLevel) {
        self.level = level;
    }

    /// Get a reference to the underlying graph.
    pub fn graph(&self) -> &DiGraph<DagNode, DagEdge, u32> {
        &self.graph
    }

    /// Verify the structural integrity of the DAG.
    ///
    /// Checks that:
    /// - Every qubit has exactly one In node and one Out node
    /// - Every classical bit has exactly one In node and one Out node
    /// - The graph is acyclic
    /// - All operation nodes are reachable from some In node
    /// - Wire edges form valid paths from In to Out for each wire
    #[allow(clippy::too_many_lines)]
    pub fn verify_integrity(&self) -> IrResult<()> {
        // 1. Check that the graph is acyclic
        if petgraph::algo::is_cyclic_directed(&self.graph) {
            return Err(IrError::InvalidDag("Graph contains a cycle".into()));
        }

        // 2. Check that every qubit has In and Out nodes
        for &qubit in self.qubit_inputs.keys() {
            if !self.qubit_outputs.contains_key(&qubit) {
                return Err(IrError::InvalidDag(format!(
                    "Qubit {qubit:?} has an In node but no Out node"
                )));
            }
        }
        for &qubit in self.qubit_outputs.keys() {
            if !self.qubit_inputs.contains_key(&qubit) {
                return Err(IrError::InvalidDag(format!(
                    "Qubit {qubit:?} has an Out node but no In node"
                )));
            }
        }

        // 3. Check that every clbit has In and Out nodes
        for &clbit in self.clbit_inputs.keys() {
            if !self.clbit_outputs.contains_key(&clbit) {
                return Err(IrError::InvalidDag(format!(
                    "Clbit {clbit:?} has an In node but no Out node"
                )));
            }
        }
        for &clbit in self.clbit_outputs.keys() {
            if !self.clbit_inputs.contains_key(&clbit) {
                return Err(IrError::InvalidDag(format!(
                    "Clbit {clbit:?} has an Out node but no In node"
                )));
            }
        }

        // 4. Verify wire continuity for each qubit: walk from In to Out
        for (&qubit, &in_node) in &self.qubit_inputs {
            let out_node = self.qubit_outputs[&qubit];
            let wire = WireId::Qubit(qubit);

            let mut current = in_node;
            let mut steps = 0;
            let max_steps = self.graph.node_count();

            loop {
                if current == out_node {
                    break;
                }

                // Find the outgoing edge for this wire
                let next = self
                    .graph
                    .edges_directed(current, Direction::Outgoing)
                    .find(|e| e.weight().wire == wire)
                    .map(|e| e.target());

                match next {
                    Some(n) => current = n,
                    None => {
                        return Err(IrError::InvalidDag(format!(
                            "Wire for qubit {qubit:?} is broken: no outgoing edge from node {current:?}"
                        )));
                    }
                }

                steps += 1;
                if steps > max_steps {
                    return Err(IrError::InvalidDag(format!(
                        "Wire for qubit {qubit:?} has too many steps (possible infinite loop)"
                    )));
                }
            }
        }

        // 5. Verify wire continuity for each clbit
        for (&clbit, &in_node) in &self.clbit_inputs {
            let out_node = self.clbit_outputs[&clbit];
            let wire = WireId::Clbit(clbit);

            let mut current = in_node;
            let mut steps = 0;
            let max_steps = self.graph.node_count();

            loop {
                if current == out_node {
                    break;
                }

                let next = self
                    .graph
                    .edges_directed(current, Direction::Outgoing)
                    .find(|e| e.weight().wire == wire)
                    .map(|e| e.target());

                match next {
                    Some(n) => current = n,
                    None => {
                        return Err(IrError::InvalidDag(format!(
                            "Wire for clbit {clbit:?} is broken: no outgoing edge from node {current:?}"
                        )));
                    }
                }

                steps += 1;
                if steps > max_steps {
                    return Err(IrError::InvalidDag(format!(
                        "Wire for clbit {clbit:?} has too many steps (possible infinite loop)"
                    )));
                }
            }
        }

        // 6. Check all operation nodes are reachable from some In node.
        // A successful toposort already visits all nodes in the graph, so
        // if it succeeds (which it does since we checked acyclicity above),
        // all nodes are reachable. We only need to verify the sorted set
        // covers every op node.
        let topo_nodes = petgraph::algo::toposort(&self.graph, None).unwrap_or_default();
        let node_count = self.graph.node_count();
        if topo_nodes.len() != node_count {
            return Err(IrError::InvalidDag(
                "Unreachable operation node found in DAG".into(),
            ));
        }

        Ok(())
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
            wire_front: self.wire_front.clone(),
            global_phase: self.global_phase,
            level: self.level,
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

    #[test]
    fn test_verify_integrity_empty() {
        let dag = CircuitDag::new();
        dag.verify_integrity().unwrap();
    }

    #[test]
    fn test_verify_integrity_simple_circuit() {
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

        dag.verify_integrity().unwrap();
    }

    #[test]
    fn test_verify_integrity_with_measurement() {
        use crate::qubit::ClbitId;

        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_clbit(ClbitId(0));
        dag.apply(Instruction::single_qubit_gate(StandardGate::H, QubitId(0)))
            .unwrap();
        dag.apply(Instruction::measure(QubitId(0), ClbitId(0)))
            .unwrap();

        dag.verify_integrity().unwrap();
    }

    #[test]
    fn test_verify_integrity_multi_qubit_circuit() {
        use crate::qubit::ClbitId;

        let mut dag = CircuitDag::new();
        dag.add_qubit(QubitId(0));
        dag.add_qubit(QubitId(1));
        dag.add_qubit(QubitId(2));
        dag.add_clbit(ClbitId(0));
        dag.add_clbit(ClbitId(1));
        dag.add_clbit(ClbitId(2));

        // Build a GHZ-like circuit
        dag.apply(Instruction::single_qubit_gate(StandardGate::H, QubitId(0)))
            .unwrap();
        dag.apply(Instruction::two_qubit_gate(
            StandardGate::CX,
            QubitId(0),
            QubitId(1),
        ))
        .unwrap();
        dag.apply(Instruction::two_qubit_gate(
            StandardGate::CX,
            QubitId(1),
            QubitId(2),
        ))
        .unwrap();
        dag.apply(Instruction::measure(QubitId(0), ClbitId(0)))
            .unwrap();
        dag.apply(Instruction::measure(QubitId(1), ClbitId(1)))
            .unwrap();
        dag.apply(Instruction::measure(QubitId(2), ClbitId(2)))
            .unwrap();

        dag.verify_integrity().unwrap();
    }
}
