//! Cancellation optimization passes.

use arvak_ir::CircuitDag;
use arvak_ir::dag::{DagNode, NodeIndex, WireId};
use arvak_ir::gate::{GateKind, StandardGate};
use arvak_ir::instruction::{Instruction, InstructionKind};
use arvak_ir::parameter::ParameterExpression;
use arvak_ir::qubit::QubitId;
use petgraph::Direction;
use petgraph::visit::EdgeRef;
use rustc_hash::FxHashSet;

use crate::error::CompileResult;
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;
use crate::unitary::Unitary2x2;

use super::EPSILON;

/// CX cancellation pass.
///
/// Cancels pairs of adjacent CX gates on the same qubits.
/// CX · CX = I (identity)
pub struct CancelCX;

impl CancelCX {
    /// Create a new CX cancellation pass.
    pub fn new() -> Self {
        Self
    }

    /// Find pairs of adjacent CX gates that can be cancelled.
    #[allow(clippy::unused_self)]
    fn find_cancellable_pairs(&self, dag: &CircuitDag) -> Vec<(NodeIndex, NodeIndex)> {
        let mut pairs = Vec::new();
        let mut processed: FxHashSet<NodeIndex> = FxHashSet::default();

        // Build adjacency information
        let graph = dag.graph();

        for (node_idx, inst) in dag.topological_ops() {
            if processed.contains(&node_idx) {
                continue;
            }

            // Check if this is a CX gate
            if let InstructionKind::Gate(gate) = &inst.kind {
                if let GateKind::Standard(StandardGate::CX) = &gate.kind {
                    let control = inst.qubits[0];
                    let target = inst.qubits[1];

                    // Look for the next gate on both qubits
                    let successors: Vec<_> = graph
                        .edges_directed(node_idx, Direction::Outgoing)
                        .filter_map(|e| {
                            let target_node = e.target();
                            if let DagNode::Op(succ_inst) = &graph[target_node] {
                                Some((target_node, succ_inst))
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Find if there's a CX with the same control/target as immediate successor
                    for (succ_idx, succ_inst) in &successors {
                        if let InstructionKind::Gate(succ_gate) = &succ_inst.kind {
                            if let GateKind::Standard(StandardGate::CX) = &succ_gate.kind {
                                if succ_inst.qubits.len() == 2
                                    && succ_inst.qubits[0] == control
                                    && succ_inst.qubits[1] == target
                                {
                                    // Check if this is truly adjacent (no intervening gates on either wire)
                                    if self.is_truly_adjacent(
                                        dag, node_idx, *succ_idx, control, target,
                                    ) {
                                        pairs.push((node_idx, *succ_idx));
                                        processed.insert(node_idx);
                                        processed.insert(*succ_idx);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        pairs
    }

    /// Check if two nodes are truly adjacent on both wires.
    #[allow(clippy::unused_self)]
    fn is_truly_adjacent(
        &self,
        dag: &CircuitDag,
        node1: NodeIndex,
        node2: NodeIndex,
        control: QubitId,
        target: QubitId,
    ) -> bool {
        let graph = dag.graph();

        // For each qubit, check that node2 is the immediate successor of node1
        for qubit in [control, target] {
            let wire = WireId::Qubit(qubit);

            // Find successor of node1 on this wire
            let mut found_direct = false;
            for edge in graph.edges_directed(node1, Direction::Outgoing) {
                if edge.weight().wire == wire && edge.target() == node2 {
                    found_direct = true;
                    break;
                }
            }

            if !found_direct {
                return false;
            }
        }

        true
    }
}

impl Default for CancelCX {
    fn default() -> Self {
        Self::new()
    }
}

impl Pass for CancelCX {
    fn name(&self) -> &'static str {
        "CancelCX"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // Keep cancelling until no more pairs found.
        // Bound iterations to avoid pathological cases.
        const MAX_ITERATIONS: usize = 100;
        for _ in 0..MAX_ITERATIONS {
            let pairs = self.find_cancellable_pairs(dag);
            if pairs.is_empty() {
                break;
            }

            // Remove pairs (in reverse order to maintain indices)
            for (node1, node2) in pairs.into_iter().rev() {
                let _ = dag.remove_op(node2);
                let _ = dag.remove_op(node1);
            }
        }

        Ok(())
    }
}

/// Commutative cancellation pass.
///
/// Uses gate commutation rules to move gates past each other and enable
/// additional cancellations.
///
/// Commutation rules used:
/// - CZ commutes with RZ on either qubit
/// - CX commutes with RZ on control, RX on target
/// - Diagonal gates commute with each other
/// - Same-type rotation gates can be merged (RZ(a) · RZ(b) = RZ(a+b))
pub struct CommutativeCancellation;

impl CommutativeCancellation {
    /// Create a new commutative cancellation pass.
    pub fn new() -> Self {
        Self
    }

    /// Check if two gates commute.
    ///
    /// This is used for future enhancements where gates can be reordered
    /// to enable additional cancellations.
    #[allow(dead_code, clippy::match_same_arms)]
    fn gates_commute(
        gate1: &StandardGate,
        qubits1: &[QubitId],
        gate2: &StandardGate,
        qubits2: &[QubitId],
    ) -> bool {
        // Gates on disjoint qubits always commute
        let shared: Vec<_> = qubits1.iter().filter(|q| qubits2.contains(q)).collect();
        if shared.is_empty() {
            return true;
        }

        // Check specific commutation rules
        match (gate1, gate2) {
            // Diagonal gates commute with each other
            (g1, g2) if Self::is_diagonal(g1) && Self::is_diagonal(g2) => true,

            // CZ commutes with Rz on either qubit
            (StandardGate::CZ, StandardGate::Rz(_)) | (StandardGate::Rz(_), StandardGate::CZ) => {
                true
            }
            (StandardGate::CZ, StandardGate::Z) | (StandardGate::Z, StandardGate::CZ) => true,
            (StandardGate::CZ, StandardGate::S) | (StandardGate::S, StandardGate::CZ) => true,
            (StandardGate::CZ, StandardGate::T) | (StandardGate::T, StandardGate::CZ) => true,

            // CX commutes with Rz on control
            (StandardGate::CX, StandardGate::Rz(_)) | (StandardGate::Rz(_), StandardGate::CX) => {
                // Only if Rz is on the control qubit of CX
                // This requires more context - skip for now
                false
            }

            _ => false,
        }
    }

    /// Check if a gate is diagonal (only affects phases, not populations).
    ///
    /// Used by `gates_commute` for determining commutation relationships.
    #[allow(dead_code)]
    fn is_diagonal(gate: &StandardGate) -> bool {
        matches!(
            gate,
            StandardGate::I
                | StandardGate::Z
                | StandardGate::S
                | StandardGate::Sdg
                | StandardGate::T
                | StandardGate::Tdg
                | StandardGate::Rz(_)
                | StandardGate::P(_)
                | StandardGate::CZ
        )
    }

    /// Merge two same-type rotation gates.
    fn merge_rotations(gate1: &StandardGate, gate2: &StandardGate) -> Option<StandardGate> {
        match (gate1, gate2) {
            (StandardGate::Rz(p1), StandardGate::Rz(p2)) => {
                let a1 = p1.as_f64()?;
                let a2 = p2.as_f64()?;
                let sum = Unitary2x2::normalize_angle(a1 + a2);
                if sum.abs() < EPSILON {
                    None // Cancels to identity
                } else {
                    Some(StandardGate::Rz(ParameterExpression::constant(sum)))
                }
            }
            (StandardGate::Rx(p1), StandardGate::Rx(p2)) => {
                let a1 = p1.as_f64()?;
                let a2 = p2.as_f64()?;
                let sum = Unitary2x2::normalize_angle(a1 + a2);
                if sum.abs() < EPSILON {
                    None
                } else {
                    Some(StandardGate::Rx(ParameterExpression::constant(sum)))
                }
            }
            (StandardGate::Ry(p1), StandardGate::Ry(p2)) => {
                let a1 = p1.as_f64()?;
                let a2 = p2.as_f64()?;
                let sum = Unitary2x2::normalize_angle(a1 + a2);
                if sum.abs() < EPSILON {
                    None
                } else {
                    Some(StandardGate::Ry(ParameterExpression::constant(sum)))
                }
            }
            _ => None,
        }
    }

    /// Find mergeable rotation pairs.
    /// Returns (node1, node2, Option<`merged_gate`>) where None means both gates cancel.
    #[allow(clippy::similar_names, clippy::unused_self)]
    fn find_mergeable_rotations(
        &self,
        dag: &CircuitDag,
    ) -> Vec<(NodeIndex, NodeIndex, Option<StandardGate>)> {
        let mut merges = Vec::new();
        let mut processed: FxHashSet<NodeIndex> = FxHashSet::default();

        for (node_idx, inst) in dag.topological_ops() {
            if processed.contains(&node_idx) {
                continue;
            }

            if let InstructionKind::Gate(gate) = &inst.kind {
                if let GateKind::Standard(std_gate) = &gate.kind {
                    // Only process rotation gates
                    if !matches!(
                        std_gate,
                        StandardGate::Rx(_) | StandardGate::Ry(_) | StandardGate::Rz(_)
                    ) {
                        continue;
                    }

                    let qubit = inst.qubits[0];
                    let wire = WireId::Qubit(qubit);

                    // Find immediate successor on this wire
                    let graph = dag.graph();
                    for edge in graph.edges_directed(node_idx, Direction::Outgoing) {
                        if edge.weight().wire != wire {
                            continue;
                        }

                        let succ_node = edge.target();
                        if let DagNode::Op(succ_inst) = &graph[succ_node] {
                            if let InstructionKind::Gate(succ_gate) = &succ_inst.kind {
                                if let GateKind::Standard(succ_std) = &succ_gate.kind {
                                    // Check if they're the same type of rotation
                                    let same_type = matches!(
                                        (std_gate, succ_std),
                                        (StandardGate::Rz(_), StandardGate::Rz(_))
                                            | (StandardGate::Rx(_), StandardGate::Rx(_))
                                            | (StandardGate::Ry(_), StandardGate::Ry(_))
                                    );
                                    if same_type {
                                        let merged = Self::merge_rotations(std_gate, succ_std);
                                        merges.push((node_idx, succ_node, merged));
                                        processed.insert(node_idx);
                                        processed.insert(succ_node);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        merges
    }
}

impl Default for CommutativeCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl Pass for CommutativeCancellation {
    fn name(&self) -> &'static str {
        "CommutativeCancellation"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // Find and merge adjacent same-type rotations.
        // Bound iterations to avoid pathological cases.
        const MAX_ITERATIONS: usize = 100;
        for _ in 0..MAX_ITERATIONS {
            let merges = self.find_mergeable_rotations(dag);
            if merges.is_empty() {
                break;
            }

            for (node1, node2, merged) in merges.into_iter().rev() {
                // Remove second node
                let _ = dag.remove_op(node2);

                match merged {
                    Some(gate) => {
                        // Replace first node with merged gate
                        if let Some(inst) = dag.get_instruction_mut(node1) {
                            *inst = Instruction::single_qubit_gate(gate, inst.qubits[0]);
                        }
                    }
                    None => {
                        // Gates cancel - remove both
                        let _ = dag.remove_op(node1);
                    }
                }
            }
        }

        Ok(())
    }
}
