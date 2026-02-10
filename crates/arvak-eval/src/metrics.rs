//! Metrics Aggregator: structural, compilation, orchestration, and emitter effect metrics.
//!
//! Combines data from input analysis, compilation observation,
//! orchestration analysis, and emitter compliance into unified evaluation metrics.

use serde::{Deserialize, Serialize};

use crate::emitter::EmitterReport;
use crate::input::InputAnalysis;
use crate::observer::CompilationObserver;
use crate::orchestration::OrchestrationReport;
use crate::scheduler_context::SchedulerFitness;

/// Compilation effect metrics (deltas from compilation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationEffect {
    /// Depth change (negative = reduction).
    pub depth_delta: i64,
    /// Depth ratio (compiled / original). < 1.0 means reduction.
    pub depth_ratio: f64,
    /// Operation count change.
    pub ops_delta: i64,
    /// Operation count ratio.
    pub ops_ratio: f64,
    /// Two-qubit gate change.
    pub two_qubit_delta: i64,
    /// Two-qubit gate ratio.
    pub two_qubit_ratio: f64,
}

/// Orchestration effect metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationEffect {
    /// Number of quantum phases in the hybrid DAG.
    pub quantum_phases: usize,
    /// Number of classical phases.
    pub classical_phases: usize,
    /// Critical path cost (abstract time units).
    pub critical_path_cost: f64,
    /// Critical path length (number of nodes).
    pub critical_path_length: usize,
    /// Maximum parallelizable quantum jobs.
    pub max_parallel_quantum: usize,
    /// Parallelism ratio (higher = more parallel opportunity).
    pub parallelism_ratio: f64,
    /// Whether the workload is purely quantum.
    pub is_purely_quantum: bool,
    /// Scheduler fitness score (0.0-1.0, None if no scheduler context).
    pub scheduler_fitness: Option<f64>,
}

/// Emitter compliance effect metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterEffect {
    /// Target backend name.
    pub target: String,
    /// Native gate coverage ratio (0.0-1.0).
    pub native_coverage: f64,
    /// Total materializable coverage ratio (0.0-1.0).
    pub materializable_coverage: f64,
    /// Estimated gate expansion factor from decomposition.
    pub estimated_expansion: f64,
    /// Number of distinct gate types requiring decomposition.
    pub decomposed_gate_types: usize,
    /// Number of distinct gate types that are lost.
    pub lost_gate_types: usize,
    /// Whether the circuit is fully materializable.
    pub fully_materializable: bool,
    /// Whether QASM3 emission succeeded.
    pub emission_success: bool,
}

/// Aggregated evaluation metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    /// Compilation effect (None if no compilation was performed).
    pub compilation_effect: Option<CompilationEffect>,
    /// Orchestration effect (None if --orchestration not used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration_effect: Option<OrchestrationEffect>,
    /// Emitter compliance effect (None if --emit not used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitter_effect: Option<EmitterEffect>,
}

/// Aggregator for combining metrics from all modules.
pub struct MetricsAggregator;

impl MetricsAggregator {
    /// Aggregate metrics from all evaluation modules.
    pub fn aggregate(input: &InputAnalysis, observer: &CompilationObserver) -> AggregatedMetrics {
        let compilation_effect = Self::compute_compilation_effect(input, observer);

        AggregatedMetrics {
            compilation_effect,
            orchestration_effect: None,
            emitter_effect: None,
        }
    }

    /// Aggregate metrics including orchestration analysis.
    pub fn aggregate_with_orchestration(
        input: &InputAnalysis,
        observer: &CompilationObserver,
        orchestration: &OrchestrationReport,
        scheduler_fitness: Option<&SchedulerFitness>,
    ) -> AggregatedMetrics {
        let compilation_effect = Self::compute_compilation_effect(input, observer);
        let orchestration_effect = Some(Self::compute_orchestration_effect(
            orchestration,
            scheduler_fitness,
        ));

        AggregatedMetrics {
            compilation_effect,
            orchestration_effect,
            emitter_effect: None,
        }
    }

    /// Aggregate all metrics: compilation + orchestration + emitter.
    pub fn aggregate_full(
        input: &InputAnalysis,
        observer: &CompilationObserver,
        orchestration: Option<(&OrchestrationReport, Option<&SchedulerFitness>)>,
        emitter: Option<&EmitterReport>,
    ) -> AggregatedMetrics {
        let compilation_effect = Self::compute_compilation_effect(input, observer);
        let orchestration_effect =
            orchestration.map(|(orch, sched)| Self::compute_orchestration_effect(orch, sched));
        let emitter_effect = emitter.map(Self::compute_emitter_effect);

        AggregatedMetrics {
            compilation_effect,
            orchestration_effect,
            emitter_effect,
        }
    }

    fn compute_orchestration_effect(
        orch: &OrchestrationReport,
        scheduler_fitness: Option<&SchedulerFitness>,
    ) -> OrchestrationEffect {
        OrchestrationEffect {
            quantum_phases: orch.summary.quantum_phases,
            classical_phases: orch.summary.classical_phases,
            critical_path_cost: orch.critical_path.total_cost,
            critical_path_length: orch.critical_path.node_indices.len(),
            max_parallel_quantum: orch.batchability.max_parallel_quantum,
            parallelism_ratio: orch.batchability.parallelism_ratio,
            is_purely_quantum: orch.batchability.is_purely_quantum,
            scheduler_fitness: scheduler_fitness.map(|f| f.fitness_score),
        }
    }

    fn compute_emitter_effect(emitter: &EmitterReport) -> EmitterEffect {
        use crate::emitter::MaterializationStatus;

        let decomposed_gate_types = emitter
            .gate_materializations
            .iter()
            .filter(|m| m.status == MaterializationStatus::Decomposed)
            .count();
        let lost_gate_types = emitter
            .gate_materializations
            .iter()
            .filter(|m| m.status == MaterializationStatus::Lost)
            .count();

        EmitterEffect {
            target: emitter.target.clone(),
            native_coverage: emitter.coverage.native_coverage,
            materializable_coverage: emitter.coverage.materializable_coverage,
            estimated_expansion: emitter.coverage.estimated_expansion,
            decomposed_gate_types,
            lost_gate_types,
            fully_materializable: emitter.fully_materializable,
            emission_success: emitter.emission.success,
        }
    }

    fn compute_compilation_effect(
        input: &InputAnalysis,
        observer: &CompilationObserver,
    ) -> Option<CompilationEffect> {
        let before_depth = input.structural_metrics.depth as i64;
        let after_depth = observer.final_metrics.depth as i64;
        let before_ops = input.structural_metrics.total_ops as i64;
        let after_ops = observer.final_metrics.total_ops as i64;
        let before_2q = input.structural_metrics.two_qubit_gates as i64;
        let after_2q = observer.final_metrics.two_qubit_gates as i64;

        Some(CompilationEffect {
            depth_delta: after_depth - before_depth,
            depth_ratio: if before_depth > 0 {
                after_depth as f64 / before_depth as f64
            } else {
                1.0
            },
            ops_delta: after_ops - before_ops,
            ops_ratio: if before_ops > 0 {
                after_ops as f64 / before_ops as f64
            } else {
                1.0
            },
            two_qubit_delta: after_2q - before_2q,
            two_qubit_ratio: if before_2q > 0 {
                after_2q as f64 / before_2q as f64
            } else {
                1.0
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compilation_effect_ratios() {
        let effect = CompilationEffect {
            depth_delta: -2,
            depth_ratio: 0.6,
            ops_delta: -3,
            ops_ratio: 0.7,
            two_qubit_delta: 0,
            two_qubit_ratio: 1.0,
        };

        assert!(effect.depth_ratio < 1.0);
        assert!(effect.ops_ratio < 1.0);
        assert_eq!(effect.two_qubit_ratio, 1.0);
    }
}
