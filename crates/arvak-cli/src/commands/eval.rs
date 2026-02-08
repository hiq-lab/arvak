//! Evaluator command implementation.
//!
//! `arvak eval --input <circuit.qasm3> --profile default`

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
) -> anyhow::Result<()> {
    // Build config from CLI args
    let config = EvalConfig {
        profile: profile.into(),
        optimization_level,
        target: target.into(),
        target_qubits,
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
    eprintln!(
        "  Contract:    {} safe, {} conditional, {} violating [{}]",
        report.contract.safe_count,
        report.contract.conditional_count,
        report.contract.violating_count,
        if report.contract.compliant {
            style("COMPLIANT").green()
        } else {
            style("NON-COMPLIANT").red()
        },
    );
    eprintln!(
        "  Target:      {} ({} qubits)",
        report.contract.target_name, report.contract.target_qubits
    );
    eprintln!("  Profile:     {}", report.profile);

    Ok(())
}
