//! Optimization passes.

use hiq_ir::CircuitDag;

use crate::error::CompileResult;
use crate::pass::{Pass, PassKind};
use crate::property::PropertySet;

/// Single-qubit gate optimization pass.
///
/// Merges consecutive single-qubit gates on the same qubit.
/// This is a placeholder implementation that demonstrates
/// the pass structure. A full implementation would:
/// - Collect runs of 1q gates
/// - Compute their combined unitary
/// - Decompose back to minimal gate sequence
pub struct Optimize1qGates;

impl Pass for Optimize1qGates {
    fn name(&self) -> &str {
        "Optimize1qGates"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, _dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // TODO: Implement actual 1q gate optimization
        // For now, this is a no-op placeholder
        //
        // The algorithm would be:
        // 1. For each qubit, collect runs of consecutive 1q gates
        // 2. For each run, compute the combined 2x2 unitary matrix
        // 3. Decompose the unitary back to a minimal gate sequence
        //    using ZYZ or ZXZ decomposition
        // 4. Replace the run with the optimized sequence
        //
        // This can significantly reduce gate counts, especially
        // after basis translation which can introduce redundant gates.

        Ok(())
    }

    fn should_run(&self, dag: &CircuitDag, _properties: &PropertySet) -> bool {
        // Only run if there are operations to optimize
        dag.num_ops() > 0
    }
}

/// CX cancellation pass.
///
/// Cancels pairs of adjacent CX gates on the same qubits.
#[allow(dead_code)]
pub struct CancelCX;

impl Pass for CancelCX {
    fn name(&self) -> &str {
        "CancelCX"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, _dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // TODO: Implement CX cancellation
        // CX · CX = I (identity)
        //
        // The algorithm would be:
        // 1. For each qubit pair, find consecutive CX gates
        // 2. If two CX gates are adjacent with same control/target,
        //    remove both

        Ok(())
    }
}

/// Commutative cancellation pass.
///
/// Uses gate commutation rules to cancel gates.
#[allow(dead_code)]
pub struct CommutativeCancellation;

impl Pass for CommutativeCancellation {
    fn name(&self) -> &str {
        "CommutativeCancellation"
    }

    fn kind(&self) -> PassKind {
        PassKind::Transformation
    }

    fn run(&self, _dag: &mut CircuitDag, _properties: &mut PropertySet) -> CompileResult<()> {
        // TODO: Implement commutative cancellation
        // Uses facts like:
        // - CZ commutes with Rz on either qubit
        // - CX commutes with Rz on control and Rx on target
        // - Diagonal gates commute with each other

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiq_ir::{Circuit, QubitId};

    #[test]
    fn test_optimize_1q_gates() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.h(QubitId(0)).unwrap(); // H·H = I
        let mut dag = circuit.into_dag();

        let mut props = PropertySet::new();
        Optimize1qGates.run(&mut dag, &mut props).unwrap();

        // Currently a no-op, so ops count unchanged
        // When implemented, this should reduce to 0 or identity
        assert_eq!(dag.num_ops(), 2);
    }
}
