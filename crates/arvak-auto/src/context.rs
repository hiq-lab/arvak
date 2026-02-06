//! Uncomputation context management.

use hiq_ir::Circuit;
use hiq_ir::qubit::QubitId;
use rustc_hash::FxHashSet;

use crate::error::{UncomputeError, UncomputeResult};
use crate::inverse::inverse_instruction;

/// Scope of uncomputation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UncomputeScope {
    /// Uncompute all ancilla qubits.
    All,
    /// Uncompute only specified qubits.
    Selected,
    /// Uncompute qubits not in the output set.
    ExcludeOutput,
}

impl Default for UncomputeScope {
    fn default() -> Self {
        Self::All
    }
}

/// Context for automatic uncomputation.
///
/// This tracks the state of a circuit at the point where uncomputation
/// should begin, allowing the operations to be reversed.
#[derive(Debug)]
pub struct UncomputeContext {
    /// Snapshot of operation count when context was created.
    start_op_count: usize,
    /// Qubits that should be uncomputed.
    target_qubits: FxHashSet<QubitId>,
    /// Qubits that are marked as output (should NOT be uncomputed).
    output_qubits: FxHashSet<QubitId>,
    /// Scope of uncomputation.
    scope: UncomputeScope,
    /// Label for this context (debugging).
    label: Option<String>,
}

impl UncomputeContext {
    /// Begin an uncomputation context.
    ///
    /// Records the current state of the circuit so that operations
    /// performed after this point can be reversed.
    pub fn begin(circuit: &Circuit) -> Self {
        Self {
            start_op_count: circuit.dag().num_ops(),
            target_qubits: FxHashSet::default(),
            output_qubits: FxHashSet::default(),
            scope: UncomputeScope::All,
            label: None,
        }
    }

    /// Begin a context with a specific scope.
    pub fn begin_with_scope(circuit: &Circuit, scope: UncomputeScope) -> Self {
        Self {
            start_op_count: circuit.dag().num_ops(),
            target_qubits: FxHashSet::default(),
            output_qubits: FxHashSet::default(),
            scope,
            label: None,
        }
    }

    /// Add a label to this context (for debugging).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Mark specific qubits to be uncomputed.
    pub fn mark_uncompute(&mut self, qubits: impl IntoIterator<Item = QubitId>) {
        self.target_qubits.extend(qubits);
    }

    /// Mark qubits as output (should not be uncomputed).
    pub fn mark_output(&mut self, qubits: impl IntoIterator<Item = QubitId>) {
        self.output_qubits.extend(qubits);
    }

    /// Get the starting operation count.
    pub fn start_op_count(&self) -> usize {
        self.start_op_count
    }

    /// Get the target qubits for uncomputation.
    pub fn target_qubits(&self) -> &FxHashSet<QubitId> {
        &self.target_qubits
    }

    /// Get the output qubits (protected from uncomputation).
    pub fn output_qubits(&self) -> &FxHashSet<QubitId> {
        &self.output_qubits
    }

    /// Get the scope.
    pub fn scope(&self) -> UncomputeScope {
        self.scope
    }

    /// Get the label if set.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Perform uncomputation on the circuit.
    ///
    /// This inverts all operations that were added after the context was created,
    /// applying them in reverse order.
    pub fn uncompute(self, circuit: &mut Circuit) -> UncomputeResult<()> {
        let current_count = circuit.dag().num_ops();

        if current_count <= self.start_op_count {
            // Nothing to uncompute
            return Ok(());
        }

        // Collect instructions added after context was created
        let instructions: Vec<_> = circuit
            .dag()
            .topological_ops()
            .skip(self.start_op_count)
            .filter(|(_idx, inst)| {
                // Filter based on scope
                match self.scope {
                    UncomputeScope::All => true,
                    UncomputeScope::Selected => {
                        inst.qubits.iter().any(|q| self.target_qubits.contains(q))
                    }
                    UncomputeScope::ExcludeOutput => {
                        !inst.qubits.iter().any(|q| self.output_qubits.contains(q))
                    }
                }
            })
            .map(|(_idx, inst)| inst.clone())
            .collect();

        // Apply inverse operations in reverse order
        for inst in instructions.into_iter().rev() {
            // Skip if this touches output qubits (in ExcludeOutput mode)
            if self.scope == UncomputeScope::ExcludeOutput
                && inst.qubits.iter().any(|q| self.output_qubits.contains(q))
            {
                continue;
            }

            let inverse = inverse_instruction(&inst)?;
            circuit
                .dag_mut()
                .apply(inverse)
                .map_err(|e| UncomputeError::CircuitError(e.to_string()))?;
        }

        Ok(())
    }
}

/// Convenience function to uncompute a context.
pub fn uncompute(circuit: &mut Circuit, context: UncomputeContext) -> UncomputeResult<()> {
    context.uncompute(circuit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let circuit = Circuit::new("test");
        let ctx = UncomputeContext::begin(&circuit);

        assert_eq!(ctx.start_op_count(), 0);
        assert_eq!(ctx.scope(), UncomputeScope::All);
    }

    #[test]
    fn test_context_with_label() {
        let circuit = Circuit::new("test");
        let ctx = UncomputeContext::begin(&circuit).with_label("my_section");

        assert_eq!(ctx.label(), Some("my_section"));
    }

    #[test]
    fn test_uncompute_simple() {
        let mut circuit = Circuit::with_size("test", 1, 0);
        let ctx = UncomputeContext::begin(&circuit);

        // Apply some gates
        circuit.h(QubitId(0)).unwrap();
        circuit.t(QubitId(0)).unwrap();

        // Operations count before uncompute
        assert_eq!(circuit.dag().num_ops(), 2);

        // Uncompute
        ctx.uncompute(&mut circuit).unwrap();

        // Should have added inverse operations: Tdg, H
        assert_eq!(circuit.dag().num_ops(), 4);
    }

    #[test]
    fn test_uncompute_with_output() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        let mut ctx = UncomputeContext::begin_with_scope(&circuit, UncomputeScope::ExcludeOutput);

        // Mark qubit 0 as output
        ctx.mark_output([QubitId(0)]);

        // Apply gates to both qubits
        circuit.h(QubitId(0)).unwrap();
        circuit.h(QubitId(1)).unwrap();

        assert_eq!(circuit.dag().num_ops(), 2);

        // Uncompute - should only uncompute qubit 1
        ctx.uncompute(&mut circuit).unwrap();

        // Should have 3 ops: H(0), H(1), H(1)†
        // The H on qubit 0 is not uncomputed because it's marked as output
        assert_eq!(circuit.dag().num_ops(), 3);
    }
}
