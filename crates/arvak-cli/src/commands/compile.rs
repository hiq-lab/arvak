//! Compile command implementation.

use anyhow::{Context, Result};
use console::style;
use std::fs;
use std::path::Path;

use arvak_compile::PassManagerBuilder;
use arvak_ir::Circuit;
use arvak_qasm3::emit;

use super::common::{get_target_properties, load_circuit};

/// Execute the compile command.
pub async fn execute(
    input: &str,
    output: Option<&str>,
    target: &str,
    optimization_level: u8,
) -> Result<()> {
    println!(
        "{} Compiling {} for target {}",
        style("→").cyan().bold(),
        style(input).green(),
        style(target).yellow()
    );

    // Load circuit
    let circuit = load_circuit(input)?;
    println!(
        "  Loaded: {} qubits, depth {}",
        circuit.num_qubits(),
        circuit.depth()
    );

    // Get target properties
    let (coupling_map, basis_gates) = get_target_properties(target)?;

    // Build pass manager
    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(optimization_level)
        .with_target(coupling_map, basis_gates)
        .build();

    println!(
        "  Running {} compilation passes (level {})",
        pm.len(),
        optimization_level
    );

    // Compile
    let mut dag = circuit.into_dag();
    pm.run(&mut dag, &mut props)?;

    let compiled = Circuit::from_dag(dag);

    println!("{} Compilation complete", style("✓").green().bold());
    println!(
        "  Result: depth {}, {} ops",
        compiled.depth(),
        compiled.dag().num_ops()
    );

    // Save output
    let output_path = output.unwrap_or_else(|| {
        // Default: replace extension with _compiled.qasm
        let p = Path::new(input);
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        Box::leak(format!("{stem}_compiled.qasm").into_boxed_str())
    });

    save_circuit(&compiled, output_path)?;
    println!("  Output: {}", style(output_path).green());

    Ok(())
}

/// Save a circuit to a file.
fn save_circuit(circuit: &Circuit, path: &str) -> Result<()> {
    let path_obj = Path::new(path);
    let ext = path_obj
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("qasm");

    let content = match ext.to_lowercase().as_str() {
        "qasm" | "qasm3" => emit(circuit).map_err(|e| anyhow::anyhow!("Emit error: {e}"))?,
        "json" => {
            anyhow::bail!("JSON format not yet supported")
        }
        _ => emit(circuit).map_err(|e| anyhow::anyhow!("Emit error: {e}"))?,
    };

    fs::write(path, content).with_context(|| format!("Failed to write file: {path}"))?;

    Ok(())
}
