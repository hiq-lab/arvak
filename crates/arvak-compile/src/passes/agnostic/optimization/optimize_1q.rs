//! Single-qubit gate optimization pass.

use std::f64::consts::PI;
use std::sync::LazyLock;

use arvak_ir::CircuitDag;
use arvak_ir::dag::NodeIndex;
use arvak_ir::gate::{GateKind, StandardGate};
use arvak_ir::instruction::{Instruction, InstructionKind};
use arvak_ir::noise::NoiseRole;
use arvak_ir::parameter::ParameterExpression;
use arvak_ir::qubit::QubitId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;
use crate::unitary::Unitary2x2;

use super::EPSILON;

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
    ///
    /// Constant gates (I, X, Y, Z, H, S, Sdg, T, Tdg, SX, SXdg) use
    /// pre-computed cached matrices to avoid recomputing trig functions.
    fn gate_to_unitary(gate: &StandardGate) -> Option<Unitary2x2> {
        // Pre-computed unitaries for constant gates (computed once, reused forever).
        static U_I: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::identity);
        static U_X: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::x);
        static U_Y: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::y);
        static U_Z: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::z);
        static U_H: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::h);
        static U_S: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::s);
        static U_SDG: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::sdg);
        static U_T: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::t);
        static U_TDG: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::tdg);
        static U_SX: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::sx);
        static U_SXDG: LazyLock<Unitary2x2> = LazyLock::new(Unitary2x2::sxdg);

        match gate {
            StandardGate::I => Some(*U_I),
            StandardGate::X => Some(*U_X),
            StandardGate::Y => Some(*U_Y),
            StandardGate::Z => Some(*U_Z),
            StandardGate::H => Some(*U_H),
            StandardGate::S => Some(*U_S),
            StandardGate::Sdg => Some(*U_SDG),
            StandardGate::T => Some(*U_T),
            StandardGate::Tdg => Some(*U_TDG),
            StandardGate::SX => Some(*U_SX),
            StandardGate::SXdg => Some(*U_SXDG),
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
        // RY(β) = RZ(π) · SX · RZ(β + π) · SX   (up to global phase)
        // So: RZ(α) · RY(β) · RZ(γ)
        //   = RZ(α + π) · SX · RZ(β + π) · SX · RZ(γ)
        // (Verified numerically; the previous identity
        //  RZ(π/2)·SX·RZ(β)·SX·RZ(-π/2) is NOT equal to RY(β).)

        let mut gates = Vec::new();

        if beta.abs() < EPSILON {
            // Pure Z rotation
            let total_z = alpha + gamma;
            if total_z.abs() > EPSILON {
                gates.push(StandardGate::Rz(ParameterExpression::constant(total_z)));
            }
        } else {
            // Full decomposition
            let z1 = gamma;
            let z2 = Unitary2x2::normalize_angle(beta + PI);
            let z3 = Unitary2x2::normalize_angle(alpha + PI);

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
    ///
    /// Computes the topological order once and indexes operations by qubit,
    /// avoiding the previous O(num_qubits * V+E) pattern.
    #[allow(clippy::unused_self)]
    fn find_1q_runs(&self, dag: &CircuitDag) -> Vec<(QubitId, Vec<NodeIndex>)> {
        // Compute topological order ONCE — previously this was called per-qubit.
        let topo_ops: Vec<_> = dag.topological_ops().collect();

        // Build per-qubit operation lists from the single topo pass.
        let mut qubit_ops: FxHashMap<QubitId, Vec<(NodeIndex, &Instruction)>> =
            FxHashMap::default();
        for &(node_idx, inst) in &topo_ops {
            for &qubit in &inst.qubits {
                qubit_ops.entry(qubit).or_default().push((node_idx, inst));
            }
        }

        let mut runs = Vec::new();
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        for (qubit, ops) in &qubit_ops {
            let mut current_run: Vec<NodeIndex> = Vec::new();

            for &(node_idx, inst) in ops {
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
                        runs.push((*qubit, std::mem::take(&mut current_run)));
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
                    runs.push((*qubit, std::mem::take(&mut current_run)));
                } else {
                    current_run.clear();
                }
            }

            // Don't forget the final run
            if current_run.len() > 1 {
                for &idx in &current_run {
                    visited.insert(idx);
                }
                runs.push((*qubit, current_run));
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
        // Process one run at a time, re-discovering runs after each modification.
        // petgraph's remove_node uses swap-remove, which invalidates the last
        // node's NodeIndex. Re-discovering after each modification ensures all
        // indices are fresh.
        //
        // Only strictly-improving replacements (fewer gates than the run) are
        // applied. Each application removes at least one op, so the loop
        // terminates after at most `num_ops` iterations — no iteration cap
        // that could leave the circuit in a half-optimized state.
        let max_iterations = dag.num_ops().max(1);
        for _ in 0..max_iterations {
            let runs = self.find_1q_runs(dag);

            let mut applied = false;
            for (qubit, nodes) in runs {
                if nodes.len() < 2 {
                    continue;
                }

                // Compute the combined unitary. Gates are applied in
                // topological order, so with column-vector convention the
                // total unitary is U_last · … · U_first: multiply each new
                // gate on the LEFT of the accumulator.
                let mut combined = Unitary2x2::identity();
                for &node_idx in &nodes {
                    if let Some(inst) = dag.get_instruction(node_idx) {
                        if let InstructionKind::Gate(gate) = &inst.kind {
                            if let GateKind::Standard(std_gate) = &gate.kind {
                                if let Some(u) = Self::gate_to_unitary(std_gate) {
                                    combined = u * combined;
                                }
                            }
                        }
                    }
                }

                // Decompose to a minimal gate sequence.
                let new_gates = self.decompose_unitary(&combined);
                let num_new = new_gates.len();

                // Only replace when strictly shorter. Replacing with the same
                // length re-creates an actionable run and would loop forever;
                // replacing with a longer sequence (possible: a 2-gate run can
                // need 3 ZYZ gates) must keep the original — truncating it
                // would silently change circuit semantics.
                if num_new >= nodes.len() {
                    continue;
                }

                if num_new == 0 {
                    // All gates cancel - remove all nodes in this run.
                    // Sort by descending index so swap-remove never invalidates
                    // a remaining node (the last node IS the one being removed).
                    let mut to_remove = nodes;
                    to_remove.sort_unstable_by(|a, b| b.index().cmp(&a.index()));
                    for node_idx in to_remove {
                        dag.remove_op(node_idx).map_err(CompileError::Ir)?;
                    }
                } else {
                    // Replace in-place: update first N nodes, remove the rest.
                    let (keep, remove) = nodes.split_at(num_new);

                    // Update kept nodes (no removal, so indices remain valid)
                    for (&node_idx, gate) in keep.iter().zip(new_gates) {
                        if let Some(inst) = dag.get_instruction_mut(node_idx) {
                            *inst = Instruction::single_qubit_gate(gate, qubit);
                        }
                    }

                    // Remove extra nodes in descending index order to avoid
                    // swap-remove invalidation.
                    let mut to_remove: Vec<NodeIndex> = remove.to_vec();
                    to_remove.sort_unstable_by(|a, b| b.index().cmp(&a.index()));
                    for node_idx in to_remove {
                        dag.remove_op(node_idx).map_err(CompileError::Ir)?;
                    }
                }

                // Node indices are stale after removals — re-discover runs.
                applied = true;
                break;
            }

            if !applied {
                break; // Fixed point reached.
            }
        }

        Ok(())
    }

    fn should_run(&self, dag: &CircuitDag, _properties: &PropertySet) -> bool {
        // Only run if there are operations to optimize
        dag.num_ops() > 1
    }
}
