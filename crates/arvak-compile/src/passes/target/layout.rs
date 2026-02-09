//! Layout passes for mapping logical qubits to physical qubits.

use arvak_ir::{CircuitDag, CircuitLevel};

use crate::error::{CompileError, CompileResult};
use crate::pass::{Pass, PassKind};
use crate::property::{Layout, PropertySet};

/// Trivial layout pass.
///
/// Maps logical qubit i to physical qubit i.
/// This is the simplest layout strategy and works when the
/// circuit fits within the device and no optimization is needed.
pub struct TrivialLayout;

impl Pass for TrivialLayout {
    fn name(&self) -> &'static str {
        "TrivialLayout"
    }

    fn kind(&self) -> PassKind {
        PassKind::Analysis
    }

    #[allow(clippy::cast_possible_truncation)]
    fn run(&self, dag: &mut CircuitDag, properties: &mut PropertySet) -> CompileResult<()> {
        // Check if we have a coupling map
        let coupling_map = properties
            .coupling_map
            .as_ref()
            .ok_or(CompileError::MissingCouplingMap)?;

        // Check if circuit fits
        let num_logical = dag.num_qubits();
        let num_physical = coupling_map.num_qubits() as usize;

        if num_logical > num_physical {
            return Err(CompileError::CircuitTooLarge {
                required: num_logical,
                available: coupling_map.num_qubits(),
            });
        }

        // Create trivial layout
        let layout = Layout::trivial(num_logical as u32);
        properties.layout = Some(layout);

        // Mark the circuit as physical level
        dag.set_level(CircuitLevel::Physical);

        Ok(())
    }

    fn should_run(&self, _dag: &CircuitDag, properties: &PropertySet) -> bool {
        // Only run if we don't have a layout yet and have a coupling map
        properties.layout.is_none() && properties.coupling_map.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property::{BasisGates, CouplingMap};
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_trivial_layout() {
        use arvak_ir::CircuitLevel;

        let mut circuit = Circuit::with_size("test", 3, 0);
        circuit.h(QubitId(0)).unwrap();
        let mut dag = circuit.into_dag();

        assert_eq!(dag.level(), CircuitLevel::Logical);

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        TrivialLayout.run(&mut dag, &mut props).unwrap();

        let layout = props.layout.as_ref().unwrap();
        assert_eq!(layout.get_physical(QubitId(0)), Some(0));
        assert_eq!(layout.get_physical(QubitId(1)), Some(1));
        assert_eq!(layout.get_physical(QubitId(2)), Some(2));
        assert_eq!(dag.level(), CircuitLevel::Physical);
    }

    #[test]
    fn test_trivial_layout_too_large() {
        let circuit = Circuit::with_size("test", 10, 0);
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new().with_target(CouplingMap::linear(5), BasisGates::iqm());

        let result = TrivialLayout.run(&mut dag, &mut props);
        assert!(matches!(result, Err(CompileError::CircuitTooLarge { .. })));
    }
}
