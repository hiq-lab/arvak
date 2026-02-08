//! Evaluation report structure.
//!
//! The top-level report combining all evaluation module outputs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::contract::ContractReport;
use crate::input::InputReport;
use crate::metrics::AggregatedMetrics;
use crate::observer::CompilationReport;
use crate::reproducibility::ReproducibilityInfo;

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
    /// Reproducibility information.
    pub reproducibility: ReproducibilityInfo,
}
