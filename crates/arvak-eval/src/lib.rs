//! Arvak Evaluator: Compiler & Orchestration Observability
//!
//! This crate provides the evaluation framework for observing and analyzing
//! quantum circuit compilation pipelines. It is QDMI-first and independent
//! of external benchmark frameworks.
//!
//! # Overview
//!
//! The evaluator operates on OpenQASM 3.0 circuits and produces structured
//! reports covering:
//!
//! - **Input Analysis**: Parsing, validation, and content hashing
//! - **Compilation Observation**: Pass-wise metrics with before/after deltas
//! - **QDMI Contract Checking**: Safety classification against device capabilities
//! - **Metrics Aggregation**: Structural and compilation effect metrics
//! - **Reproducibility**: CLI snapshots, versioning, and deterministic exports
//!
//! # Architecture
//!
//! ```text
//! [QASM3 Input] -> Input Module -> Compilation Observer
//!                                    |
//!                                    v
//!                            QDMI Contract Module
//!                                    |
//!                                    v
//!                           Metrics Aggregator
//!                                    |
//!                                    v
//!                         Reproducibility Module
//!                                    |
//!                                    v
//!                                JSON Output
//! ```
//!
//! # Example
//!
//! ```ignore
//! use arvak_eval::{Evaluator, EvalConfig};
//!
//! let config = EvalConfig::default();
//! let evaluator = Evaluator::new(config);
//! let report = evaluator.evaluate_file("circuit.qasm3")?;
//! println!("{}", serde_json::to_string_pretty(&report)?);
//! ```

pub mod contract;
pub mod error;
pub mod export;
pub mod input;
pub mod metrics;
pub mod observer;
pub mod report;
pub mod reproducibility;

pub use error::{EvalError, EvalResult};
pub use report::EvalReport;

use input::InputAnalysis;
use metrics::MetricsAggregator;
use observer::CompilationObserver;
use contract::ContractChecker;
use reproducibility::ReproducibilityInfo;
use export::ExportConfig;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_hal::Capabilities;
use tracing::info;

/// Evaluation profile controlling compilation target and observation depth.
#[derive(Debug, Clone)]
pub struct EvalConfig {
    /// Name of this evaluation profile.
    pub profile: String,
    /// Optimization level for compilation (0-3).
    pub optimization_level: u8,
    /// Target backend name (iqm, ibm, simulator).
    pub target: String,
    /// Number of qubits on target device.
    pub target_qubits: u32,
    /// Export configuration.
    pub export: ExportConfig,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            optimization_level: 1,
            target: "iqm".into(),
            target_qubits: 20,
            export: ExportConfig::default(),
        }
    }
}

impl EvalConfig {
    /// Build target capabilities from the config.
    pub fn target_capabilities(&self) -> Capabilities {
        match self.target.as_str() {
            "ibm" => Capabilities::ibm(&self.target, self.target_qubits),
            "simulator" => Capabilities::simulator(self.target_qubits),
            // Default to IQM
            _ => Capabilities::iqm(&self.target, self.target_qubits),
        }
    }

    /// Build coupling map and basis gates for compilation.
    fn build_target_properties(&self) -> (CouplingMap, BasisGates) {
        match self.target.as_str() {
            "ibm" => (CouplingMap::linear(self.target_qubits), BasisGates::ibm()),
            "simulator" => (CouplingMap::full(self.target_qubits), BasisGates::universal()),
            _ => (CouplingMap::star(self.target_qubits), BasisGates::iqm()),
        }
    }
}

/// The main evaluator orchestrating all modules.
pub struct Evaluator {
    config: EvalConfig,
}

impl Evaluator {
    /// Create a new evaluator with the given configuration.
    pub fn new(config: EvalConfig) -> Self {
        Self { config }
    }

    /// Evaluate an OpenQASM 3.0 source string.
    pub fn evaluate(&self, qasm_source: &str, cli_args: &[String]) -> EvalResult<EvalReport> {
        info!("Starting evaluation with profile '{}'", self.config.profile);

        // 1. Input analysis
        let input_analysis = InputAnalysis::analyze(qasm_source)?;
        let circuit = input_analysis.circuit.clone();

        info!(
            "Input: {} qubits, {} ops, depth {}",
            input_analysis.structural_metrics.num_qubits,
            input_analysis.structural_metrics.total_ops,
            input_analysis.structural_metrics.depth,
        );

        // 2. Compilation observation
        let (coupling_map, basis_gates) = self.config.build_target_properties();
        let (pm, mut props) = PassManagerBuilder::new()
            .with_optimization_level(self.config.optimization_level)
            .with_target(coupling_map, basis_gates)
            .build();

        let mut dag = circuit.into_dag();
        let observer = CompilationObserver::observe(&pm, &mut dag, &mut props)?;

        info!(
            "Compilation: {} passes observed, final depth {}",
            observer.pass_records.len(),
            observer.final_metrics.depth,
        );

        // 3. QDMI contract check
        let capabilities = self.config.target_capabilities();
        let contract_report = ContractChecker::check(&observer.final_dag, &capabilities);

        info!(
            "Contract: {} safe, {} conditional, {} violating",
            contract_report.safe_count,
            contract_report.conditional_count,
            contract_report.violating_count,
        );

        // 4. Metrics aggregation
        let aggregated = MetricsAggregator::aggregate(
            &input_analysis,
            &observer,
            &contract_report,
        );

        // 5. Reproducibility
        let reproducibility = ReproducibilityInfo::capture(cli_args);

        // 6. Build report
        let report = EvalReport {
            schema_version: "0.1.0".into(),
            timestamp: chrono::Utc::now(),
            profile: self.config.profile.clone(),
            input: input_analysis.into_report(),
            compilation: observer.into_report(),
            contract: contract_report,
            metrics: aggregated,
            reproducibility,
        };

        Ok(report)
    }

    /// Evaluate an OpenQASM 3.0 file by path.
    pub fn evaluate_file(
        &self,
        path: &std::path::Path,
        cli_args: &[String],
    ) -> EvalResult<EvalReport> {
        let source = std::fs::read_to_string(path).map_err(|e| {
            EvalError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
        self.evaluate(&source, cli_args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELL_QASM: &str = r#"
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
"#;

    #[test]
    fn test_evaluator_basic() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert_eq!(report.schema_version, "0.1.0");
        assert_eq!(report.input.num_qubits, 2);
        assert!(report.input.total_ops >= 2);
        assert!(!report.input.content_hash.is_empty());
    }

    #[test]
    fn test_evaluator_iqm_target() {
        let config = EvalConfig {
            target: "iqm".into(),
            target_qubits: 20,
            optimization_level: 2,
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert!(!report.compilation.passes.is_empty());
        assert!(report.metrics.compilation_effect.is_some());
    }

    #[test]
    fn test_evaluator_json_export() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("schema_version"));
        assert!(json.contains("content_hash"));
    }
}
