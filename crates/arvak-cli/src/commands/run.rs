//! Run command implementation.

use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use arvak_adapter_sim::SimulatorBackend;
use arvak_compile::PassManagerBuilder;
use arvak_hal::Backend;
use arvak_ir::Circuit;

#[cfg(feature = "iqm")]
use arvak_adapter_iqm::IqmBackend;

#[cfg(feature = "ibm")]
use arvak_adapter_ibm::IbmBackend;

use super::common::{get_target_properties, load_circuit, print_results};

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
            anyhow::bail!("Unknown backend: '{other}'. Available: simulator, iqm, ibm");
        }
    };

    // Check availability
    if !backend_impl.is_available().await? {
        anyhow::bail!("Backend '{backend}' is not available");
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
    spinner.set_message(format!("Running job {job_id}..."));

    // Wait for result
    let result = backend_impl.wait(&job_id).await?;
    spinner.finish_and_clear();

    // Print results
    print_results(&result);

    Ok(())
}
