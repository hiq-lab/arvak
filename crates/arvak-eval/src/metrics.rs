//! Metrics Aggregator: structural and compilation effect metrics.
//!
//! Combines data from input analysis, compilation observation, and
//! contract checking into unified evaluation metrics.

use serde::{Deserialize, Serialize};

use crate::contract::ContractReport;
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

/// QDMI compliance summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceSummary {
    /// Whether the circuit is fully compliant.
    pub compliant: bool,
    /// Fraction of safe operations.
    pub safe_fraction: f64,
    /// Fraction of conditional operations.
    pub conditional_fraction: f64,
    /// Fraction of violating operations.
    pub violating_fraction: f64,
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

/// Aggregated evaluation metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    /// Compilation effect (None if no compilation was performed).
    pub compilation_effect: Option<CompilationEffect>,
    /// QDMI compliance summary.
    pub compliance: ComplianceSummary,
    /// Orchestration effect (None if --orchestration not used).
    pub orchestration_effect: Option<OrchestrationEffect>,
}

/// Aggregator for combining metrics from all modules.
pub struct MetricsAggregator;

impl MetricsAggregator {
    /// Aggregate metrics from all evaluation modules.
    pub fn aggregate(
        input: &InputAnalysis,
        observer: &CompilationObserver,
        contract: &ContractReport,
    ) -> AggregatedMetrics {
        let compilation_effect = Self::compute_compilation_effect(input, observer);
        let compliance = Self::compute_compliance(contract);

        AggregatedMetrics {
            compilation_effect,
            compliance,
            orchestration_effect: None,
        }
    }

    /// Aggregate metrics including orchestration analysis.
    pub fn aggregate_with_orchestration(
        input: &InputAnalysis,
        observer: &CompilationObserver,
        contract: &ContractReport,
        orchestration: &OrchestrationReport,
        scheduler_fitness: Option<&SchedulerFitness>,
    ) -> AggregatedMetrics {
        let compilation_effect = Self::compute_compilation_effect(input, observer);
        let compliance = Self::compute_compliance(contract);
        let orchestration_effect =
            Some(Self::compute_orchestration_effect(orchestration, scheduler_fitness));

        AggregatedMetrics {
            compilation_effect,
            compliance,
            orchestration_effect,
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

    fn compute_compliance(contract: &ContractReport) -> ComplianceSummary {
        let total = contract.safe_count + contract.conditional_count + contract.violating_count;
        let total_f = total.max(1) as f64;

        ComplianceSummary {
            compliant: contract.compliant,
            safe_fraction: contract.safe_count as f64 / total_f,
            conditional_fraction: contract.conditional_count as f64 / total_f,
            violating_fraction: contract.violating_count as f64 / total_f,
        }
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

    #[test]
    fn test_compliance_summary() {
        let summary = ComplianceSummary {
            compliant: true,
            safe_fraction: 0.8,
            conditional_fraction: 0.2,
            violating_fraction: 0.0,
        };

        assert!(summary.compliant);
        assert!((summary.safe_fraction + summary.conditional_fraction + summary.violating_fraction - 1.0).abs() < 1e-10);
    }
}
