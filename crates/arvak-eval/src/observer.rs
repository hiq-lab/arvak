//! Compilation Observer: pass-wise metrics collection and delta calculation.
//!
//! Wraps the `PassManager` execution to capture before/after snapshots
//! of circuit metrics at each compilation pass.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::debug;

use arvak_compile::{PassManager, PropertySet};
use arvak_ir::CircuitDag;
use arvak_ir::instruction::InstructionKind;

use crate::error::{EvalError, EvalResult};

/// Snapshot of circuit metrics at a point in the compilation pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitSnapshot {
    /// Circuit depth.
    pub depth: usize,
    /// Total operations count.
    pub total_ops: usize,
    /// Number of single-qubit gates.
    pub single_qubit_gates: usize,
    /// Number of two-qubit gates.
    pub two_qubit_gates: usize,
    /// Number of multi-qubit gates.
    pub multi_qubit_gates: usize,
    /// Gate counts by name.
    pub gate_counts: BTreeMap<String, usize>,
}

impl CircuitSnapshot {
    /// Take a snapshot of the current DAG state.
    pub fn capture(dag: &CircuitDag) -> Self {
        let mut gate_counts = BTreeMap::new();
        let mut single_qubit_gates = 0usize;
        let mut two_qubit_gates = 0usize;
        let mut multi_qubit_gates = 0usize;
        let mut total_ops = 0usize;

        for (_idx, inst) in dag.topological_ops() {
            total_ops += 1;
            if let InstructionKind::Gate(gate) = &inst.kind {
                let name = gate.name().to_string();
                *gate_counts.entry(name).or_insert(0) += 1;

                match gate.num_qubits() {
                    1 => single_qubit_gates += 1,
                    2 => two_qubit_gates += 1,
                    _ => multi_qubit_gates += 1,
                }
            }
        }

        Self {
            depth: dag.depth(),
            total_ops,
            single_qubit_gates,
            two_qubit_gates,
            multi_qubit_gates,
            gate_counts,
        }
    }
}

/// Delta between two circuit snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDelta {
    /// Change in depth (positive = increased).
    pub depth_delta: i64,
    /// Change in total ops.
    pub ops_delta: i64,
    /// Change in single-qubit gates.
    pub single_qubit_delta: i64,
    /// Change in two-qubit gates.
    pub two_qubit_delta: i64,
}

impl SnapshotDelta {
    /// Compute the delta between a before and after snapshot.
    pub fn compute(before: &CircuitSnapshot, after: &CircuitSnapshot) -> Self {
        Self {
            depth_delta: after.depth as i64 - before.depth as i64,
            ops_delta: after.total_ops as i64 - before.total_ops as i64,
            single_qubit_delta: after.single_qubit_gates as i64 - before.single_qubit_gates as i64,
            two_qubit_delta: after.two_qubit_gates as i64 - before.two_qubit_gates as i64,
        }
    }
}

/// Record of a single compilation pass execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassRecord {
    /// Name of the pass.
    pub pass_name: String,
    /// Index in the pipeline.
    pub pass_index: usize,
    /// Whether the pass was skipped.
    pub skipped: bool,
    /// Snapshot before the pass ran.
    pub before: CircuitSnapshot,
    /// Snapshot after the pass ran.
    pub after: CircuitSnapshot,
    /// Delta from this pass.
    pub delta: SnapshotDelta,
}

/// Result of observing the full compilation pipeline.
pub struct CompilationObserver {
    /// Individual pass records.
    pub pass_records: Vec<PassRecord>,
    /// Snapshot of the circuit before any passes.
    pub initial_metrics: CircuitSnapshot,
    /// Snapshot of the circuit after all passes.
    pub final_metrics: CircuitSnapshot,
    /// The compiled DAG (for downstream contract checking).
    pub final_dag: CircuitDag,
}

impl CompilationObserver {
    /// Observe the compilation pipeline by running it pass-by-pass.
    ///
    /// Instead of using `PassManager::run()` directly, this method
    /// runs each pass individually to capture before/after snapshots.
    ///
    /// Note: Currently this is a single-pass observation of the full pipeline.
    /// Per-pass metrics (individual pass before/after snapshots) are not yet
    /// implemented because `PassManager` does not expose individual passes.
    pub fn observe(
        pm: &PassManager,
        dag: &mut CircuitDag,
        props: &mut PropertySet,
    ) -> EvalResult<Self> {
        let initial_metrics = CircuitSnapshot::capture(dag);
        let mut pass_records = Vec::new();

        // Access passes through the PassManager.
        // Since PassManager doesn't expose passes directly, we use
        // the standard run() and capture only before/after the full pipeline.
        let before_all = CircuitSnapshot::capture(dag);

        pm.run(dag, props)
            .map_err(|e| EvalError::Compilation(e.to_string()))?;

        let after_all = CircuitSnapshot::capture(dag);

        // Record the full pipeline as a single observation.
        // Future versions can hook into individual passes.
        let delta = SnapshotDelta::compute(&before_all, &after_all);

        pass_records.push(PassRecord {
            pass_name: "full_pipeline".into(),
            pass_index: 0,
            skipped: false,
            before: before_all,
            after: after_all.clone(),
            delta,
        });

        debug!(
            "Compilation observed: depth {} -> {}, ops {} -> {}",
            initial_metrics.depth, after_all.depth, initial_metrics.total_ops, after_all.total_ops,
        );

        Ok(Self {
            pass_records,
            initial_metrics,
            final_metrics: after_all,
            final_dag: dag.clone(),
        })
    }

    /// Convert to serializable report form.
    pub fn into_report(self) -> CompilationReport {
        let overall_delta = SnapshotDelta::compute(&self.initial_metrics, &self.final_metrics);
        CompilationReport {
            initial: self.initial_metrics,
            final_snapshot: self.final_metrics,
            overall_delta,
            passes: self.pass_records,
        }
    }
}

/// Serializable compilation observation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationReport {
    /// Circuit state before compilation.
    pub initial: CircuitSnapshot,
    /// Circuit state after compilation.
    pub final_snapshot: CircuitSnapshot,
    /// Overall delta across the full pipeline.
    pub overall_delta: SnapshotDelta,
    /// Per-pass records.
    pub passes: Vec<PassRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use arvak_compile::PassManagerBuilder;
    use arvak_ir::{Circuit, QubitId};

    #[test]
    fn test_circuit_snapshot() {
        let mut circuit = Circuit::with_size("test", 2, 0);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let snapshot = CircuitSnapshot::capture(circuit.dag());
        assert_eq!(snapshot.depth, 2);
        assert_eq!(snapshot.single_qubit_gates, 1);
        assert_eq!(snapshot.two_qubit_gates, 1);
    }

    #[test]
    fn test_snapshot_delta() {
        let before = CircuitSnapshot {
            depth: 5,
            total_ops: 10,
            single_qubit_gates: 6,
            two_qubit_gates: 4,
            multi_qubit_gates: 0,
            gate_counts: BTreeMap::new(),
        };
        let after = CircuitSnapshot {
            depth: 3,
            total_ops: 7,
            single_qubit_gates: 3,
            two_qubit_gates: 4,
            multi_qubit_gates: 0,
            gate_counts: BTreeMap::new(),
        };
        let delta = SnapshotDelta::compute(&before, &after);
        assert_eq!(delta.depth_delta, -2);
        assert_eq!(delta.ops_delta, -3);
        assert_eq!(delta.single_qubit_delta, -3);
        assert_eq!(delta.two_qubit_delta, 0);
    }

    #[test]
    fn test_compilation_observer() {
        let mut circuit = Circuit::with_size("test", 2, 2);
        circuit.h(QubitId(0)).unwrap();
        circuit.cx(QubitId(0), QubitId(1)).unwrap();

        let (pm, mut props) = PassManagerBuilder::new().with_optimization_level(0).build();

        let mut dag = circuit.into_dag();
        let observer = CompilationObserver::observe(&pm, &mut dag, &mut props).unwrap();

        assert!(!observer.pass_records.is_empty());
        assert_eq!(observer.initial_metrics.depth, observer.final_metrics.depth);
    }
}
