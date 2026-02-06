//! Run command implementation.

use anyhow::{Context, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::Path;

use hiq_adapter_sim::SimulatorBackend;
use hiq_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use hiq_hal::Backend;
use hiq_ir::Circuit;
use hiq_qasm3::parse;

#[cfg(feature = "iqm")]
use hiq_adapter_iqm::IqmBackend;

#[cfg(feature = "ibm")]
use hiq_adapter_ibm::IbmBackend;

/// Execute the run command.
pub async fn execute(
    input: &str,
    shots: u32,
    backend: &str,
    do_compile: bool,
    target: Option<&str>,
) -> Result<()> {
    println!(
        "{} Running {} on {} ({} shots)",
        style("→").cyan().bold(),
        style(input).green(),
        style(backend).yellow(),
        shots
    );

    // Load circuit
    let mut circuit = load_circuit(input)?;
    println!(
        "  Loaded: {} qubits, depth {}",
        circuit.num_qubits(),
        circuit.depth()
    );

    // Compile if requested
    if do_compile {
        let target = target.unwrap_or(backend);
        println!("  Compiling for target: {}", style(target).yellow());

        let (coupling_map, basis_gates) = get_target_properties(target)?;
        let (pm, mut props) = PassManagerBuilder::new()
            .with_optimization_level(1)
            .with_target(coupling_map, basis_gates)
            .build();

        let mut dag = circuit.into_dag();
        pm.run(&mut dag, &mut props)?;
        circuit = Circuit::from_dag(dag);

        println!(
            "  Compiled: depth {}, {} ops",
            circuit.depth(),
            circuit.dag().num_ops()
        );
    }

    // Create backend
    let backend_impl: Box<dyn Backend> = match backend.to_lowercase().as_str() {
        "simulator" | "sim" => Box::new(SimulatorBackend::new()),
        #[cfg(feature = "iqm")]
        "iqm" | "garnet" => {
            println!("  Connecting to IQM Resonance...");
            match IqmBackend::new() {
                Ok(b) => Box::new(b),
                Err(e) => {
                    anyhow::bail!(
                        "Failed to connect to IQM: {}. Set IQM_TOKEN environment variable.",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "iqm"))]
        "iqm" | "garnet" => {
            anyhow::bail!("IQM backend not available. Rebuild with --features iqm");
        }
        #[cfg(feature = "ibm")]
        "ibm" | "ibmq" | "ibm_brisbane" | "ibm_kyoto" | "ibm_osaka" => {
            println!("  Connecting to IBM Quantum...");
            match IbmBackend::with_target(backend) {
                Ok(b) => Box::new(b),
                Err(e) => {
                    anyhow::bail!(
                        "Failed to connect to IBM Quantum: {}. Set IBM_QUANTUM_TOKEN environment variable.",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "ibm"))]
        "ibm" | "ibmq" | "ibm_brisbane" | "ibm_kyoto" | "ibm_osaka" => {
            anyhow::bail!("IBM backend not available. Rebuild with --features ibm");
        }
        other => {
            anyhow::bail!(
                "Unknown backend: '{}'. Available: simulator, iqm, ibm",
                other
            );
        }
    };

    // Check availability
    if !backend_impl.is_available().await? {
        anyhow::bail!("Backend '{}' is not available", backend);
    }

    // Submit job
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Submitting job...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let job_id = backend_impl.submit(&circuit, shots).await?;
    spinner.set_message(format!("Running job {}...", job_id));

    // Wait for result
    let result = backend_impl.wait(&job_id).await?;
    spinner.finish_and_clear();

    // Print results
    println!(
        "\n{} Results ({} shots):",
        style("✓").green().bold(),
        result.shots
    );

    let sorted = result.counts.sorted();
    let total = result.counts.total_shots() as f64;

    for (bitstring, count) in sorted.iter().take(16) {
        let prob = **count as f64 / total * 100.0;
        let bar_len = (prob / 2.0).round() as usize;
        let bar: String = "█".repeat(bar_len);

        println!(
            "  {}: {:>6} ({:>5.2}%) {}",
            style(bitstring).cyan(),
            count,
            prob,
            style(bar).green()
        );
    }

    if sorted.len() > 16 {
        println!("  ... and {} more outcomes", sorted.len() - 16);
    }

    if let Some(time_ms) = result.execution_time_ms {
        println!("\n  Execution time: {} ms", style(time_ms).yellow());
    }

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

    parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e))
}

/// Get target properties.
fn get_target_properties(target: &str) -> Result<(CouplingMap, BasisGates)> {
    match target.to_lowercase().as_str() {
        "iqm" | "iqm5" => Ok((CouplingMap::star(5), BasisGates::iqm())),
        "iqm20" => Ok((CouplingMap::star(20), BasisGates::iqm())),
        "ibm" | "ibm5" => Ok((CouplingMap::linear(5), BasisGates::ibm())),
        "simulator" | "sim" => Ok((CouplingMap::full(20), BasisGates::universal())),
        other => {
            anyhow::bail!("Unknown target: {}", other);
        }
    }
}
