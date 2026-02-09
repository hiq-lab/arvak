//! Evaluator command implementation.
//!
//! `arvak eval --input <circuit.qasm3> --profile default [--orchestration] [--emit <backend>] [--benchmark <suite>]`

use console::style;
use std::path::Path;

use arvak_eval::{EvalConfig, Evaluator};

/// Execute the eval command.
pub async fn execute(
    input: &str,
    profile: &str,
    target: &str,
    optimization_level: u8,
    output: Option<&str>,
    target_qubits: u32,
    orchestration: bool,
    scheduler_site: Option<&str>,
    emit: Option<&str>,
    benchmark: Option<&str>,
    benchmark_qubits: Option<usize>,
) -> anyhow::Result<()> {
    // Build config from CLI args
    let config = EvalConfig {
        profile: profile.into(),
        optimization_level,
        target: target.into(),
        target_qubits,
        orchestration,
        scheduler_site: scheduler_site.map(std::string::ToString::to_string),
        emit_target: emit.map(std::string::ToString::to_string),
        benchmark: benchmark.map(std::string::ToString::to_string),
        benchmark_qubits,
        ..Default::default()
    };

    // Capture CLI args for reproducibility
    let cli_args: Vec<String> = std::env::args().collect();

    // Run evaluation
    let evaluator = Evaluator::new(config.clone());
    let path = Path::new(input);
    let report = evaluator.evaluate_file(path, &cli_args)?;

    // Output
    let json = arvak_eval::export::to_json(&report, &config.export)?;

    if let Some(output_path) = output {
        arvak_eval::export::to_file(&report, Path::new(output_path), &config.export)?;
        eprintln!(
            "{} Report written to {}",
            style("OK").green().bold(),
            output_path
        );
    } else {
        println!("{json}");
    }

    // Print summary to stderr
    eprintln!();
    eprintln!("{}", style("Evaluation Summary").bold().underlined());
    eprintln!(
        "  Input:       {} qubits, {} ops, depth {}",
        report.input.num_qubits, report.input.total_ops, report.input.depth
    );
    eprintln!(
        "  Compiled:    depth {} -> {}, ops {} -> {}",
        report.compilation.initial.depth,
        report.compilation.final_snapshot.depth,
        report.compilation.initial.total_ops,
        report.compilation.final_snapshot.total_ops,
    );
    // Orchestration summary (if enabled)
    if let Some(ref orch) = report.orchestration {
        eprintln!();
        eprintln!("{}", style("Orchestration").bold().underlined());
        eprintln!(
            "  Hybrid DAG:  {} nodes ({} quantum, {} classical), {} edges",
            orch.summary.total_nodes,
            orch.summary.quantum_phases,
            orch.summary.classical_phases,
            orch.summary.total_edges,
        );
        eprintln!(
            "  Critical:    path length {}, cost {:.1}",
            orch.critical_path.node_indices.len(),
            orch.critical_path.total_cost,
        );
        eprintln!(
            "  Batch:       max {} parallel, ratio {:.2}{}",
            orch.batchability.max_parallel_quantum,
            orch.batchability.parallelism_ratio,
            if orch.batchability.is_purely_quantum {
                " (purely quantum)"
            } else {
                ""
            },
        );
    }

    if let Some(ref sched) = report.scheduler {
        eprintln!(
            "  Scheduler:   {} ({}) fitness={:.2}, batch_cap={}",
            sched.constraints.site,
            sched.constraints.partition,
            sched.fitness_score,
            sched.walltime.batch_capacity,
        );
        eprintln!("  Assessment:  {}", sched.assessment);
    }

    // Emitter compliance summary (if enabled)
    if let Some(ref emitter) = report.emitter {
        eprintln!();
        eprintln!("{}", style("Emitter Compliance").bold().underlined());
        eprintln!(
            "  Target:      {} [{}]",
            emitter.target,
            if emitter.fully_materializable {
                style("MATERIALIZABLE").green()
            } else {
                style("INCOMPLETE").red()
            },
        );
        eprintln!(
            "  Coverage:    {:.0}% native, {:.0}% materializable, {:.1}x expansion",
            emitter.coverage.native_coverage * 100.0,
            emitter.coverage.materializable_coverage * 100.0,
            emitter.coverage.estimated_expansion,
        );
        eprintln!(
            "  Gates:       {} total, {} native, {} decomposed, {} lost",
            emitter.coverage.total_gates,
            emitter.coverage.native_count,
            emitter.coverage.decomposed_count,
            emitter.coverage.lost_count,
        );
        if !emitter.losses.is_empty() {
            eprintln!("  Losses:");
            for loss in &emitter.losses {
                eprintln!("    - {}: {}", loss.capability, loss.impact);
            }
        }
        eprintln!(
            "  Emission:    {}",
            if emitter.emission.success {
                format!("OK ({} lines)", emitter.emission.line_count.unwrap_or(0))
            } else {
                format!(
                    "FAILED: {}",
                    emitter.emission.error.as_deref().unwrap_or("unknown")
                )
            },
        );
    }

    // Benchmark info (if provided)
    if let Some(ref bench) = report.benchmark {
        eprintln!();
        eprintln!("{}", style("Benchmark (non-normative)").bold().underlined());
        eprintln!(
            "  Suite:       {} ({} qubits, {} gates)",
            bench.name, bench.num_qubits, bench.expected_gates,
        );
    }

    eprintln!("  Profile:     {}", report.profile);

    Ok(())
}
