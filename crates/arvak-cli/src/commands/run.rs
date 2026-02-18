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

#[cfg(feature = "braket")]
use arvak_adapter_braket::BraketBackend;

#[cfg(feature = "scaleway")]
use arvak_adapter_scaleway::ScalewayBackend;

use super::common::{get_basis_gates, load_circuit, print_results};

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

    // Create backend FIRST so we can extract real topology for compilation
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
        "ibm" | "ibmq" | "ibm_torino" | "ibm_fez" | "ibm_marrakesh" | "ibm_brisbane"
        | "ibm_kyoto" | "ibm_osaka" => {
            println!("  Connecting to IBM Quantum...");
            match IbmBackend::connect(backend).await {
                Ok(mut b) => {
                    if do_compile {
                        b.set_skip_transpilation(true);
                    }
                    Box::new(b)
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to connect to IBM Quantum: {}. Set IBM_API_KEY + IBM_SERVICE_CRN (or IBM_QUANTUM_TOKEN).",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "ibm"))]
        "ibm" | "ibmq" | "ibm_torino" | "ibm_fez" | "ibm_marrakesh" | "ibm_brisbane"
        | "ibm_kyoto" | "ibm_osaka" => {
            anyhow::bail!("IBM backend not available. Rebuild with --features ibm");
        }
        #[cfg(feature = "braket")]
        "braket" | "braket-sv1" | "sv1" | "braket-tn1" | "tn1" | "braket-dm1" | "dm1"
        | "rigetti" | "ankaa" | "ionq" | "aria" => {
            println!("  Connecting to AWS Braket...");
            let device_arn = arvak_adapter_braket::device::arn_for_name(backend)
                .ok_or_else(|| anyhow::anyhow!("Unknown Braket device: {backend}"))?;
            match BraketBackend::connect(device_arn).await {
                Ok(b) => Box::new(b),
                Err(e) => {
                    anyhow::bail!(
                        "Failed to connect to AWS Braket: {}. Set ARVAK_BRAKET_S3_BUCKET and configure AWS credentials.",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "braket"))]
        "braket" | "braket-sv1" | "sv1" | "braket-tn1" | "tn1" | "braket-dm1" | "dm1"
        | "rigetti" | "ankaa" | "ionq" | "aria" => {
            anyhow::bail!("Braket backend not available. Rebuild with --features braket");
        }
        #[cfg(feature = "scaleway")]
        "scaleway" | "scaleway-garnet" | "scaleway-emerald" => {
            println!("  Connecting to Scaleway QaaS...");
            match ScalewayBackend::new() {
                Ok(b) => {
                    println!("  Session: {}, Platform: {}", b.session_id(), b.platform());
                    Box::new(b)
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to connect to Scaleway: {}. Set SCALEWAY_SECRET_KEY, SCALEWAY_PROJECT_ID, and SCALEWAY_SESSION_ID.",
                        e
                    );
                }
            }
        }
        #[cfg(not(feature = "scaleway"))]
        "scaleway" | "scaleway-garnet" | "scaleway-emerald" => {
            anyhow::bail!("Scaleway backend not available. Rebuild with --features scaleway");
        }
        other => {
            anyhow::bail!(
                "Unknown backend: '{other}'. Available: simulator, iqm, ibm, braket, scaleway"
            );
        }
    };

    // Compile if requested — use real topology from HAL capabilities
    if do_compile {
        let compile_target = target.unwrap_or(backend);
        println!("  Compiling for target: {}", style(compile_target).yellow());

        let caps = backend_impl.capabilities();
        let coupling_map =
            arvak_compile::CouplingMap::from_edge_list(caps.num_qubits, &caps.topology.edges);
        let basis_gates = get_basis_gates(compile_target)?;
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

    // Check availability
    let avail = backend_impl.availability().await?;
    if !avail.is_available {
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
