//! Dense layout pass for topology-aware qubit placement.
//!
//! Places logical qubits onto the best-connected subgraph of the physical
//! device, minimising the total two-qubit gate distance. This reduces the
//! number of SWAP insertions needed during routing.
//!
//! # Algorithm
//!
//! 1. Build an **interaction graph** from the circuit: nodes are logical
//!    qubits, edge weights are the number of two-qubit gates between each pair.
//! 2. Compute a **connectivity score** for each physical qubit: the number of
//!    edges incident to it, weighted by how many of its neighbors also have
//!    high connectivity.
//! 3. **Greedy placement**: assign logical qubits with the most interactions
//!    first, choosing the physical qubit that minimises the weighted distance
//!    to already-placed neighbors.

use rustc_hash::FxHashMap;

use arvak_ir::{CircuitDag, CircuitLevel, QubitId};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::{Layout, PropertySet};

/// Dense layout pass.
///
/// Maps logical qubits to physical qubits by analysing the circuit's
/// two-qubit gate interaction pattern and choosing physical positions
/// that minimise routing distance.
pub struct DenseLayout;

impl Pass for DenseLayout {
    fn name(&self) -> &'static str {
        "DenseLayout"
    }

    fn kind(&self) -> PassKind {
        PassKind::Analysis
    }

    #[allow(clippy::cast_possible_truncation)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let coupling_map = properties
            .coupling_map
            .as_ref()
            .ok_or(CompileError::MissingCouplingMap)?;

        let num_logical = dag.num_qubits();
        let num_physical = coupling_map.num_qubits() as usize;

        if num_logical > num_physical {
            return Err(CompileError::CircuitTooLarge {
                required: num_logical,
                available: coupling_map.num_qubits(),
            });
        }

        // If no two-qubit gates, fall back to trivial layout.
        let logical_qubits: Vec<QubitId> = dag.qubits().collect();

        // Build interaction graph: count two-qubit gates between each pair.
        let mut interactions: FxHashMap<(QubitId, QubitId), u32> = FxHashMap::default();
        let mut qubit_interaction_count: FxHashMap<QubitId, u32> = FxHashMap::default();

        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                let q0 = inst.qubits[0];
                let q1 = inst.qubits[1];
                let key = if q0.0 <= q1.0 { (q0, q1) } else { (q1, q0) };
                *interactions.entry(key).or_insert(0) += 1;
                *qubit_interaction_count.entry(q0).or_insert(0) += 1;
                *qubit_interaction_count.entry(q1).or_insert(0) += 1;
            }
        }

        // If no two-qubit gates, trivial layout is fine.
        if interactions.is_empty() {
            let layout = Layout::trivial(num_logical as u32);
            properties.initial_layout = Some(layout.clone());
            properties.layout = Some(layout);
            dag.set_level(CircuitLevel::Physical);
            return Ok(());
        }

        // Sort logical qubits by interaction count (most interactions first).
        let mut sorted_logical = logical_qubits.clone();
        sorted_logical.sort_by(|a, b| {
            let ca = qubit_interaction_count.get(a).copied().unwrap_or(0);
            let cb = qubit_interaction_count.get(b).copied().unwrap_or(0);
            cb.cmp(&ca)
        });

        // Compute physical qubit connectivity scores.
        // Score = number of edges in the coupling map incident to this qubit.
        let mut phys_connectivity: Vec<(u32, usize)> = (0..coupling_map.num_qubits())
            .map(|p| (p, coupling_map.neighbors(p).count()))
            .collect();
        // Sort by connectivity (highest first).
        phys_connectivity.sort_by(|a, b| b.1.cmp(&a.1));

        // Greedy placement.
        let mut layout = Layout::new();
        let mut placed_physical: Vec<bool> = vec![false; num_physical];

        // Place the first (most-interacting) logical qubit on the most-connected
        // physical qubit.
        let first_logical = sorted_logical[0];
        let first_physical = phys_connectivity[0].0;
        layout.add(first_logical, first_physical);
        placed_physical[first_physical as usize] = true;

        // Place remaining logical qubits.
        for &logical in &sorted_logical[1..] {
            // Find interactions between this logical qubit and already-placed ones.
            let mut best_physical: Option<u32> = None;
            let mut best_cost = u64::MAX;

            for phys in 0..coupling_map.num_qubits() {
                if placed_physical[phys as usize] {
                    continue;
                }

                // Compute cost: sum of (interaction_weight * distance) for all
                // already-placed neighbors.
                let mut cost: u64 = 0;
                for (&(qa, qb), &weight) in &interactions {
                    let partner = if qa == logical {
                        Some(qb)
                    } else if qb == logical {
                        Some(qa)
                    } else {
                        None
                    };

                    if let Some(partner) = partner {
                        if let Some(partner_phys) = layout.get_physical(partner) {
                            let dist = coupling_map
                                .distance(phys, partner_phys)
                                .unwrap_or(u32::MAX);
                            cost += u64::from(weight) * u64::from(dist);
                        }
                    }
                }

                // Tie-break by physical connectivity (prefer more-connected).
                // Encode as: cost * large_factor - connectivity
                let connectivity = coupling_map.neighbors(phys).count() as u64;
                let score = cost
                    .saturating_mul(1000)
                    .saturating_sub(connectivity.min(999));

                if score < best_cost {
                    best_cost = score;
                    best_physical = Some(phys);
                }
            }

            let phys = best_physical.ok_or(CompileError::PassFailed {
                name: "DenseLayout".into(),
                reason: "no available physical qubit for placement".into(),
            })?;

            layout.add(logical, phys);
            placed_physical[phys as usize] = true;
        }

        properties.initial_layout = Some(layout.clone());
        properties.layout = Some(layout);
        dag.set_level(CircuitLevel::Physical);

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties.layout.is_none() && properties.coupling_map.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::{BasicRouting, SabreRouting, TrivialLayout};
    use crate::property::{BasisGates, CouplingMap};
    use arvak_ir::{Circuit, InstructionKind, QubitId};

    /// Count SWAP gates in a DAG.
    fn count_swaps(dag: &CircuitDag) -> usize {
        dag.topological_ops()
            .filter(
                |(_, inst)| matches!(&inst.kind, InstructionKind::Gate(g) if g.name() == "swap"),
            )
            .count()
    }

    /// Verify all two-qubit gates act on adjacent physical qubits.
    fn assert_all_adjacent(dag: &CircuitDag, coupling_map: &CouplingMap) {
        for (_, inst) in dag.topological_ops() {
            if inst.qubits.len() == 2 {
                assert!(
                    coupling_map.is_connected(inst.qubits[0].0, inst.qubits[1].0),
                    "two-qubit gate on non-adjacent physical qubits ({}, {})",
                    inst.qubits[0].0,
                    inst.qubits[1].0,
                );
            }
        }
    }

    #[test]
    fn test_dense_layout_basic() {
        // Simple circuit: just verify it produces a valid layout.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();

        let layout = props.layout.as_ref().unwrap();
        // All 3 logical qubits must be mapped.
        assert_eq!(layout.len(), 3);
        // Each logical qubit maps to a unique physical qubit.
        let mut physicals: Vec<u32> = (0..3)
            .map(|i| layout.get_physical(QubitId(i)).unwrap())
            .collect();
        physicals.sort_unstable();
        physicals.dedup();
        assert_eq!(physicals.len(), 3, "all physical qubits must be unique");
    }

    #[test]
    fn test_dense_layout_places_interacting_qubits_adjacent() {
        // CX(0,1) on linear(5). DenseLayout should place q0 and q1 on
        // adjacent physical qubits so no SWAP is needed.
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();

        let layout = props.layout.as_ref().unwrap();
        let p0 = layout.get_physical(QubitId(0)).unwrap();
        let p1 = layout.get_physical(QubitId(1)).unwrap();
        let cm = props.coupling_map.as_ref().unwrap();

        assert!(
            cm.is_connected(p0, p1),
            "DenseLayout should place interacting qubits adjacent: p0={p0}, p1={p1}"
        );
    }

    #[test]
    fn test_dense_layout_reduces_swaps_vs_trivial() {
        // Circuit with CX(0,4) on star(5): trivial layout puts q4 at distance 2
        // from q0 (star center). DenseLayout should find a better placement.
        //
        // Star topology: 0-1, 0-2, 0-3, 0-4
        // CX(0,4) with trivial: q0→phys0, q4→phys4 → adjacent (star center).
        //
        // Let's use a harder case: CX(1,3) on linear(5).
        // Trivial: q1→phys1, q3→phys3, distance=2, needs 1 SWAP.
        // Dense: should place them adjacent.
        let mut circuit = Circuit::with_size("test", 5, 0);
        // Many CX(1,3) gates to make the interaction weight high.
        for _ in 0..5 {
            circuit.cx(QubitId(1), QubitId(3)).unwrap();
        }
        let mut dag_dense = circuit.dag().clone();
        let mut dag_trivial = circuit.into_dag();

        let cm = CouplingMap::linear(5);

        // DenseLayout + routing.
        let mut props_dense = PropertySet::new().with_target(cm.clone(), BasisGates::iqm());
        DenseLayout.run(&mut dag_dense, &mut props_dense).unwrap();

        // Verify DenseLayout placed q1 and q3 adjacent.
        let layout = props_dense.layout.as_ref().unwrap();
        let p1 = layout.get_physical(QubitId(1)).unwrap();
        let p3 = layout.get_physical(QubitId(3)).unwrap();
        assert!(
            cm.is_connected(p1, p3),
            "DenseLayout should place q1(→p{p1}) and q3(→p{p3}) adjacent on linear(5)"
        );

        BasicRouting.run(&mut dag_dense, &mut props_dense).unwrap();
        let dense_swaps = count_swaps(&dag_dense);

        // TrivialLayout + routing.
        let mut props_trivial = PropertySet::new().with_target(cm.clone(), BasisGates::iqm());
        TrivialLayout
            .run(&mut dag_trivial, &mut props_trivial)
            .unwrap();
        BasicRouting
            .run(&mut dag_trivial, &mut props_trivial)
            .unwrap();
        let trivial_swaps = count_swaps(&dag_trivial);

        assert!(
            dense_swaps <= trivial_swaps,
            "DenseLayout ({dense_swaps} SWAPs) should need <= TrivialLayout ({trivial_swaps} SWAPs)"
        );

        assert_all_adjacent(&dag_dense, &cm);
        assert_all_adjacent(&dag_trivial, &cm);
    }

    #[test]
    fn test_dense_layout_no_two_qubit_gates() {
        // Circuit with only single-qubit gates: falls back to trivial layout.
        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.x(QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();

        let layout = props.layout.as_ref().unwrap();
        // Should produce trivial i→i layout.
        assert_eq!(layout.get_physical(QubitId(0)), Some(0));
        assert_eq!(layout.get_physical(QubitId(1)), Some(1));
        assert_eq!(layout.get_physical(QubitId(2)), Some(2));
    }

    #[test]
    fn test_dense_layout_too_large() {
        let circuit = Circuit::with_size("test", 10, 0);
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        let result = DenseLayout.run(&mut dag, &mut props);
        assert!(matches!(result, Err(CompileError::CircuitTooLarge { .. })));
    }

    #[test]
    fn test_dense_layout_with_sabre_end_to_end() {
        // Full pipeline: DenseLayout + SabreRouting on a circuit that benefits
        // from smart placement.
        // Circuit: heavy interaction between q0-q3, q1-q4 on linear(5).
        // Trivial layout: q0=0, q3=3 (distance 3), q1=1, q4=4 (distance 3).
        // Dense layout should reduce these distances.
        let mut circuit = Circuit::with_size("test", 5, 0);
        for _ in 0..3 {
            circuit.cx(QubitId(0), QubitId(3)).unwrap();
            circuit.cx(QubitId(1), QubitId(4)).unwrap();
        }
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();
        SabreRouting::new().run(&mut dag, &mut props).unwrap();

        assert_all_adjacent(&dag, props.coupling_map.as_ref().unwrap());
    }

    #[test]
    fn test_dense_layout_star_topology() {
        // Star(5): center=0 connected to 1,2,3,4.
        // Circuit: CX(1,2), CX(1,3) — q1 has most interactions.
        // DenseLayout should place q1 on the center (phys 0).
        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        circuit.cx(QubitId(1), QubitId(3)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::star(5), BasisGates::iqm());
        DenseLayout.run(&mut dag, &mut props).unwrap();

        let layout = props.layout.as_ref().unwrap();
        let p1 = layout.get_physical(QubitId(1)).unwrap();
        // q1 (most interactions) should be on center (phys 0) which has highest connectivity.
        assert_eq!(
            p1, 0,
            "most-interacting qubit should be placed on star center (phys 0), got phys {p1}"
        );
    }
}
