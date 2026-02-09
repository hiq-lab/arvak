//! Evaluator endpoint: run arvak-eval analysis on a QASM3 circuit.

use std::sync::Arc;
use std::time::Instant;

use axum::{Json, extract::State};

use arvak_eval::{EvalConfig, Evaluator};

use crate::error::ApiError;
use crate::state::AppState;

use serde::{Deserialize, Serialize};

/// Request to run the evaluator.
#[derive(Debug, Deserialize)]
pub struct EvalRequest {
    /// QASM3 source code.
    pub qasm: String,
    /// Target backend (iqm, ibm, simulator).
    #[serde(default = "default_target")]
    pub target: String,
    /// Optimization level (0-3).
    #[serde(default = "default_opt_level")]
    pub optimization_level: u8,
    /// Number of qubits on target device.
    #[serde(default = "default_target_qubits")]
    pub target_qubits: u32,
    /// Enable orchestration analysis.
    #[serde(default)]
    pub orchestration: bool,
    /// Scheduler site (lrz, lumi, or null).
    pub scheduler_site: Option<String>,
    /// Emit target (iqm, ibm, cuda-q, or null).
    pub emit_target: Option<String>,
    /// Benchmark suite (ghz, qft, grover, random, or null).
    pub benchmark: Option<String>,
    /// Number of qubits for benchmark.
    pub benchmark_qubits: Option<usize>,
}

fn default_target() -> String {
    "iqm".into()
}
fn default_opt_level() -> u8 {
    1
}
fn default_target_qubits() -> u32 {
    20
}

/// Compact contract gate summary for the dashboard.
#[derive(Debug, Serialize)]
pub struct ContractGateSummary {
    pub gate: String,
    pub tag: String,
}

/// Compact emitter gate summary for the dashboard.
#[derive(Debug, Serialize)]
pub struct EmitterGateSummary {
    pub gate: String,
    pub count: usize,
    pub status: String,
    pub cost: Option<usize>,
}

/// Compact loss entry for the dashboard.
#[derive(Debug, Serialize)]
pub struct LossSummary {
    pub capability: String,
    pub category: String,
    pub impact: String,
    pub cost: Option<usize>,
}

/// Scheduler fitness view for the dashboard.
#[derive(Debug, Serialize)]
pub struct SchedulerView {
    pub site: String,
    pub partition: String,
    pub qubits_fit: bool,
    pub fits_walltime: bool,
    pub fitness_score: f64,
    pub recommended_walltime: u64,
    pub batch_capacity: u32,
    pub assessment: String,
}

/// Orchestration summary for the dashboard.
#[derive(Debug, Serialize)]
pub struct OrchestrationView {
    pub quantum_phases: usize,
    pub classical_phases: usize,
    pub critical_path_cost: f64,
    pub critical_path_length: usize,
    pub max_parallel_quantum: usize,
    pub parallelism_ratio: f64,
    pub is_purely_quantum: bool,
    /// Nodes for hybrid DAG visualization.
    pub nodes: Vec<HybridNodeView>,
    /// Edges for hybrid DAG visualization.
    pub edges: Vec<HybridEdgeView>,
}

/// Hybrid DAG node for D3 visualization.
#[derive(Debug, Serialize)]
pub struct HybridNodeView {
    pub index: usize,
    pub kind: String,
    pub label: String,
    pub cost: f64,
    pub depth: Option<usize>,
    pub gate_count: Option<usize>,
}

/// Hybrid DAG edge for D3 visualization.
#[derive(Debug, Serialize)]
pub struct HybridEdgeView {
    pub from: usize,
    pub to: usize,
    pub data_flow: String,
}

/// Emitter compliance view for the dashboard.
#[derive(Debug, Serialize)]
pub struct EmitterView {
    pub target: String,
    pub native_coverage: f64,
    pub materializable_coverage: f64,
    pub estimated_expansion: f64,
    pub fully_materializable: bool,
    pub emission_success: bool,
    pub emission_lines: Option<usize>,
    pub gates: Vec<EmitterGateSummary>,
    pub losses: Vec<LossSummary>,
}

/// Benchmark info for the dashboard.
#[derive(Debug, Serialize)]
pub struct BenchmarkView {
    pub name: String,
    pub num_qubits: usize,
    pub expected_gates: usize,
}

/// Full evaluator response.
#[derive(Debug, Serialize)]
pub struct EvalResponse {
    pub schema_version: String,
    pub profile: String,
    /// Input analysis
    pub input: InputView,
    /// Compilation deltas
    pub compilation: CompilationView,
    /// QDMI contract
    pub contract: ContractView,
    /// Orchestration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationView>,
    /// Scheduler fitness (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<SchedulerView>,
    /// Emitter compliance (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitter: Option<EmitterView>,
    /// Benchmark info (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkView>,
    /// Aggregated metrics
    pub metrics: MetricsView,
}

/// Input summary.
#[derive(Debug, Serialize)]
pub struct InputView {
    pub num_qubits: usize,
    pub num_clbits: usize,
    pub total_ops: usize,
    pub depth: usize,
    pub content_hash: String,
}

/// Compilation summary.
#[derive(Debug, Serialize)]
pub struct CompilationView {
    pub num_passes: usize,
    pub original_depth: usize,
    pub compiled_depth: usize,
    pub original_ops: usize,
    pub compiled_ops: usize,
    pub depth_delta: i64,
    pub ops_delta: i64,
    pub compile_time_us: u64,
    pub throughput_gates_per_sec: u64,
}

/// Contract summary.
#[derive(Debug, Serialize)]
pub struct ContractView {
    pub target_name: String,
    pub compliant: bool,
    pub safe_count: usize,
    pub conditional_count: usize,
    pub violating_count: usize,
    pub gates: Vec<ContractGateSummary>,
}

/// Aggregated metrics summary.
#[derive(Debug, Serialize)]
pub struct MetricsView {
    pub depth_ratio: Option<f64>,
    pub ops_ratio: Option<f64>,
    pub two_qubit_ratio: Option<f64>,
    pub safe_fraction: f64,
    pub conditional_fraction: f64,
    pub violating_fraction: f64,
    pub scheduler_fitness: Option<f64>,
    pub native_coverage: Option<f64>,
    pub materializable_coverage: Option<f64>,
}

/// POST /api/eval - Run the evaluator on a QASM3 circuit.
pub async fn evaluate(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<EvalRequest>,
) -> Result<Json<EvalResponse>, ApiError> {
    let config = EvalConfig {
        profile: "dashboard".into(),
        optimization_level: req.optimization_level,
        target: req.target.clone(),
        target_qubits: req.target_qubits,
        orchestration: req.orchestration,
        scheduler_site: req.scheduler_site,
        emit_target: req.emit_target,
        benchmark: req.benchmark,
        benchmark_qubits: req.benchmark_qubits,
        ..Default::default()
    };

    let evaluator = Evaluator::new(config);
    let eval_start = Instant::now();
    let report = evaluator
        .evaluate(&req.qasm, &[])
        .map_err(|e| ApiError::Internal(format!("Evaluation failed: {e}")))?;
    let eval_time = eval_start.elapsed();

    // Build response from report
    let input = InputView {
        num_qubits: report.input.num_qubits,
        num_clbits: report.input.num_clbits,
        total_ops: report.input.total_ops,
        depth: report.input.depth,
        content_hash: report.input.content_hash.clone(),
    };

    let compile_time_us = eval_time.as_micros() as u64;
    let compiled_ops = report.compilation.final_snapshot.total_ops;
    let throughput_gates_per_sec = if eval_time.as_nanos() > 0 {
        (compiled_ops as f64 / eval_time.as_secs_f64()) as u64
    } else {
        0
    };

    let compilation = CompilationView {
        num_passes: report.compilation.passes.len(),
        original_depth: report.compilation.initial.depth,
        compiled_depth: report.compilation.final_snapshot.depth,
        original_ops: report.compilation.initial.total_ops,
        compiled_ops,
        depth_delta: report
            .metrics
            .compilation_effect
            .as_ref()
            .map_or(0, |e| e.depth_delta),
        ops_delta: report
            .metrics
            .compilation_effect
            .as_ref()
            .map_or(0, |e| e.ops_delta),
        compile_time_us,
        throughput_gates_per_sec,
    };

    let contract = ContractView {
        target_name: report.contract.target_name.clone(),
        compliant: report.contract.compliant,
        safe_count: report.contract.safe_count,
        conditional_count: report.contract.conditional_count,
        violating_count: report.contract.violating_count,
        gates: report
            .contract
            .gate_summary
            .iter()
            .map(|(gate, tag)| ContractGateSummary {
                gate: gate.clone(),
                tag: tag.to_string(),
            })
            .collect(),
    };

    let orchestration = report.orchestration.as_ref().map(|orch| OrchestrationView {
        quantum_phases: orch.summary.quantum_phases,
        classical_phases: orch.summary.classical_phases,
        critical_path_cost: orch.critical_path.total_cost,
        critical_path_length: orch.critical_path.node_indices.len(),
        max_parallel_quantum: orch.batchability.max_parallel_quantum,
        parallelism_ratio: orch.batchability.parallelism_ratio,
        is_purely_quantum: orch.batchability.is_purely_quantum,
        nodes: orch
            .dag
            .nodes
            .iter()
            .map(|n| HybridNodeView {
                index: n.index,
                kind: n.kind.to_string(),
                label: n.label.clone(),
                cost: n.estimated_cost,
                depth: n.depth,
                gate_count: n.gate_count,
            })
            .collect(),
        edges: orch
            .dag
            .edges
            .iter()
            .map(|e| HybridEdgeView {
                from: e.from,
                to: e.to,
                data_flow: format!("{:?}", e.data_flow),
            })
            .collect(),
    });

    let scheduler = report.scheduler.as_ref().map(|sched| SchedulerView {
        site: sched.constraints.site.clone(),
        partition: sched.constraints.partition.clone(),
        qubits_fit: sched.qubits_fit,
        fits_walltime: sched.walltime.fits_walltime,
        fitness_score: sched.fitness_score,
        recommended_walltime: sched.walltime.recommended_walltime,
        batch_capacity: sched.walltime.batch_capacity,
        assessment: sched.assessment.clone(),
    });

    let emitter = report.emitter.as_ref().map(|em| EmitterView {
        target: em.target.clone(),
        native_coverage: em.coverage.native_coverage,
        materializable_coverage: em.coverage.materializable_coverage,
        estimated_expansion: em.coverage.estimated_expansion,
        fully_materializable: em.fully_materializable,
        emission_success: em.emission.success,
        emission_lines: em.emission.line_count,
        gates: em
            .gate_materializations
            .iter()
            .map(|g| EmitterGateSummary {
                gate: g.gate_name.clone(),
                count: g.count,
                status: format!("{:?}", g.status),
                cost: g.decomposition_cost,
            })
            .collect(),
        losses: em
            .losses
            .iter()
            .map(|l| LossSummary {
                capability: l.capability.clone(),
                category: format!("{:?}", l.category),
                impact: l.impact.clone(),
                cost: l.native_gate_cost,
            })
            .collect(),
    });

    let benchmark_view = report.benchmark.as_ref().map(|b| BenchmarkView {
        name: b.name.clone(),
        num_qubits: b.num_qubits,
        expected_gates: b.expected_gates,
    });

    let metrics = MetricsView {
        depth_ratio: report
            .metrics
            .compilation_effect
            .as_ref()
            .map(|e| e.depth_ratio),
        ops_ratio: report
            .metrics
            .compilation_effect
            .as_ref()
            .map(|e| e.ops_ratio),
        two_qubit_ratio: report
            .metrics
            .compilation_effect
            .as_ref()
            .map(|e| e.two_qubit_ratio),
        safe_fraction: report.metrics.compliance.safe_fraction,
        conditional_fraction: report.metrics.compliance.conditional_fraction,
        violating_fraction: report.metrics.compliance.violating_fraction,
        scheduler_fitness: report
            .metrics
            .orchestration_effect
            .as_ref()
            .and_then(|o| o.scheduler_fitness),
        native_coverage: report
            .metrics
            .emitter_effect
            .as_ref()
            .map(|e| e.native_coverage),
        materializable_coverage: report
            .metrics
            .emitter_effect
            .as_ref()
            .map(|e| e.materializable_coverage),
    };

    Ok(Json(EvalResponse {
        schema_version: report.schema_version,
        profile: report.profile,
        input,
        compilation,
        contract,
        orchestration,
        scheduler,
        emitter,
        benchmark: benchmark_view,
        metrics,
    }))
}
