//! Zone-aware routing pass for neutral-atom architectures.
//!
//! Neutral-atom quantum computers organize qubits into interaction zones.
//! Qubits within a zone can interact via Rydberg gates, but qubits in
//! different zones must be shuttled together first.
//!
//! This pass inserts shuttle instructions when two-qubit gates span zones.

use arvak_ir::{CircuitDag, Instruction};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Zone assignment for neutral-atom routing.
///
/// Stored as a custom property in the `PropertySet`.
#[derive(Debug, Clone)]
pub struct ZoneAssignment {
    /// Number of zones.
    pub zones: u32,
    /// Qubits per zone (evenly divided, last zone gets remainder).
    pub qubits_per_zone: u32,
    /// Total number of qubits.
    pub num_qubits: u32,
}

impl ZoneAssignment {
    /// Create a new zone assignment.
    pub fn new(num_qubits: u32, zones: u32) -> Self {
        Self {
            zones,
            qubits_per_zone: num_qubits / zones.max(1),
            num_qubits,
        }
    }

    /// Get the zone that a physical qubit belongs to.
    pub fn zone_of(&self, qubit: u32) -> u32 {
        let z = qubit / self.qubits_per_zone;
        z.min(self.zones - 1)
    }

    /// Check if two physical qubits are in the same zone.
    pub fn same_zone(&self, q1: u32, q2: u32) -> bool {
        self.zone_of(q1) == self.zone_of(q2)
    }
}

/// Neutral-atom zone-aware routing pass.
///
/// Inserts shuttle instructions before two-qubit gates that span different
/// zones. The shuttled qubit is moved to the target zone and then back
/// after the interaction.
pub struct NeutralAtomRouting {
    /// Number of interaction zones.
    pub zones: u32,
}

impl NeutralAtomRouting {
    /// Create a new neutral-atom routing pass with the given zone count.
    pub fn new(zones: u32) -> Self {
        Self { zones }
    }
}

impl Pass for NeutralAtomRouting {
    fn name(&self) -> &'static str {
        "NeutralAtomRouting"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    #[allow(clippy::cast_possible_truncation)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        let layout = properties
            .layout
            .as_ref()
            .ok_or(CompileError::MissingLayout)?;

        let num_qubits = dag.num_qubits() as u32;
        let zone_assignment = ZoneAssignment::new(num_qubits, self.zones);

        // Collect only the qubit pairs for two-qubit gates (avoids cloning full instructions)
        let two_qubit_ops: Vec<_> = dag
            .topological_ops()
            .filter(|(_, inst)| inst.qubits.len() == 2)
            .map(|(idx, inst)| (idx, inst.qubits[0], inst.qubits[1]))
            .collect();

        for (_node_idx, q0, q1) in two_qubit_ops {

            let p0 = layout.get_physical(q0).ok_or(CompileError::MissingLayout)?;
            let p1 = layout.get_physical(q1).ok_or(CompileError::MissingLayout)?;

            let z0 = zone_assignment.zone_of(p0);
            let z1 = zone_assignment.zone_of(p1);

            if z0 == z1 {
                continue; // Same zone, no shuttle needed
            }

            // Insert shuttle: move q1 to zone of q0
            dag.apply(Instruction::shuttle(q1, z1, z0))
                .map_err(CompileError::Ir)?;

            // The gate itself is already in the DAG.
            // Insert shuttle back: move q1 back to its original zone
            dag.apply(Instruction::shuttle(q1, z0, z1))
                .map_err(CompileError::Ir)?;
        }

        // Store zone assignment for downstream passes
        properties.insert(zone_assignment);

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        properties.layout.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passes::TrivialLayout;
    use crate::property::{BasisGates, CouplingMap};
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_zone_assignment() {
        let za = ZoneAssignment::new(6, 2);
        assert_eq!(za.zone_of(0), 0);
        assert_eq!(za.zone_of(1), 0);
        assert_eq!(za.zone_of(2), 0);
        assert_eq!(za.zone_of(3), 1);
        assert_eq!(za.zone_of(4), 1);
        assert_eq!(za.zone_of(5), 1);

        assert!(za.same_zone(0, 1));
        assert!(za.same_zone(3, 5));
        assert!(!za.same_zone(2, 3));
    }

    #[test]
    fn test_same_zone_no_shuttle() {
        // CZ on qubits 0,1 in zone 0 — no shuttle needed
        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.cz(QubitId(0), QubitId(1)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::zoned(4, 2), BasisGates::neutral_atom());

        TrivialLayout.run(&mut dag, &mut props).unwrap();

        let ops_before = dag.num_ops();
        NeutralAtomRouting::new(2)
            .run(&mut dag, &mut props)
            .unwrap();
        let ops_after = dag.num_ops();

        // No shuttles inserted
        assert_eq!(ops_before, ops_after);
    }

    #[test]
    fn test_cross_zone_inserts_shuttles() {
        // CZ on qubits 0,3 — zone 0 and zone 1 → needs shuttle
        let mut circuit = Circuit::with_size("test", 4, 0);
        circuit.cz(QubitId(0), QubitId(3)).unwrap();
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::zoned(4, 2), BasisGates::neutral_atom());

        TrivialLayout.run(&mut dag, &mut props).unwrap();

        let ops_before = dag.num_ops();
        NeutralAtomRouting::new(2)
            .run(&mut dag, &mut props)
            .unwrap();
        let ops_after = dag.num_ops();

        // Should have inserted 2 shuttle ops (move there + move back)
        assert_eq!(ops_after, ops_before + 2);

        // Zone assignment should be stored
        let za = props.get::<ZoneAssignment>().unwrap();
        assert_eq!(za.zones, 2);
    }

    #[test]
    fn test_multi_gate_cross_zone() {
        // Multiple cross-zone gates
        let mut circuit = Circuit::with_size("test", 6, 0);
        circuit.cz(QubitId(0), QubitId(3)).unwrap(); // zone 0↔1
        circuit.cz(QubitId(1), QubitId(4)).unwrap(); // zone 0↔1
        circuit.cz(QubitId(0), QubitId(1)).unwrap(); // same zone, no shuttle
        let mut dag = circuit.into_dag();

        let mut props =
            PropertySet::new().with_target(CouplingMap::zoned(6, 2), BasisGates::neutral_atom());

        TrivialLayout.run(&mut dag, &mut props).unwrap();

        let ops_before = dag.num_ops();
        NeutralAtomRouting::new(2)
            .run(&mut dag, &mut props)
            .unwrap();
        let ops_after = dag.num_ops();

        // 2 cross-zone gates × 2 shuttles each = 4 shuttle ops
        assert_eq!(ops_after, ops_before + 4);
    }
}
