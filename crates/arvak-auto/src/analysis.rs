//! Circuit analysis for automatic uncomputation.

use arvak_ir::Circuit;
use arvak_ir::instruction::InstructionKind;
use arvak_ir::qubit::QubitId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::UncomputeResult;

/// Result of analyzing a circuit for uncomputation.
#[derive(Debug, Clone)]
pub struct UncomputeAnalysis {
    /// Qubits that can be safely uncomputed.
    pub uncomputable: FxHashSet<QubitId>,
    /// Qubits that cannot be uncomputed (measured, entangled with output).
    pub non_uncomputable: FxHashSet<QubitId>,
    /// Qubits marked as output.
    pub output_qubits: FxHashSet<QubitId>,
    /// Qubits that were measured.
    pub measured_qubits: FxHashSet<QubitId>,
    /// Dependencies between qubits (qubit A depends on qubit B).
    pub dependencies: FxHashMap<QubitId, FxHashSet<QubitId>>,
}

impl UncomputeAnalysis {
    /// Create a new empty analysis.
    pub fn new() -> Self {
        Self {
            uncomputable: FxHashSet::default(),
            non_uncomputable: FxHashSet::default(),
            output_qubits: FxHashSet::default(),
            measured_qubits: FxHashSet::default(),
            dependencies: FxHashMap::default(),
        }
    }

    /// Check if a qubit can be uncomputed.
    pub fn can_uncompute(&self, qubit: QubitId) -> bool {
        self.uncomputable.contains(&qubit)
    }

    /// Get all qubits that can be uncomputed.
    pub fn get_uncomputable(&self) -> &FxHashSet<QubitId> {
        &self.uncomputable
    }

    /// Get the reason a qubit cannot be uncomputed, if any.
    pub fn non_uncompute_reason(&self, qubit: QubitId) -> Option<&'static str> {
        if self.measured_qubits.contains(&qubit) {
            Some("qubit was measured")
        } else if self.output_qubits.contains(&qubit) {
            Some("qubit is marked as output")
        } else if self.non_uncomputable.contains(&qubit) {
            Some("qubit is entangled with non-uncomputable qubit")
        } else {
            None
        }
    }
}

impl Default for UncomputeAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

/// Analyze a circuit to determine which qubits can be uncomputed.
///
/// A qubit can be uncomputed if:
/// 1. It has not been measured
/// 2. It is not marked as an output qubit
/// 3. It is not entangled with a qubit that cannot be uncomputed
///
/// # Arguments
///
/// - `circuit`: The circuit to analyze
/// - `output_qubits`: Qubits that are considered output and should not be uncomputed
pub fn analyze_uncomputation(
    circuit: &Circuit,
    output_qubits: impl IntoIterator<Item = QubitId>,
) -> UncomputeResult<UncomputeAnalysis> {
    let mut analysis = UncomputeAnalysis::new();
    let dag = circuit.dag();

    // Mark output qubits
    for qubit in output_qubits {
        analysis.output_qubits.insert(qubit);
        analysis.non_uncomputable.insert(qubit);
    }

    // Find measured qubits
    for (_idx, inst) in dag.topological_ops() {
        if let InstructionKind::Measure = &inst.kind {
            for &qubit in &inst.qubits {
                analysis.measured_qubits.insert(qubit);
                analysis.non_uncomputable.insert(qubit);
            }
        }
    }

    // Build dependency graph (two-qubit gates create dependencies)
    for (_idx, inst) in dag.topological_ops() {
        if inst.qubits.len() > 1 {
            // Two-qubit (or more) gate: qubits become dependent on each other
            for &q1 in &inst.qubits {
                for &q2 in &inst.qubits {
                    if q1 != q2 {
                        analysis.dependencies.entry(q1).or_default().insert(q2);
                    }
                }
            }
        }
    }

    // Propagate non-uncomputable status through dependencies
    let mut changed = true;
    while changed {
        changed = false;
        for qubit in dag.qubits() {
            if analysis.non_uncomputable.contains(&qubit) {
                continue;
            }

            // Check if this qubit depends on any non-uncomputable qubit
            if let Some(deps) = analysis.dependencies.get(&qubit) {
                for &dep in deps {
                    if analysis.non_uncomputable.contains(&dep) {
                        analysis.non_uncomputable.insert(qubit);
                        changed = true;
                        break;
                    }
                }
            }
        }
    }

    // All qubits not in non_uncomputable are uncomputable
    for qubit in dag.qubits() {
        if !analysis.non_uncomputable.contains(&qubit) {
            analysis.uncomputable.insert(qubit);
        }
    }

    Ok(analysis)
}

/// Analyze which operations in a circuit can be reversed for uncomputation.
///
/// Returns the indices of operations that can be safely inverted.
pub fn find_reversible_ops(circuit: &Circuit) -> Vec<usize> {
    let dag = circuit.dag();
    let mut reversible = Vec::new();

    for (idx, (_node, inst)) in dag.topological_ops().enumerate() {
        match &inst.kind {
            InstructionKind::Gate(_) => {
                // All gate operations are reversible
                reversible.push(idx);
            }
            InstructionKind::Barrier | InstructionKind::Delay { .. } => {
                // Barriers and delays don't need reversal
                reversible.push(idx);
            }
            InstructionKind::Measure | InstructionKind::Reset => {
                // Non-reversible operations
            }
            InstructionKind::Shuttle { .. } => {
                // Shuttling is reversible (swap zones)
                reversible.push(idx);
            }
        }
    }

    reversible
}

/// Find the "computational cone" of a set of output qubits.
///
/// The computational cone includes all qubits that contribute to
/// the state of the output qubits through entangling operations.
pub fn find_computational_cone(
    circuit: &Circuit,
    output_qubits: impl IntoIterator<Item = QubitId>,
) -> FxHashSet<QubitId> {
    let dag = circuit.dag();
    let mut cone: FxHashSet<QubitId> = output_qubits.into_iter().collect();

    // Work backwards through the circuit
    let ops: Vec<_> = dag.topological_ops().collect();

    for (_idx, inst) in ops.into_iter().rev() {
        // If any qubit in this instruction is in the cone,
        // add all qubits in the instruction to the cone
        if inst.qubits.iter().any(|q| cone.contains(q)) {
            for &q in &inst.qubits {
                cone.insert(q);
            }
        }
    }

    cone
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_ir::qubit::ClbitId;

    #[test]
    fn test_analyze_empty_circuit() {
        let circuit = Circuit::with_size("test", 3, 0);
        let analysis = analyze_uncomputation(&circuit, []).unwrap();

        // All qubits should be uncomputable
        assert_eq!(analysis.uncomputable.len(), 3);
        assert_eq!(analysis.non_uncomputable.len(), 0);
    }

    #[test]
    fn test_analyze_with_output() {
        let circuit = Circuit::with_size("test", 3, 0);
        let analysis = analyze_uncomputation(&circuit, [QubitId(0)]).unwrap();

        // Qubit 0 should not be uncomputable
        assert!(!analysis.can_uncompute(QubitId(0)));
        assert!(analysis.can_uncompute(QubitId(1)));
        assert!(analysis.can_uncompute(QubitId(2)));
    }

    #[test]
    fn test_analyze_with_measurement() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();

        let analysis = analyze_uncomputation(&circuit, []).unwrap();

        // Qubit 0 was measured - cannot be uncomputed
        assert!(!analysis.can_uncompute(QubitId(0)));
        assert!(analysis.can_uncompute(QubitId(1)));
        assert!(analysis.measured_qubits.contains(&QubitId(0)));
    }

    #[test]
    fn test_analyze_entanglement_propagation() {
        let mut circuit = Circuit::with_size("test", 3, 0);

        // Entangle qubits 0 and 1
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        // Mark qubit 1 as output
        let analysis = analyze_uncomputation(&circuit, [QubitId(1)]).unwrap();

        // Qubit 0 should also be non-uncomputable due to entanglement
        assert!(!analysis.can_uncompute(QubitId(0)));
        assert!(!analysis.can_uncompute(QubitId(1)));
        assert!(analysis.can_uncompute(QubitId(2)));
    }

    #[test]
    fn test_find_reversible_ops() {
        let mut circuit = Circuit::with_size("test", 2, 1);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.measure(QubitId(0), ClbitId(0)).unwrap();

        let reversible = find_reversible_ops(&circuit);

        // H and CX are reversible, measure is not
        assert_eq!(reversible.len(), 2);
    }

    #[test]
    fn test_computational_cone() {
        let mut circuit = Circuit::with_size("test", 4, 0);

        // Create a small circuit
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();
        circuit.cx(QubitId(1), QubitId(2)).unwrap();
        // Qubit 3 is independent

        let cone = find_computational_cone(&circuit, [QubitId(2)]);

        // Cone should include qubits 0, 1, 2 but not 3
        assert!(cone.contains(&QubitId(0)));
        assert!(cone.contains(&QubitId(1)));
        assert!(cone.contains(&QubitId(2)));
        assert!(!cone.contains(&QubitId(3)));
    }
}
