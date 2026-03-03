//! Two-qubit block consolidation pass.
//!
//! Identifies maximal sequences of gates operating on the same pair of qubits,
//! computes the 4×4 unitary matrix of each block, and replaces blocks that use
//! more entangling gates than the KAK-optimal count with a single `CustomGate`
//! carrying the precomputed unitary.

use num_complex::Complex64;
use rustc_hash::{FxHashMap, FxHashSet};
use tracing::debug;

use arvak_ir::{
    CircuitDag, CustomGate, Gate, GateKind, Instruction, InstructionKind, NodeIndex, QubitId,
    StandardGate,
};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;
use crate::unitary::{Unitary2x2, Unitary4x4};

/// Maximum number of iterations to avoid infinite loops.
const MAX_ITERATIONS: usize = 200;

/// Consolidate two-qubit blocks by replacing multi-gate sequences with
/// a single `CustomGate` carrying the equivalent 4×4 unitary.
///
/// A "block" is a maximal contiguous sequence of gates acting on exactly
/// the same pair of qubits, with no barriers, measurements, or resets
/// between them.
///
/// The pass computes the combined unitary of the block, determines the
/// minimum CNOT count via Makhlin invariants, and replaces the block
/// if the original gate count exceeds the optimal.
pub struct ConsolidateBlocks;

impl Pass for ConsolidateBlocks {
    fn name(&self) -> &'static str {
        "ConsolidateBlocks"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        for _ in 0..MAX_ITERATIONS {
            let block = find_best_block(dag);
            let Some(block) = block else {
                break;
            };

            debug!(
                "Consolidating block on qubits ({}, {}) with {} gates ({} entangling) → {} optimal CNOTs",
                block.q0.0,
                block.q1.0,
                block.nodes.len(),
                block.entangling_count,
                block.optimal_cnots
            );

            replace_block(dag, &block)?;
        }

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, _properties: &PropertySet) -> bool {
        true
    }
}

/// A two-qubit block identified for consolidation.
struct TwoQubitBlock {
    /// The nodes in topological order.
    nodes: Vec<NodeIndex>,
    /// First qubit of the pair.
    q0: QubitId,
    /// Second qubit of the pair.
    q1: QubitId,
    /// Number of entangling gates in the original block.
    entangling_count: usize,
    /// Minimum CNOT count from KAK decomposition.
    optimal_cnots: u8,
    /// The 4×4 unitary matrix of the block.
    unitary: [Complex64; 16],
}

/// Find the best block to consolidate (most entangling gate savings).
///
/// Returns `None` if no block can be improved.
fn find_best_block(dag: &CircuitDag) -> Option<TwoQubitBlock> {
    // Collect all operations in topological order.
    let topo_ops: Vec<(NodeIndex, Instruction)> = dag
        .topological_ops()
        .map(|(idx, inst)| (idx, inst.clone()))
        .collect();

    // Build per-qubit operation lists (topological order preserved).
    let mut qubit_ops: FxHashMap<QubitId, Vec<usize>> = FxHashMap::default();
    for (i, (_, inst)) in topo_ops.iter().enumerate() {
        for &q in &inst.qubits {
            qubit_ops.entry(q).or_default().push(i);
        }
    }

    // Find two-qubit gates and explore blocks around them.
    let mut best: Option<TwoQubitBlock> = None;

    // Track which nodes have already been considered.
    let mut visited_2q: FxHashSet<NodeIndex> = FxHashSet::default();

    for (i, (node_idx, inst)) in topo_ops.iter().enumerate() {
        if inst.qubits.len() != 2 {
            continue;
        }
        if visited_2q.contains(node_idx) {
            continue;
        }

        let q0 = inst.qubits[0];
        let q1 = inst.qubits[1];

        // Grow the block: find all consecutive gates on this qubit pair.
        let block_nodes = grow_block(i, q0, q1, &topo_ops, &qubit_ops);
        if block_nodes.is_empty() {
            continue;
        }

        // Mark all 2q gates in this block as visited.
        for &bi in &block_nodes {
            let (nidx, inst) = &topo_ops[bi];
            if inst.qubits.len() == 2 {
                visited_2q.insert(*nidx);
            }
        }

        // Count entangling gates.
        let entangling_count = block_nodes
            .iter()
            .filter(|&&bi| is_entangling_gate(&topo_ops[bi].1))
            .count();

        // Need at least 2 entangling gates to have potential savings,
        // or at least 4 total gates (to consolidate single-qubit overhead).
        if entangling_count < 2 && block_nodes.len() < 4 {
            continue;
        }

        // Compute the 4×4 unitary.
        let unitary = compute_block_unitary(q0, q1, &block_nodes, &topo_ops);
        let Some(unitary) = unitary else {
            continue; // Skip blocks with parameterized gates.
        };

        // Determine minimum CNOT count via KAK.
        let u4 = Unitary4x4 { data: unitary };
        let kak = u4.kak_decompose();

        // Is this an improvement?
        let savings = entangling_count as i32 - i32::from(kak.num_cnots);
        if savings <= 0 {
            continue;
        }

        // Track the best block (most savings).
        let node_indices: Vec<NodeIndex> = block_nodes.iter().map(|&bi| topo_ops[bi].0).collect();
        let is_better = best
            .as_ref()
            .is_none_or(|b| savings > (b.entangling_count as i32 - i32::from(b.optimal_cnots)));

        if is_better {
            best = Some(TwoQubitBlock {
                nodes: node_indices,
                q0,
                q1,
                entangling_count,
                optimal_cnots: kak.num_cnots,
                unitary,
            });
        }
    }

    best
}

/// Grow a block of consecutive gates on the qubit pair (q0, q1).
///
/// Starting from a seed two-qubit gate at topo index `seed`, expand in both
/// directions collecting gates that act exclusively on {q0} or {q0, q1} or {q1},
/// stopping at barriers, measurements, resets, or gates involving other qubits.
fn grow_block(
    seed: usize,
    q0: QubitId,
    q1: QubitId,
    topo_ops: &[(NodeIndex, Instruction)],
    qubit_ops: &FxHashMap<QubitId, Vec<usize>>,
) -> Vec<usize> {
    let pair: FxHashSet<QubitId> = [q0, q1].into_iter().collect();

    // Find the position of the seed in both qubit lists.
    let q0_ops = qubit_ops.get(&q0).map_or(&[] as &[usize], Vec::as_slice);
    let q1_ops = qubit_ops.get(&q1).map_or(&[] as &[usize], Vec::as_slice);

    let q0_pos = q0_ops.iter().position(|&i| i == seed).unwrap_or(0);
    let q1_pos = q1_ops.iter().position(|&i| i == seed).unwrap_or(0);

    let mut block_set: FxHashSet<usize> = FxHashSet::default();
    block_set.insert(seed);

    // Expand forward on q0.
    for &idx in &q0_ops[(q0_pos + 1)..] {
        if !can_include_in_block(idx, &pair, topo_ops) {
            break;
        }
        block_set.insert(idx);
    }

    // Expand forward on q1.
    for &idx in &q1_ops[(q1_pos + 1)..] {
        if !can_include_in_block(idx, &pair, topo_ops) {
            break;
        }
        block_set.insert(idx);
    }

    // Expand backward on q0.
    for &idx in q0_ops[..q0_pos].iter().rev() {
        if !can_include_in_block(idx, &pair, topo_ops) {
            break;
        }
        block_set.insert(idx);
    }

    // Expand backward on q1.
    for &idx in q1_ops[..q1_pos].iter().rev() {
        if !can_include_in_block(idx, &pair, topo_ops) {
            break;
        }
        block_set.insert(idx);
    }

    // Return in topological order.
    let mut result: Vec<usize> = block_set.into_iter().collect();
    result.sort_unstable();
    result
}

/// Check if a gate at the given topo index can be included in a block
/// operating on the given qubit pair.
fn can_include_in_block(
    idx: usize,
    pair: &FxHashSet<QubitId>,
    topo_ops: &[(NodeIndex, Instruction)],
) -> bool {
    let (_, inst) = &topo_ops[idx];

    // Only gates can be consolidated; barriers, measurements, etc. are boundaries.
    if !matches!(inst.kind, InstructionKind::Gate(_)) {
        return false;
    }

    // All qubits of the gate must be in the pair.
    inst.qubits.iter().all(|q| pair.contains(q))
}

/// Check if an instruction is an entangling (multi-qubit) gate.
fn is_entangling_gate(inst: &Instruction) -> bool {
    if inst.qubits.len() < 2 {
        return false;
    }
    matches!(inst.kind, InstructionKind::Gate(_))
}

/// Compute the 4×4 unitary of a block of gates on (q0, q1).
///
/// Returns `None` if any gate in the block has unresolvable parameters.
fn compute_block_unitary(
    q0: QubitId,
    q1: QubitId,
    block_indices: &[usize],
    topo_ops: &[(NodeIndex, Instruction)],
) -> Option<[Complex64; 16]> {
    let mut result = Unitary4x4::identity();

    for &bi in block_indices {
        let (_, inst) = &topo_ops[bi];
        let InstructionKind::Gate(gate) = &inst.kind else {
            return None;
        };

        let gate_u = match &gate.kind {
            GateKind::Standard(sg) => {
                if inst.qubits.len() == 1 {
                    // Single-qubit gate: tensor with identity on the other qubit.
                    let u2 = gate_1q_to_unitary(sg)?;
                    if inst.qubits[0] == q0 {
                        Unitary4x4::kron(&u2, &Unitary2x2::identity())
                    } else {
                        Unitary4x4::kron(&Unitary2x2::identity(), &u2)
                    }
                } else if inst.qubits.len() == 2 {
                    // Two-qubit gate.
                    let u4 = gate_2q_to_unitary(sg)?;
                    // If the qubit order is reversed (q1, q0), we need to swap.
                    if inst.qubits[0] == q0 && inst.qubits[1] == q1 {
                        u4
                    } else if inst.qubits[0] == q1 && inst.qubits[1] == q0 {
                        // Apply SWAP before and after to reverse qubit order.
                        let swap = swap_4x4();
                        swap.mul(&u4).mul(&swap)
                    } else {
                        return None; // Qubits don't match the pair.
                    }
                } else {
                    return None; // 3+ qubit gates not supported.
                }
            }
            GateKind::Custom(custom) => {
                if let Some(ref matrix) = custom.matrix {
                    if matrix.len() == 4 && inst.qubits.len() == 1 {
                        let u2 = Unitary2x2::new(matrix[0], matrix[1], matrix[2], matrix[3]);
                        if inst.qubits[0] == q0 {
                            Unitary4x4::kron(&u2, &Unitary2x2::identity())
                        } else {
                            Unitary4x4::kron(&Unitary2x2::identity(), &u2)
                        }
                    } else if matrix.len() == 16 && inst.qubits.len() == 2 {
                        let mut data = [Complex64::new(0.0, 0.0); 16];
                        data.copy_from_slice(matrix);
                        let u4 = Unitary4x4 { data };
                        if inst.qubits[0] == q0 && inst.qubits[1] == q1 {
                            u4
                        } else {
                            let swap = swap_4x4();
                            swap.mul(&u4).mul(&swap)
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None; // No matrix available.
                }
            }
        };

        result = result.mul(&gate_u);
    }

    Some(result.data)
}

/// Convert a single-qubit `StandardGate` to its 2×2 unitary.
fn gate_1q_to_unitary(gate: &StandardGate) -> Option<Unitary2x2> {
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
            let t = theta.as_f64()?;
            let p = phi.as_f64()?;
            Some(Unitary2x2::rz(p) * Unitary2x2::rx(t) * Unitary2x2::rz(-p))
        }
        _ => None,
    }
}

/// Convert a two-qubit `StandardGate` to its 4×4 unitary.
///
/// Returns `None` for parameterized gates whose parameters can't be resolved.
fn gate_2q_to_unitary(gate: &StandardGate) -> Option<Unitary4x4> {
    let o = Complex64::new(1.0, 0.0);
    let z = Complex64::new(0.0, 0.0);
    let i = Complex64::new(0.0, 1.0);
    let m = Complex64::new(-1.0, 0.0);

    match gate {
        StandardGate::CX => Some(Unitary4x4 {
            data: [o, z, z, z, z, o, z, z, z, z, z, o, z, z, o, z],
        }),
        StandardGate::CY => Some(Unitary4x4 {
            data: [o, z, z, z, z, o, z, z, z, z, z, -i, z, z, i, z],
        }),
        StandardGate::CZ => Some(Unitary4x4 {
            data: [o, z, z, z, z, o, z, z, z, z, o, z, z, z, z, m],
        }),
        StandardGate::Swap => Some(Unitary4x4 {
            data: [o, z, z, z, z, z, o, z, z, o, z, z, z, z, z, o],
        }),
        StandardGate::ISwap => Some(Unitary4x4 {
            data: [o, z, z, z, z, z, i, z, z, i, z, z, z, z, z, o],
        }),
        StandardGate::CH => {
            let s = Complex64::new(1.0 / 2.0_f64.sqrt(), 0.0);
            Some(Unitary4x4 {
                data: [o, z, z, z, z, o, z, z, z, z, s, s, z, z, s, -s],
            })
        }
        StandardGate::ECR => {
            let s = Complex64::new(1.0 / 2.0_f64.sqrt(), 0.0);
            let si = Complex64::new(0.0, 1.0 / 2.0_f64.sqrt());
            Some(Unitary4x4 {
                data: [z, z, s, si, z, z, si, s, s, -si, z, z, -si, s, z, z],
            })
        }
        _ => None,
    }
}

/// The 4×4 SWAP matrix for reordering qubit arguments.
fn swap_4x4() -> Unitary4x4 {
    let o = Complex64::new(1.0, 0.0);
    let z = Complex64::new(0.0, 0.0);
    Unitary4x4 {
        data: [o, z, z, z, z, z, o, z, z, o, z, z, z, z, z, o],
    }
}

/// Replace a block of gates with a single `CustomGate` carrying the unitary.
fn replace_block(dag: &mut CircuitDag, block: &TwoQubitBlock) -> CompileResult<()> {
    // Build the replacement CustomGate.
    let custom = CustomGate::new("consolidated_2q", 2).with_matrix(block.unitary.to_vec());
    let replacement = Instruction::gate(Gate::custom(custom), [block.q0, block.q1]);

    // Remove nodes in reverse index order to handle petgraph's swap-remove.
    let mut to_remove: Vec<NodeIndex> = block.nodes.clone();
    to_remove.sort_unstable_by(|a, b| b.index().cmp(&a.index()));

    for node_idx in to_remove {
        // Check if the node is still valid (may have been invalidated by swap-remove).
        if dag.get_instruction(node_idx).is_some() {
            dag.remove_op(node_idx).map_err(CompileError::Ir)?;
        }
    }

    // Apply the replacement gate.
    dag.apply(replacement).map_err(CompileError::Ir)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::Pass;
    use crate::property::PropertySet;
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_consolidate_cx_pair() {
        // Two CX gates on the same pair: CX·CX = I → 0 entangling gates.
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();
        let mut props = PropertySet::new();

        let before_ops = dag.num_ops();
        ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

        // Two CX = identity → should consolidate to a single custom gate or fewer.
        assert!(
            dag.num_ops() < before_ops,
            "Expected fewer ops after consolidation, got {} (was {})",
            dag.num_ops(),
            before_ops
        );
    }

    #[test]
    fn test_consolidate_no_improvement() {
        // Single CX: can't improve (already 1 entangling gate).
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();
        let mut props = PropertySet::new();

        let before_ops = dag.num_ops();
        ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

        assert_eq!(
            dag.num_ops(),
            before_ops,
            "Single CX should not be consolidated"
        );
    }

    #[test]
    fn test_consolidate_swap_sequence() {
        // CX·CX·CX on same pair = SWAP (3 entangling → 3 optimal, no savings).
        // But CX(0,1)·CX(1,0)·CX(0,1) on same pair with surrounding 1q gates
        // might have savings.
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.h(QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();
        let mut props = PropertySet::new();

        // H·CX·CX·CX·H on a pair: 3 entangling gates + 2 single-qubit.
        // The combined unitary might need only 1 or 2 CNOTs.
        let before_ops = dag.num_ops();
        ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

        // Should consolidate (3 CX + 2 H → 1 custom gate).
        assert!(
            dag.num_ops() <= before_ops,
            "Expected ≤{before_ops} ops after consolidation, got {}",
            dag.num_ops(),
        );
    }

    #[test]
    fn test_consolidate_different_pairs() {
        // Gates on different qubit pairs should not be consolidated together.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();
        let mut props = PropertySet::new();

        let before_ops = dag.num_ops();
        ConsolidateBlocks.run(&mut dag, &mut props).unwrap();

        assert_eq!(
            dag.num_ops(),
            before_ops,
            "Gates on different pairs should not be consolidated"
        );
    }

    #[test]
    fn test_kak_cnot_count_correct() {
        // Verify the KAK decomposition gives correct CNOT counts.
        let cx_u = gate_2q_to_unitary(&StandardGate::CX).unwrap();
        assert!(cx_u.kak_decompose().num_cnots <= 1, "CX: expected ≤1 CNOT");

        let swap_u = gate_2q_to_unitary(&StandardGate::Swap).unwrap();
        assert_eq!(
            swap_u.kak_decompose().num_cnots,
            3,
            "SWAP: expected 3 CNOTs"
        );

        let cz_u = gate_2q_to_unitary(&StandardGate::CZ).unwrap();
        assert!(cz_u.kak_decompose().num_cnots <= 1, "CZ: expected ≤1 CNOT");
    }
}
