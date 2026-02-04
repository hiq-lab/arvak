//! Compile command implementation.

use anyhow::{Context, Result};
use console::style;
use std::fs;
use std::path::Path;

use hiq_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use hiq_ir::Circuit;
use hiq_qasm3::{emit, parse};

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
        Box::leak(format!("{}_compiled.qasm", stem).into_boxed_str())
    });

    save_circuit(&compiled, output_path)?;
    println!("  Output: {}", style(output_path).green());

    Ok(())
}

/// Load a circuit from a file.
fn load_circuit(path: &str) -> Result<Circuit> {
    let path_obj = Path::new(path);

    if !path_obj.exists() {
        anyhow::bail!("File not found: {}", path);
    }

    let source =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))?;

    // Determine format by extension
    let ext = path_obj.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext.to_lowercase().as_str() {
        "qasm" | "qasm3" => parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e)),
        "json" => {
            // TODO: Add JSON circuit format
            anyhow::bail!("JSON format not yet supported")
        }
        _ => {
            // Try QASM3 by default
            parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e))
        }
    }
}

/// Save a circuit to a file.
fn save_circuit(circuit: &Circuit, path: &str) -> Result<()> {
    let path_obj = Path::new(path);
    let ext = path_obj
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("qasm");

    let content = match ext.to_lowercase().as_str() {
        "qasm" | "qasm3" => emit(circuit).map_err(|e| anyhow::anyhow!("Emit error: {}", e))?,
        "json" => {
            // TODO: Add JSON circuit format
            anyhow::bail!("JSON format not yet supported")
        }
        _ => emit(circuit).map_err(|e| anyhow::anyhow!("Emit error: {}", e))?,
    };

    fs::write(path, content).with_context(|| format!("Failed to write file: {}", path))?;

    Ok(())
}

/// Get target properties (coupling map and basis gates).
fn get_target_properties(target: &str) -> Result<(CouplingMap, BasisGates)> {
    match target.to_lowercase().as_str() {
        "iqm" | "iqm5" => Ok((CouplingMap::star(5), BasisGates::iqm())),
        "iqm20" => Ok((CouplingMap::star(20), BasisGates::iqm())),
        "ibm" | "ibm5" => Ok((CouplingMap::linear(5), BasisGates::ibm())),
        "ibm27" => Ok((CouplingMap::linear(27), BasisGates::ibm())),
        "simulator" | "sim" => Ok((CouplingMap::full(20), BasisGates::universal())),
        other => {
            anyhow::bail!(
                "Unknown target: '{}'. Available: iqm, iqm5, iqm20, ibm, ibm5, ibm27, simulator",
                other
            );
        }
    }
}
