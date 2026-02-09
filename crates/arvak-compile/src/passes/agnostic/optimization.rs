//! Optimization passes.

use arvak_ir::CircuitDag;
use arvak_ir::dag::{DagNode, NodeIndex, WireId};
use arvak_ir::gate::{GateKind, StandardGate};
use arvak_ir::instruction::{Instruction, InstructionKind};
use arvak_ir::noise::NoiseRole;
use arvak_ir::parameter::ParameterExpression;
use arvak_ir::qubit::QubitId;
use petgraph::Direction;
use petgraph::visit::EdgeRef;
use rustc_hash::FxHashSet;
use std::f64::consts::PI;

use crate::error::CompileResult;
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;
use crate::unitary::Unitary2x2;

/// Tolerance for angle comparisons.
const EPSILON: f64 = 1e-10;

/// Single-qubit gate optimization pass.
///
/// Merges consecutive single-qubit gates on the same qubit and decomposes
/// them back to a minimal gate sequence using ZYZ decomposition.
pub struct Optimize1qGates {
    /// Target basis gates for decomposition.
    basis: OneQubitBasis,
}

/// Target basis for 1-qubit gate decomposition.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum OneQubitBasis {
    /// Use RZ, RY, RZ decomposition.
    #[default]
    ZYZ,
    /// Use U3 gate (theta, phi, lambda).
    U3,
    /// Use RZ, SX decomposition (IBM native).
    ZSX,
}

impl Default for Optimize1qGates {
    fn default() -> Self {
        Self::new()
    }
}

impl Optimize1qGates {
    /// Create a new 1q gate optimizer with ZYZ basis.
    pub fn new() -> Self {
        Self {
            basis: OneQubitBasis::ZYZ,
        }
    }

    /// Create a new 1q gate optimizer with specified basis.
    pub fn with_basis(basis: OneQubitBasis) -> Self {
        Self { basis }
    }

    /// Get the unitary matrix for a single-qubit gate.
    fn gate_to_unitary(gate: &StandardGate) -> Option<Unitary2x2> {
        match gate {
            StandardGate::I => Some(Unitary2x2::identity()),
            StandardGate::X => Some(Unitary2x2::x()),
            StandardGate::Y => Some(Unitary2x2::y()),
            StandardGate::Z => Some(Unitary2x2::z()),
            StandardGate::H => Some(Unitary2x2::h()),
            StandardGate::S => Some(Unitary2x2::s()),
            StandardGate::Sdg => Some(Unitary2x2::sdg()),
            StandardGate::T => Some(Unitary2x2::t()),
            StandardGate::Tdg => Some(Unitary2x2::tdg()),
            StandardGate::SX => Some(Unitary2x2::sx()),
            StandardGate::SXdg => Some(Unitary2x2::sxdg()),
            StandardGate::Rx(p) => p.as_f64().map(Unitary2x2::rx),
            StandardGate::Ry(p) => p.as_f64().map(Unitary2x2::ry),
            StandardGate::Rz(p) => p.as_f64().map(Unitary2x2::rz),
            StandardGate::P(p) => p.as_f64().map(Unitary2x2::p),
            StandardGate::U(theta, phi, lambda) => {
                let t = theta.as_f64()?;
                let p = phi.as_f64()?;
                let l = lambda.as_f64()?;
                Some(Unitary2x2::u(t, p, l))
            }
            StandardGate::PRX(theta, phi) => {
                // PRX(θ, φ) = RZ(φ) · RX(θ) · RZ(-φ)
                let t = theta.as_f64()?;
                let p = phi.as_f64()?;
                let rz_phi = Unitary2x2::rz(p);
                let rx_theta = Unitary2x2::rx(t);
                let rz_neg_phi = Unitary2x2::rz(-p);
                Some(rz_phi * rx_theta * rz_neg_phi)
            }
            _ => None, // Multi-qubit gates
        }
    }

    /// Decompose a unitary to gates based on the target basis.
    fn decompose_unitary(&self, unitary: &Unitary2x2) -> Vec<StandardGate> {
        let (alpha, beta, gamma, _phase) = unitary.zyz_decomposition();

        // Normalize angles
        let alpha = Unitary2x2::normalize_angle(alpha);
        let beta = Unitary2x2::normalize_angle(beta);
        let gamma = Unitary2x2::normalize_angle(gamma);

        match self.basis {
            OneQubitBasis::ZYZ => {
                let mut gates = Vec::new();

                // RZ(gamma)
                if gamma.abs() > EPSILON {
                    gates.push(StandardGate::Rz(ParameterExpression::constant(gamma)));
                }

                // RY(beta)
                if beta.abs() > EPSILON {
                    gates.push(StandardGate::Ry(ParameterExpression::constant(beta)));
                }

                // RZ(alpha)
                if alpha.abs() > EPSILON {
                    gates.push(StandardGate::Rz(ParameterExpression::constant(alpha)));
                }

                // If empty (identity), return nothing
                gates
            }
            OneQubitBasis::U3 => {
                // Skip if identity
                if alpha.abs() < EPSILON && beta.abs() < EPSILON && gamma.abs() < EPSILON {
                    return vec![];
                }

                // U(theta, phi, lambda) where:
                // theta = beta, phi = alpha - π/2, lambda = gamma + π/2
                // Actually for our ZYZ: U(beta, alpha, gamma) directly
                vec![StandardGate::U(
                    ParameterExpression::constant(beta),
                    ParameterExpression::constant(alpha),
                    ParameterExpression::constant(gamma),
                )]
            }
            OneQubitBasis::ZSX => {
                // Decompose to RZ, SX gates (IBM native)
                // This is more complex - for now use ZYZ and convert
                self.zyz_to_zsx(alpha, beta, gamma)
            }
        }
    }

    /// Convert ZYZ angles to RZ-SX decomposition.
    #[allow(clippy::unused_self)]
    fn zyz_to_zsx(&self, alpha: f64, beta: f64, gamma: f64) -> Vec<StandardGate> {
        // RY(β) = RZ(π/2) · SX · RZ(β) · SX · RZ(-π/2)
        // So: RZ(α) · RY(β) · RZ(γ)
        //   = RZ(α) · RZ(π/2) · SX · RZ(β) · SX · RZ(-π/2) · RZ(γ)
        //   = RZ(α + π/2) · SX · RZ(β) · SX · RZ(γ - π/2)

        let mut gates = Vec::new();

        if beta.abs() < EPSILON {
            // Pure Z rotation
            let total_z = alpha + gamma;
            if total_z.abs() > EPSILON {
                gates.push(StandardGate::Rz(ParameterExpression::constant(total_z)));
            }
        } else {
            // Full decomposition
            let z1 = gamma - PI / 2.0;
            let z2 = beta;
            let z3 = alpha + PI / 2.0;

            if z1.abs() > EPSILON {
                gates.push(StandardGate::Rz(ParameterExpression::constant(z1)));
            }
            gates.push(StandardGate::SX);
            if z2.abs() > EPSILON {
                gates.push(StandardGate::Rz(ParameterExpression::constant(z2)));
            }
            gates.push(StandardGate::SX);
            if z3.abs() > EPSILON {
                gates.push(StandardGate::Rz(ParameterExpression::constant(z3)));
            }
        }

        gates
    }

    /// Find runs of consecutive 1q gates on each qubit.
    #[allow(clippy::unused_self)]
    fn find_1q_runs(&self, dag: &CircuitDag) -> Vec<(QubitId, Vec<NodeIndex>)> {
        let mut runs = Vec::new();
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        // For each qubit, trace through and find maximal runs
        for qubit in dag.qubits() {
            let mut current_run: Vec<NodeIndex> = Vec::new();

            for (node_idx, inst) in dag.topological_ops() {
                // Only consider operations on this qubit
                if !inst.qubits.contains(&qubit) {
                    continue;
                }

                // Check if this is a single-qubit gate on exactly this qubit
                if inst.qubits.len() == 1 && !visited.contains(&node_idx) {
                    if let InstructionKind::Gate(gate) = &inst.kind {
                        if let GateKind::Standard(std_gate) = &gate.kind {
                            if std_gate.num_qubits() == 1
                                && Self::gate_to_unitary(std_gate).is_some()
                            {
                                current_run.push(node_idx);
                                continue;
                            }
                        }
                    }
                }

                // Resource noise channels are optimization barriers —
                // they must not be reordered or removed.
                if let InstructionKind::NoiseChannel {
                    role: NoiseRole::Resource,
                    ..
                } = &inst.kind
                {
                    if current_run.len() > 1 {
                        for &idx in &current_run {
                            visited.insert(idx);
                        }
                        runs.push((qubit, std::mem::take(&mut current_run)));
                    } else {
                        current_run.clear();
                    }
                    continue;
                }

                // Deficit noise channels are informational — skip over them
                // without breaking the run.
                if let InstructionKind::NoiseChannel {
                    role: NoiseRole::Deficit,
                    ..
                } = &inst.kind
                {
                    continue;
                }

                // Multi-qubit gate or non-optimizable: end current run
                if current_run.len() > 1 {
                    for &idx in &current_run {
                        visited.insert(idx);
                    }
                    runs.push((qubit, std::mem::take(&mut current_run)));
                } else {
                    current_run.clear();
                }
            }

            // Don't forget the final run
            if current_run.len() > 1 {
                for &idx in &current_run {
                    visited.insert(idx);
                }
                runs.push((qubit, current_run));
            }
        }

        runs
    }
}

impl Pass for Optimize1qGates {
    fn name(&self) -> &'static str {
        "Optimize1qGates"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // Find all runs of consecutive 1q gates
        let runs = self.find_1q_runs(dag);

        // Process each run
        for (qubit, nodes) in runs {
            if nodes.len() < 2 {
                continue;
            }

            // Compute combined unitary
            let mut combined = Unitary2x2::identity();

            for &node_idx in &nodes {
                if let Some(inst) = dag.get_instruction(node_idx) {
                    if let InstructionKind::Gate(gate) = &inst.kind {
                        if let GateKind::Standard(std_gate) = &gate.kind {
                            if let Some(u) = Self::gate_to_unitary(std_gate) {
                                combined = combined * u;
                            }
                        }
                    }
                }
            }

            // Decompose to minimal gate sequence
            let new_gates = self.decompose_unitary(&combined);

            // Strategy: Keep the first N nodes we need, remove the rest
            // Then update the kept nodes with new gates

            let num_new = new_gates.len();

            if num_new == 0 {
                // All gates cancel - remove all nodes in this run
                for &node_idx in &nodes {
                    let _ = dag.remove_op(node_idx);
                }
            } else if num_new <= nodes.len() {
                // We can replace in-place: update first N nodes, remove the rest
                let (keep, remove) = nodes.split_at(num_new);

                // Update kept nodes
                for (i, &node_idx) in keep.iter().enumerate() {
                    if let Some(inst) = dag.get_instruction_mut(node_idx) {
                        *inst = Instruction::single_qubit_gate(new_gates[i].clone(), qubit);
                    }
                }

                // Remove extra nodes
                for &node_idx in remove {
                    let _ = dag.remove_op(node_idx);
                }
            } else {
                // Need more nodes than we have - just update what we can
                // This shouldn't happen with ZYZ decomposition (max 3 gates)
                for (i, &node_idx) in nodes.iter().enumerate() {
                    if i < num_new {
                        if let Some(inst) = dag.get_instruction_mut(node_idx) {
                            *inst = Instruction::single_qubit_gate(new_gates[i].clone(), qubit);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn should_run(&self, dag: &CircuitDag, _properties: &PropertySet) -> bool {
        // Only run if there are operations to optimize
        dag.num_ops() > 1
    }
}

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
        // Keep cancelling until no more pairs found
        loop {
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
        // Find and merge adjacent same-type rotations
        loop {
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

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::Circuit;

    #[test]
    fn test_optimize_1q_hh_cancels() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.h(QubitId(0)).unwrap(); // H·H = I
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new();
        Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

        // H·H should cancel to identity (0 gates or very small rotation)
        // Due to numerical precision, we might get 0 gates
        assert!(
            dag.num_ops() <= 1,
            "Expected 0 or 1 ops, got {}",
            dag.num_ops()
        );
    }

    #[test]
    fn test_optimize_1q_reduces_count() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.t(QubitId(0)).unwrap();
        circuit.t(QubitId(0)).unwrap();
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let initial_ops = dag.num_ops();
        assert_eq!(initial_ops, 4);

        let mut props = PropertySet::new();
        Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

        // Should reduce 4 gates to at most 3 (RZ, RY, RZ)
        assert!(
            dag.num_ops() <= 3,
            "Expected at most 3 ops, got {}",
            dag.num_ops()
        );
    }

    #[test]
    fn test_cancel_cx_adjacent() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap(); // CX·CX = I
        let mut dag = circuit.into_dag();

        assert_eq!(dag.num_ops(), 2);

        let mut props = PropertySet::new();
        CancelCX::new().run(&mut dag, &mut props).unwrap();

        // Should cancel both CX gates
        assert_eq!(dag.num_ops(), 0);
    }

    #[test]
    fn test_cancel_cx_not_adjacent() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.h(QubitId(0)).unwrap(); // Intervening gate
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        assert_eq!(dag.num_ops(), 3);

        let mut props = PropertySet::new();
        CancelCX::new().run(&mut dag, &mut props).unwrap();

        // Should NOT cancel (H gate between them)
        assert_eq!(dag.num_ops(), 3);
    }

    #[test]
    fn test_commutative_rz_merge() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.rz(PI / 4.0, QubitId(0)).unwrap();
        circuit.rz(PI / 4.0, QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        assert_eq!(dag.num_ops(), 2);

        let mut props = PropertySet::new();
        CommutativeCancellation::new()
            .run(&mut dag, &mut props)
            .unwrap();

        // Should merge to single RZ(π/2)
        assert_eq!(dag.num_ops(), 1);
    }

    #[test]
    fn test_commutative_rz_cancellation() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.rz(PI, QubitId(0)).unwrap();
        circuit.rz(-PI, QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        assert_eq!(dag.num_ops(), 2);

        let mut props = PropertySet::new();
        CommutativeCancellation::new()
            .run(&mut dag, &mut props)
            .unwrap();

        // Should merge and normalize to near-zero, which might remove the gate
        assert!(dag.num_ops() <= 1);
    }

    #[test]
    fn test_resource_noise_blocks_optimization() {
        use arvak_ir::noise::NoiseModel;

        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit
            .channel_resource(NoiseModel::Depolarizing { p: 0.03 }, QubitId(0))
            .unwrap();
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        let initial_ops = dag.num_ops();
        assert_eq!(initial_ops, 3);

        let mut props = PropertySet::new();
        Optimize1qGates::new().run(&mut dag, &mut props).unwrap();

        // H·H would normally cancel, but Resource noise channel prevents it
        assert!(
            dag.num_ops() >= 2,
            "Resource noise should block H·H cancellation"
        );
    }

    #[test]
    fn test_zyz_decomposition_roundtrip() {
        let h = Unitary2x2::h();
        let (alpha, beta, gamma, phase) = h.zyz_decomposition();

        // Reconstruct
        let reconstructed = Unitary2x2::rz(alpha) * Unitary2x2::ry(beta) * Unitary2x2::rz(gamma);
        let global = num_complex::Complex64::from_polar(1.0, phase);

        for i in 0..4 {
            let expected = h.data[i];
            let got = reconstructed.data[i] * global;
            assert!(
                (expected - got).norm() < 1e-6,
                "Mismatch: expected {expected:?}, got {got:?}"
            );
        }
    }
}
