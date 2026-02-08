//! Evaluation report structure.
//!
//! The top-level report combining all evaluation module outputs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::benchmark::BenchmarkCircuit;
use crate::contract::ContractReport;
use crate::emitter::EmitterReport;
use crate::input::InputReport;
use crate::metrics::AggregatedMetrics;
use crate::observer::CompilationReport;
use crate::orchestration::OrchestrationReport;
use crate::reproducibility::ReproducibilityInfo;
use crate::scheduler_context::SchedulerFitness;

/// Complete evaluation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// Timestamp of the evaluation.
    pub timestamp: DateTime<Utc>,
    /// Evaluation profile used.
    pub profile: String,
    /// Input analysis results.
    pub input: InputReport,
    /// Compilation observation results.
    pub compilation: CompilationReport,
    /// QDMI contract compliance results.
    pub contract: ContractReport,
    /// Aggregated metrics.
    pub metrics: AggregatedMetrics,
    /// Orchestration analysis (present when --orchestration is used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationReport>,
    /// Scheduler fitness assessment (present when --orchestration is used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<SchedulerFitness>,
    /// Emitter compliance report (present when --emit is used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitter: Option<EmitterReport>,
    /// Benchmark circuit info (present when --benchmark is used).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkCircuit>,
    /// Reproducibility information.
    pub reproducibility: ReproducibilityInfo,
}
