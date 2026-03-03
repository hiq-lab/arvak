//! SABRE routing pass for inserting SWAP gates.
//!
//! Implements the SABRE algorithm (SWAP-based Bidirectional heuristic search)
//! from Li et al., "Tackling the Qubit Mapping Problem for NISQ-Era Quantum
//! Devices" (ASPLOS 2019).
//!
//! Key ideas:
//! - Maintain a "front layer" of executable gates (all dependencies satisfied)
//! - Score candidate SWAPs by how much they reduce the total distance of
//!   front-layer gates, with a lookahead into upcoming gates
//! - Run the algorithm in both forward and reverse direction, keep the
//!   result with fewer SWAPs

use rustc_hash::{FxHashMap, FxHashSet};

use arvak_ir::{CircuitDag, Instruction, QubitId, StandardGate};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::{CouplingMap, Layout, PropertySet};

/// SABRE routing pass.
///
/// Uses the SABRE heuristic to insert fewer SWAP gates than [`BasicRouting`].
/// The algorithm considers a "front layer" of ready gates and a lookahead
/// "extended set" when choosing which SWAP to insert. It runs bidirectionally
/// and keeps the better result.
///
/// [`BasicRouting`]: super::BasicRouting
pub struct SabreRouting {
    /// Weight for the extended set in the cost function (0.0–1.0).
    /// Higher values give more weight to lookahead.
    extended_set_weight: f64,
    /// Maximum size of the extended set (number of gates to look ahead).
    extended_set_size: usize,
}

impl SabreRouting {
    /// Create a new SABRE routing pass with default parameters.
    pub fn new() -> Self {
        Self {
            extended_set_weight: 0.5,
            extended_set_size: 20,
        }
    }

    /// Create a SABRE routing pass with custom parameters.
    pub fn with_params(extended_set_weight: f64, extended_set_size: usize) -> Self {
        Self {
            extended_set_weight,
            extended_set_size,
        }
    }
}

impl Default for SabreRouting {
    fn default() -> Self {
        Self::new()
    }
}

/// A two-qubit gate with its logical qubits and original instruction.
#[derive(Clone)]
struct TwoQubitGate {
    /// Logical qubit operands (control, target).
    q0: QubitId,
    q1: QubitId,
    /// Index in the gate list.
    index: usize,
}

/// Run one direction of the SABRE algorithm.
///
/// Returns the sequence of emitted instructions (with physical qubit labels)
/// and the number of inserted SWAPs.
#[allow(clippy::too_many_lines)]
fn sabre_pass(
    ops: &[Instruction],
    initial_layout: &Layout,
    coupling_map: &CouplingMap,
    extended_set_weight: f64,
    extended_set_size: usize,
) -> CompileResult<(Vec<Instruction>, Layout, usize)> {
    let mut layout = initial_layout.clone();
    let mut emitted: Vec<Instruction> = Vec::new();
    let mut swap_count: usize = 0;

    // Build dependency graph: for each two-qubit gate, track which gates
    // must execute before it (on the same qubit).
    let num_ops = ops.len();

    // For each qubit, track which two-qubit gate indices touch it, in order.
    let mut qubit_gate_list: FxHashMap<QubitId, Vec<usize>> = FxHashMap::default();
    let mut two_qubit_gates: Vec<TwoQubitGate> = Vec::new();
    let mut gate_index_map: Vec<Option<usize>> = vec![None; num_ops];

    for (i, inst) in ops.iter().enumerate() {
        if inst.qubits.len() == 2 {
            let tq_idx = two_qubit_gates.len();
            two_qubit_gates.push(TwoQubitGate {
                q0: inst.qubits[0],
                q1: inst.qubits[1],
                index: i,
            });
            gate_index_map[i] = Some(tq_idx);
            qubit_gate_list
                .entry(inst.qubits[0])
                .or_default()
                .push(tq_idx);
            qubit_gate_list
                .entry(inst.qubits[1])
                .or_default()
                .push(tq_idx);
        }
    }

    if two_qubit_gates.is_empty() {
        // No two-qubit gates — just remap and emit everything.
        for inst in ops {
            let mut remapped = inst.clone();
            remapped.qubits = remapped
                .qubits
                .iter()
                .map(|&q| {
                    let p = layout.get_physical(q).ok_or(CompileError::MissingLayout)?;
                    Ok(QubitId(p))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            emitted.push(remapped);
        }
        return Ok((emitted, layout, 0));
    }

    // Track which two-qubit gate is "next" on each qubit.
    let mut qubit_gate_cursor: FxHashMap<QubitId, usize> = FxHashMap::default();
    for &q in qubit_gate_list.keys() {
        qubit_gate_cursor.insert(q, 0);
    }

    // Dependencies: a two-qubit gate is "ready" when it is the next gate
    // on both of its qubits.
    let mut resolved = vec![false; two_qubit_gates.len()];

    // Track which original ops have been emitted.
    let mut emitted_ops = vec![false; num_ops];

    // Track how far we've scanned through ops for single-qubit gates.
    let mut ops_cursor = 0;

    let is_gate_ready =
        |tq_idx: usize, qubit_gate_cursor: &FxHashMap<QubitId, usize>, resolved: &[bool]| -> bool {
            if resolved[tq_idx] {
                return false;
            }
            let gate = &two_qubit_gates[tq_idx];
            let cursor_q0 = qubit_gate_cursor.get(&gate.q0).copied().unwrap_or(0);
            let cursor_q1 = qubit_gate_cursor.get(&gate.q1).copied().unwrap_or(0);
            let list_q0 = &qubit_gate_list[&gate.q0];
            let list_q1 = &qubit_gate_list[&gate.q1];
            cursor_q0 < list_q0.len()
                && list_q0[cursor_q0] == tq_idx
                && cursor_q1 < list_q1.len()
                && list_q1[cursor_q1] == tq_idx
        };

    // Build initial front layer.
    let mut front_layer: Vec<usize> = Vec::new();
    for tq_idx in 0..two_qubit_gates.len() {
        if is_gate_ready(tq_idx, &qubit_gate_cursor, &resolved) {
            front_layer.push(tq_idx);
        }
    }

    // Helper: emit all single-qubit ops up to (but not including) the
    // next unresolved two-qubit gate, plus any resolved two-qubit gate
    // that is executable (adjacent in current layout).
    let emit_pending_ops = |ops_cursor: &mut usize,
                            emitted: &mut Vec<Instruction>,
                            emitted_ops: &mut [bool],
                            layout: &Layout|
     -> CompileResult<()> {
        while *ops_cursor < num_ops {
            if emitted_ops[*ops_cursor] {
                *ops_cursor += 1;
                continue;
            }
            let inst = &ops[*ops_cursor];
            // Only emit single-qubit and non-gate ops here.
            if inst.qubits.len() >= 2 {
                break;
            }
            let mut remapped = inst.clone();
            remapped.qubits = remapped
                .qubits
                .iter()
                .map(|&q| {
                    let p = layout.get_physical(q).ok_or(CompileError::MissingLayout)?;
                    Ok(QubitId(p))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            emitted.push(remapped);
            emitted_ops[*ops_cursor] = true;
            *ops_cursor += 1;
        }
        Ok(())
    };

    // Main loop.
    while !front_layer.is_empty() {
        // Emit any pending single-qubit ops.
        emit_pending_ops(&mut ops_cursor, &mut emitted, &mut emitted_ops, &layout)?;

        // Try to execute gates in the front layer that are already adjacent.
        let mut executed_any = true;
        while executed_any {
            executed_any = false;
            let mut remaining_front: Vec<usize> = Vec::new();
            for &tq_idx in &front_layer {
                let gate = &two_qubit_gates[tq_idx];
                let p0 = layout
                    .get_physical(gate.q0)
                    .ok_or(CompileError::MissingLayout)?;
                let p1 = layout
                    .get_physical(gate.q1)
                    .ok_or(CompileError::MissingLayout)?;

                if coupling_map.is_connected(p0, p1) {
                    // Execute this gate.
                    let mut remapped = ops[gate.index].clone();
                    remapped.qubits = vec![QubitId(p0), QubitId(p1)];
                    emitted.push(remapped);
                    emitted_ops[gate.index] = true;

                    // Mark resolved and advance cursors.
                    resolved[tq_idx] = true;
                    if let Some(c) = qubit_gate_cursor.get_mut(&gate.q0) {
                        *c += 1;
                    }
                    if let Some(c) = qubit_gate_cursor.get_mut(&gate.q1) {
                        *c += 1;
                    }
                    executed_any = true;
                } else {
                    remaining_front.push(tq_idx);
                }
            }
            front_layer = remaining_front;

            // After resolving some gates, new gates may become ready.
            if executed_any {
                for tq_idx in 0..two_qubit_gates.len() {
                    if !resolved[tq_idx]
                        && !front_layer.contains(&tq_idx)
                        && is_gate_ready(tq_idx, &qubit_gate_cursor, &resolved)
                    {
                        front_layer.push(tq_idx);
                    }
                }
            }
        }

        // If the front layer is empty, we're done.
        if front_layer.is_empty() {
            break;
        }

        // Build the extended set: next gates after the front layer.
        let mut extended_set: Vec<usize> = Vec::new();
        {
            let front_set: FxHashSet<usize> = front_layer.iter().copied().collect();
            for tq_idx in 0..two_qubit_gates.len() {
                if !resolved[tq_idx]
                    && !front_set.contains(&tq_idx)
                    && extended_set.len() < extended_set_size
                {
                    // Check if this gate depends only on front-layer or resolved gates.
                    let gate = &two_qubit_gates[tq_idx];
                    let q0_next = qubit_gate_cursor.get(&gate.q0).copied().unwrap_or(0);
                    let q1_next = qubit_gate_cursor.get(&gate.q1).copied().unwrap_or(0);
                    let q0_list = qubit_gate_list.get(&gate.q0);
                    let q1_list = qubit_gate_list.get(&gate.q1);

                    let q0_close = q0_list.is_none_or(|list| {
                        q0_next < list.len() && {
                            let blocking = list[q0_next];
                            front_set.contains(&blocking) || blocking == tq_idx
                        }
                    });
                    let q1_close = q1_list.is_none_or(|list| {
                        q1_next < list.len() && {
                            let blocking = list[q1_next];
                            front_set.contains(&blocking) || blocking == tq_idx
                        }
                    });

                    if q0_close || q1_close {
                        extended_set.push(tq_idx);
                    }
                }
            }
        }

        // Score each candidate SWAP and pick the best.
        // Candidates: any SWAP on an edge where at least one endpoint
        // has a mapped qubit involved in a front-layer gate.
        let mut best_swap: Option<(u32, u32)> = None;
        let mut best_score = f64::MAX;

        // Collect physical qubits involved in front-layer gates.
        let mut front_physical: FxHashSet<u32> = FxHashSet::default();
        for &tq_idx in &front_layer {
            let gate = &two_qubit_gates[tq_idx];
            if let Some(p) = layout.get_physical(gate.q0) {
                front_physical.insert(p);
            }
            if let Some(p) = layout.get_physical(gate.q1) {
                front_physical.insert(p);
            }
        }

        // Evaluate candidate SWAPs.
        for &phys in &front_physical {
            for neighbor in coupling_map.neighbors(phys) {
                // Score = front_layer_cost + weight * extended_set_cost
                // after applying this candidate SWAP.

                // Tentatively swap in layout.
                let mut trial_layout = layout.clone();
                trial_layout.swap(phys, neighbor);

                let front_cost: f64 = front_layer
                    .iter()
                    .map(|&tq_idx| {
                        let gate = &two_qubit_gates[tq_idx];
                        let p0 = trial_layout.get_physical(gate.q0).unwrap_or(0);
                        let p1 = trial_layout.get_physical(gate.q1).unwrap_or(0);
                        f64::from(coupling_map.distance(p0, p1).unwrap_or(u32::MAX))
                    })
                    .sum();

                let extended_cost: f64 = extended_set
                    .iter()
                    .map(|&tq_idx| {
                        let gate = &two_qubit_gates[tq_idx];
                        let p0 = trial_layout.get_physical(gate.q0).unwrap_or(0);
                        let p1 = trial_layout.get_physical(gate.q1).unwrap_or(0);
                        f64::from(coupling_map.distance(p0, p1).unwrap_or(u32::MAX))
                    })
                    .sum();

                let score = front_cost + extended_set_weight * extended_cost;

                if score < best_score {
                    best_score = score;
                    best_swap = Some((phys, neighbor));
                }
            }
        }

        // Apply the best SWAP.
        let (sp1, sp2) = best_swap.ok_or(CompileError::PassFailed {
            name: "SabreRouting".into(),
            reason: "no valid SWAP candidate found".into(),
        })?;

        emitted.push(Instruction::two_qubit_gate(
            StandardGate::Swap,
            QubitId(sp1),
            QubitId(sp2),
        ));
        layout.swap(sp1, sp2);
        swap_count += 1;
    }

    // Emit remaining single-qubit ops and any other ops not yet emitted.
    for (i, inst) in ops.iter().enumerate() {
        if emitted_ops[i] {
            continue;
        }
        let mut remapped = inst.clone();
        remapped.qubits = remapped
            .qubits
            .iter()
            .map(|&q| {
                let p = layout.get_physical(q).ok_or(CompileError::MissingLayout)?;
                Ok(QubitId(p))
            })
            .collect::<CompileResult<Vec<_>>>()?;
        emitted.push(remapped);
    }

    Ok((emitted, layout, swap_count))
}

impl Pass for SabreRouting {
    fn name(&self) -> &'static str {
        "SabreRouting"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    #[allow(clippy::similar_names)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let coupling_map = properties
            .coupling_map
            .as_ref()
            .ok_or(CompileError::MissingCouplingMap)?;

        let layout = properties
            .layout
            .as_ref()
            .ok_or(CompileError::MissingLayout)?;

        // Collect all instructions in topological order.
        let ops: Vec<Instruction> = dag
            .topological_ops()
            .map(|(_, inst)| inst.clone())
            .collect();

        // Forward pass.
        let (fwd_ops, fwd_layout, fwd_swaps) = sabre_pass(
            &ops,
            layout,
            coupling_map,
            self.extended_set_weight,
            self.extended_set_size,
        )?;

        // Reverse pass: reverse the instruction order, run SABRE again,
        // then reverse the output.
        let mut rev_input: Vec<Instruction> = ops;
        rev_input.reverse();

        let (mut rev_ops, rev_layout, rev_swaps) = sabre_pass(
            &rev_input,
            layout,
            coupling_map,
            self.extended_set_weight,
            self.extended_set_size,
        )?;
        rev_ops.reverse();

        // Pick the direction with fewer SWAPs.
        let (chosen_ops, chosen_layout) = if fwd_swaps <= rev_swaps {
            (fwd_ops, fwd_layout)
        } else {
            (rev_ops, rev_layout)
        };

        // Build the new DAG with physical qubit wires.
        let mut new_dag = CircuitDag::new();

        // Add physical qubit wires for all qubits in the coupling map that
        // are referenced in the output.
        let mut used_qubits: FxHashSet<u32> = FxHashSet::default();
        for inst in &chosen_ops {
            for &q in &inst.qubits {
                used_qubits.insert(q.0);
            }
        }
        // Also ensure all initially-mapped qubits are present.
        for (_, phys) in layout.iter() {
            used_qubits.insert(phys);
        }
        for &p in &used_qubits {
            new_dag.add_qubit(QubitId(p));
        }
        for clbit in dag.clbits().collect::<Vec<_>>() {
            new_dag.add_clbit(clbit);
        }

        for inst in chosen_ops {
            new_dag.apply(inst).map_err(CompileError::Ir)?;
        }

        new_dag.set_global_phase(dag.global_phase());
        new_dag.set_level(dag.level());
        *dag = new_dag;

        // Update layout.
        properties.layout = Some(chosen_layout);

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties.coupling_map.is_some() && properties.layout.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::TrivialLayout;
    use crate::property::{BasisGates, CouplingMap};
    use arvak_ir::{Circuit, InstructionKind, QubitId};

    /// Helper: count SWAP gates in a DAG.
    fn count_swaps(dag: &CircuitDag) -> usize {
        dag.topological_ops()
            .filter(
                |(_, inst)| matches!(&inst.kind, InstructionKind::Gate(g) if g.name() == "swap"),
            )
            .count()
    }

    /// Helper: verify all two-qubit gates act on adjacent physical qubits.
    fn assert_all_adjacent(dag: &CircuitDag, coupling_map: &CouplingMap) {
        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                assert!(
                    coupling_map.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "two-qubit gate {} on non-adjacent physical qubits ({}, {})",
                    match &inst.kind {
                        InstructionKind::Gate(g) => g.name().to_string(),
                        _ => "unknown".into(),
                    },
                    inst.qubits[0].0,
                    inst.qubits[1].0,
                );
            }
        }
    }

    /// Helper: count gates of a given name.
    fn count_gates(dag: &CircuitDag, name: &str) -> usize {
        dag.topological_ops()
            .filter(|(_, inst)| matches!(&inst.kind, InstructionKind::Gate(g) if g.name() == name))
            .count()
    }

    #[test]
    fn test_sabre_adjacent_no_swap() {
        // CX on adjacent qubits (q0, q1) on linear(5) — no SWAP needed.
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(count_swaps(&dag), 0, "no SWAPs needed for adjacent qubits");
        assert_eq!(count_gates(&dag, "h"), 1, "H gate preserved");
        assert_eq!(count_gates(&dag, "cx"), 1, "CX gate preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_distance_2() {
        // CX on q0, q2 on linear(5) — distance 2, needs exactly 1 SWAP.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        // Must have exactly 1 SWAP to bring them adjacent.
        assert_eq!(
            count_swaps(&dag),
            1,
            "expected 1 SWAP for distance-2 qubits"
        );
        assert_eq!(count_gates(&dag, "cx"), 1, "CX gate preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_distance_3() {
        // CX on q0, q3 on linear(5) — distance 3, needs 2 SWAPs.
        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.cx(QubitId(0), QubitId(3)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(
            count_swaps(&dag),
            2,
            "expected 2 SWAPs for distance-3 qubits"
        );
        assert_eq!(count_gates(&dag, "cx"), 1, "CX gate preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_fewer_swaps_than_basic() {
        // Circuit with multiple CX gates — SABRE should use the same or
        // fewer SWAPs than BasicRouting because the lookahead prevents
        // greedy local decisions that cause downstream problems.
        //
        // On linear(5): CX(0,4), CX(1,3), CX(0,2)
        // BasicRouting processes greedily: 3+1+? SWAPs
        // SABRE can reuse layout changes from earlier SWAPs.
        let mut circuit = Circuit::with_size("test", 5, 0);
        circuit.cx(QubitId(0), QubitId(4)).unwrap();
        circuit.cx(QubitId(1), QubitId(3)).unwrap();
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        let mut dag_sabre = circuit.dag().clone();
        let mut dag_basic = circuit.into_dag();

        // Run BasicRouting.
        let mut props_basic =
            PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag_basic, &mut props_basic).unwrap();
        crate::passes::BasicRouting
            .run(&mut dag_basic, &mut props_basic)
            .unwrap();
        let basic_swaps = count_swaps(&dag_basic);

        // Run SabreRouting.
        let mut props_sabre =
            PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag_sabre, &mut props_sabre).unwrap();
        SabreRouting::new()
            .run(&mut dag_sabre, &mut props_sabre)
            .unwrap();
        let sabre_swaps = count_swaps(&dag_sabre);

        // SABRE should use at most as many SWAPs as BasicRouting.
        assert!(
            sabre_swaps <= basic_swaps,
            "SABRE ({sabre_swaps} SWAPs) should be <= BasicRouting ({basic_swaps} SWAPs)"
        );

        // Both must produce correct (adjacent) output.
        assert_all_adjacent(&dag_basic, props_basic.coupling_map.as_ref().unwrap());
        assert_all_adjacent(&dag_sabre, props_sabre.coupling_map.as_ref().unwrap());

        // Both must preserve all 3 CX gates.
        assert_eq!(count_gates(&dag_basic, "cx"), 3);
        assert_eq!(count_gates(&dag_sabre, "cx"), 3);
    }

    #[test]
    fn test_sabre_preserves_single_qubit_gates() {
        // Circuit with H + CX(non-adjacent) + Z — all gates must be preserved.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        circuit.z(QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(count_gates(&dag, "h"), 1, "H gate preserved");
        assert_eq!(count_gates(&dag, "cx"), 1, "CX gate preserved");
        assert_eq!(count_gates(&dag, "z"), 1, "Z gate preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_no_two_qubit_gates() {
        // Circuit with only single-qubit gates — no SWAPs needed.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.x(QubitId(1)).unwrap();
        circuit.z(QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(count_swaps(&dag), 0);
        assert_eq!(dag.num_ops(), 3);
    }

    #[test]
    fn test_sabre_star_topology() {
        // Star topology: center q0 connected to all others.
        // CX(1,2) requires routing through center.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::star(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        // Star: 1-0, 2-0 => distance(1,2)=2, need 1 SWAP.
        assert_eq!(count_swaps(&dag), 1);
        assert_eq!(count_gates(&dag, "cx"), 1);
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_full_connectivity_no_swaps() {
        // Full connectivity — no SWAPs ever needed.
        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.cx(QubitId(0), QubitId(3)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::full(4), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(count_swaps(&dag), 0);
        assert_eq!(count_gates(&dag, "cx"), 2);
    }

    #[test]
    fn test_sabre_with_measurements() {
        // Circuit with measurements — must be preserved through routing.
        let mut circuit = Circuit::with_size("test", 3, 3);
        circuit.cx(QubitId(0), QubitId(2)).unwrap();
        circuit.measure(QubitId(0), arvak_ir::ClbitId(0)).unwrap();
        circuit.measure(QubitId(2), arvak_ir::ClbitId(2)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        let measure_count = dag
            .topological_ops()
            .filter(|(_, inst)| inst.is_measure())
            .count();
        assert_eq!(measure_count, 2, "measurements preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_ghz_circuit() {
        // GHZ(5) on linear(5): H(0), CX(0,1), CX(1,2), CX(2,3), CX(3,4)
        // All CX pairs are adjacent on linear topology — no SWAPs needed.
        let circuit = Circuit::ghz(5).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_eq!(
            count_swaps(&dag),
            0,
            "GHZ on matching topology needs 0 SWAPs"
        );
        assert_eq!(count_gates(&dag, "h"), 1);
        assert_eq!(count_gates(&dag, "cx"), 4);
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_sabre_ghz_on_star_needs_swaps() {
        // GHZ(4) on star(5): H(0), CX(0,1), CX(1,2), CX(2,3)
        // Star: 0 is center. CX(1,2) and CX(2,3) are not adjacent — need SWAPs.
        let circuit = Circuit::ghz(4).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::star(5), BasisGates::iqm());
        TrivialLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert!(count_swaps(&dag) > 0, "GHZ on star topology needs SWAPs");
        assert_eq!(count_gates(&dag, "cx"), 3, "all CX gates preserved");
        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }
}
