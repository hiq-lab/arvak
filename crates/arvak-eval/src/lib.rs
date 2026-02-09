//! Arvak Evaluator: Compiler, Orchestration & Emitter Observability
//!
//! This crate provides the evaluation framework for observing and analyzing
//! quantum circuit compilation pipelines. It is QDMI-first and independent
//! of external benchmark frameworks.
//!
//! # Overview
//!
//! The evaluator operates on `OpenQASM` 3.0 circuits and produces structured
//! reports covering:
//!
//! - **Input Analysis**: Parsing, validation, and content hashing
//! - **Compilation Observation**: Pass-wise metrics with before/after deltas
//! - **QDMI Contract Checking**: Safety classification against device capabilities
//! - **Orchestration Analysis**: Hybrid DAG, critical path, batchability (v0.2)
//! - **Emitter Compliance**: Native gate coverage, loss documentation (v0.3)
//! - **Benchmark Loading**: Standard circuit workloads (GHZ, QFT, etc.) (v0.3)
//! - **Metrics Aggregation**: Compilation + Orchestration + Emitter deltas
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
//!                          Orchestration Module (opt)
//!                                    |
//!                                    v
//!                       Emitter Compliance Module (opt)
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

pub mod benchmark;
pub mod contract;
pub mod emitter;
pub mod error;
pub mod export;
pub mod input;
pub mod metrics;
pub mod observer;
pub mod orchestration;
pub mod report;
pub mod reproducibility;
pub mod scheduler_context;

pub use error::{EvalError, EvalResult};
pub use report::EvalReport;

use benchmark::{BenchmarkLoader, BenchmarkSuite};
use contract::ContractChecker;
use emitter::{EmitTarget, EmitterAnalyzer};
use export::ExportConfig;
use input::InputAnalysis;
use metrics::MetricsAggregator;
use observer::CompilationObserver;
use orchestration::OrchestrationAnalyzer;
use reproducibility::ReproducibilityInfo;
use scheduler_context::{SchedulerConstraints, SchedulerContext};

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
    /// Enable orchestration analysis (hybrid DAG, batchability, critical path).
    pub orchestration: bool,
    /// HPC site for scheduler constraints (lrz, lumi, or None for auto-detect).
    pub scheduler_site: Option<String>,
    /// Emit target for emitter compliance analysis (iqm, ibm, cuda-q, or None).
    pub emit_target: Option<String>,
    /// Benchmark suite to use as workload input (ghz, qft, grover, random, or None).
    pub benchmark: Option<String>,
    /// Number of qubits for benchmark circuit generation.
    pub benchmark_qubits: Option<usize>,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            profile: "default".into(),
            optimization_level: 1,
            target: "iqm".into(),
            target_qubits: 20,
            export: ExportConfig::default(),
            orchestration: false,
            scheduler_site: None,
            emit_target: None,
            benchmark: None,
            benchmark_qubits: None,
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
            "simulator" => (
                CouplingMap::full(self.target_qubits),
                BasisGates::universal(),
            ),
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

    /// Evaluate an `OpenQASM` 3.0 source string.
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

        // 4. Orchestration analysis (optional)
        let (orchestration_report, scheduler_fitness) = if self.config.orchestration {
            let orch = OrchestrationAnalyzer::analyze(
                &observer.final_dag,
                input_analysis.structural_metrics.num_qubits,
            );

            info!(
                "Orchestration: {} quantum phases, {} classical phases, critical path cost {:.1}",
                orch.summary.quantum_phases,
                orch.summary.classical_phases,
                orch.critical_path.total_cost,
            );

            let constraints = match self.config.scheduler_site.as_deref() {
                Some("lrz") => SchedulerConstraints::lrz(),
                Some("lumi") => SchedulerConstraints::lumi(),
                Some("simulator") | None if self.config.target == "simulator" => {
                    SchedulerConstraints::simulator()
                }
                _ => SchedulerConstraints::lrz(), // Default to LRZ
            };

            let fitness = SchedulerContext::evaluate(
                input_analysis.structural_metrics.num_qubits,
                input_analysis.structural_metrics.depth,
                input_analysis.structural_metrics.total_ops,
                &constraints,
            );

            info!(
                "Scheduler: {} fitness={:.2}, batch_capacity={}",
                fitness.constraints.site, fitness.fitness_score, fitness.walltime.batch_capacity,
            );

            (Some(orch), Some(fitness))
        } else {
            (None, None)
        };

        // 5. Emitter compliance analysis (optional)
        let emitter_report = if let Some(emit_name) = &self.config.emit_target {
            let emit_target = EmitTarget::from_name(emit_name).unwrap_or_else(|| {
                // Default to matching the compilation target
                EmitTarget::from_name(&self.config.target).unwrap_or(EmitTarget::CudaQ)
            });

            let report =
                EmitterAnalyzer::analyze(&observer.final_dag, &emit_target, &capabilities)?;

            info!(
                "Emitter: {} target, {:.0}% native coverage, {:.0}% materializable, expansion {:.1}x",
                report.target,
                report.coverage.native_coverage * 100.0,
                report.coverage.materializable_coverage * 100.0,
                report.coverage.estimated_expansion,
            );

            Some(report)
        } else {
            None
        };

        // 6. Benchmark info (optional, non-normative)
        let benchmark_info = if let Some(bench_name) = &self.config.benchmark {
            let suite = BenchmarkSuite::from_name(bench_name);
            if let Some(suite) = suite {
                let num_qubits = self
                    .config
                    .benchmark_qubits
                    .unwrap_or(input_analysis.structural_metrics.num_qubits);
                match BenchmarkLoader::generate(&suite, num_qubits) {
                    Ok(bench) => {
                        info!(
                            "Benchmark: {} ({} qubits, {} gates)",
                            bench.name, bench.num_qubits, bench.expected_gates,
                        );
                        Some(bench)
                    }
                    Err(e) => {
                        info!("Benchmark generation failed: {}", e);
                        None
                    }
                }
            } else {
                info!("Unknown benchmark suite: {}", bench_name);
                None
            }
        } else {
            None
        };

        // 7. Metrics aggregation (unified: compilation + orchestration + emitter)
        let aggregated = MetricsAggregator::aggregate_full(
            &input_analysis,
            &observer,
            &contract_report,
            orchestration_report
                .as_ref()
                .map(|o| (o, scheduler_fitness.as_ref())),
            emitter_report.as_ref(),
        );

        // 8. Reproducibility
        let reproducibility = ReproducibilityInfo::capture(cli_args);

        // 9. Build report
        let report = EvalReport {
            schema_version: "0.3.0".into(),
            timestamp: chrono::Utc::now(),
            profile: self.config.profile.clone(),
            input: input_analysis.into_report(),
            compilation: observer.into_report(),
            contract: contract_report,
            metrics: aggregated,
            orchestration: orchestration_report,
            scheduler: scheduler_fitness,
            emitter: emitter_report,
            benchmark: benchmark_info,
            reproducibility,
        };

        Ok(report)
    }

    /// Evaluate an `OpenQASM` 3.0 file by path.
    pub fn evaluate_file(
        &self,
        path: &std::path::Path,
        cli_args: &[String],
    ) -> EvalResult<EvalReport> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| EvalError::Io(format!("Failed to read {}: {}", path.display(), e)))?;
        self.evaluate(&source, cli_args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELL_QASM: &str = r"
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
";

    #[test]
    fn test_evaluator_basic() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert_eq!(report.schema_version, "0.3.0");
        assert_eq!(report.input.num_qubits, 2);
        assert!(report.input.total_ops >= 2);
        assert!(!report.input.content_hash.is_empty());
        assert!(report.orchestration.is_none());
        assert!(report.emitter.is_none());
        assert!(report.benchmark.is_none());
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
    fn test_evaluator_with_orchestration() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            orchestration: true,
            scheduler_site: Some("lrz".into()),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert!(report.orchestration.is_some());
        assert!(report.scheduler.is_some());
        assert!(report.metrics.orchestration_effect.is_some());

        let orch = report.orchestration.unwrap();
        assert!(orch.summary.quantum_phases >= 1);
        assert!(orch.critical_path.total_cost > 0.0);

        let sched = report.scheduler.unwrap();
        assert!(sched.qubits_fit);
        assert_eq!(sched.constraints.site, "LRZ");
    }

    #[test]
    fn test_evaluator_orchestration_lumi() {
        let config = EvalConfig {
            target: "iqm".into(),
            target_qubits: 5,
            orchestration: true,
            scheduler_site: Some("lumi".into()),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        let sched = report.scheduler.unwrap();
        assert_eq!(sched.constraints.site, "LUMI");
        assert_eq!(sched.constraints.partition, "q_fiqci");
    }

    #[test]
    fn test_evaluator_with_emitter_iqm() {
        let config = EvalConfig {
            target: "iqm".into(),
            target_qubits: 20,
            emit_target: Some("iqm".into()),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert!(report.emitter.is_some());
        let emitter = report.emitter.unwrap();
        assert_eq!(emitter.target, "IQM");
        assert!(emitter.fully_materializable);
        assert!(emitter.emission.success);

        // Metrics should include emitter effect
        assert!(report.metrics.emitter_effect.is_some());
        let effect = report.metrics.emitter_effect.unwrap();
        assert!(effect.fully_materializable);
    }

    #[test]
    fn test_evaluator_with_emitter_ibm() {
        let config = EvalConfig {
            target: "ibm".into(),
            target_qubits: 20,
            emit_target: Some("ibm".into()),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert!(report.emitter.is_some());
        let emitter = report.emitter.unwrap();
        assert_eq!(emitter.target, "IBM");
    }

    #[test]
    fn test_evaluator_with_benchmark() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            benchmark: Some("ghz".into()),
            benchmark_qubits: Some(4),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert!(report.benchmark.is_some());
        let bench = report.benchmark.unwrap();
        assert_eq!(bench.num_qubits, 4);
        assert!(bench.qasm3_source.contains("OPENQASM 3.0"));
    }

    #[test]
    fn test_evaluator_full_pipeline() {
        // All features enabled: orchestration + emitter + benchmark
        let config = EvalConfig {
            target: "iqm".into(),
            target_qubits: 20,
            orchestration: true,
            scheduler_site: Some("lrz".into()),
            emit_target: Some("iqm".into()),
            benchmark: Some("qft".into()),
            benchmark_qubits: Some(3),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        assert_eq!(report.schema_version, "0.3.0");
        assert!(report.orchestration.is_some());
        assert!(report.scheduler.is_some());
        assert!(report.emitter.is_some());
        assert!(report.benchmark.is_some());
        assert!(report.metrics.orchestration_effect.is_some());
        assert!(report.metrics.emitter_effect.is_some());
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
        // Optional fields should not appear when disabled
        assert!(!json.contains("\"emitter\""));
        assert!(!json.contains("\"benchmark\""));
    }

    #[test]
    fn test_evaluator_json_with_emitter() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            emit_target: Some("cuda-q".into()),
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"emitter\""));
        assert!(json.contains("native_coverage"));
        assert!(json.contains("materializable_coverage"));
    }

    #[test]
    fn test_evaluator_json_with_orchestration() {
        let config = EvalConfig {
            target: "simulator".into(),
            target_qubits: 5,
            orchestration: true,
            ..Default::default()
        };
        let evaluator = Evaluator::new(config);
        let report = evaluator.evaluate(BELL_QASM, &[]).unwrap();

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("orchestration"));
        assert!(json.contains("critical_path"));
        assert!(json.contains("batchability"));
    }
}
