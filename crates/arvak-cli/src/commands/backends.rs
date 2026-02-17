//! Backends command implementation.

use anyhow::Result;
use console::style;

use arvak_adapter_sim::SimulatorBackend;
use arvak_hal::Backend;

#[cfg(feature = "iqm")]
use arvak_adapter_iqm::IqmBackend;

#[cfg(feature = "ibm")]
use arvak_adapter_ibm::IbmBackend;

/// Execute the backends command.
pub async fn execute() -> Result<()> {
    println!("{} Available backends:\n", style("Arvak").cyan().bold());

    // Simulator
    let sim = SimulatorBackend::new();
    let caps = sim.capabilities();
    let available = sim.availability().await?.is_available;

    println!(
        "  {} {} {}",
        if available {
            style("●").green()
        } else {
            style("○").red()
        },
        style("simulator").bold(),
        if caps.is_simulator { "(local)" } else { "" }
    );
    println!("    Qubits: {}", caps.num_qubits);
    println!("    Max shots: {}", caps.max_shots);
    println!(
        "    Gates: {}",
        caps.gate_set
            .native
            .join(", ")
            .chars()
            .take(50)
            .collect::<String>()
    );
    println!();

    // IQM backend
    #[cfg(feature = "iqm")]
    {
        match IqmBackend::new() {
            Ok(iqm) => {
                let available = iqm.availability().await.is_ok_and(|a| a.is_available);
                let caps = iqm.capabilities();
                println!(
                    "  {} {} ({})",
                    if available {
                        style("●").green()
                    } else {
                        style("○").yellow()
                    },
                    style("iqm").bold(),
                    iqm.target()
                );
                println!("    Qubits: {}", caps.num_qubits);
                println!("    Max shots: {}", caps.max_shots);
                println!("    Gates: {}", caps.gate_set.native.join(", "));
                if !available {
                    println!("    Status: offline or maintenance");
                }
            }
            Err(_) => {
                println!(
                    "  {} {} (not configured)",
                    style("○").dim(),
                    style("iqm").dim()
                );
                println!("    Set IQM_TOKEN environment variable to enable");
            }
        }
        println!();
    }

    #[cfg(not(feature = "iqm"))]
    {
        println!(
            "  {} {} (not compiled)",
            style("○").dim(),
            style("iqm").dim()
        );
        println!("    Rebuild with --features iqm to enable");
        println!();
    }

    // IBM backend
    #[cfg(feature = "ibm")]
    {
        match IbmBackend::connect("ibm_torino").await {
            Ok(ibm) => {
                let available = ibm.availability().await.is_ok_and(|a| a.is_available);
                let caps = ibm.capabilities();
                println!(
                    "  {} {} ({})",
                    if available {
                        style("●").green()
                    } else {
                        style("○").yellow()
                    },
                    style("ibm").bold(),
                    ibm.target()
                );
                println!("    Qubits: {}", caps.num_qubits);
                println!("    Max shots: {}", caps.max_shots);
                println!("    Gates: {}", caps.gate_set.native.join(", "));
                if !available {
                    println!("    Status: offline or maintenance");
                }
            }
            Err(_) => {
                println!(
                    "  {} {} (not configured)",
                    style("○").dim(),
                    style("ibm").dim()
                );
                println!("    Set IBM_API_KEY + IBM_SERVICE_CRN (or IBM_QUANTUM_TOKEN) to enable");
            }
        }
        println!();
    }

    #[cfg(not(feature = "ibm"))]
    {
        println!(
            "  {} {} (not compiled)",
            style("○").dim(),
            style("ibm").dim()
        );
        println!("    Rebuild with --features ibm to enable");
        println!();
    }

    // AWS Braket backends
    #[cfg(feature = "braket")]
    {
        use arvak_adapter_braket::BraketBackend;

        println!("  {} AWS Braket:", style("─").dim());

        let braket_devices = [
            ("braket-sv1", arvak_adapter_braket::device::SV1),
            ("rigetti", arvak_adapter_braket::device::RIGETTI_ANKAA_3),
            ("ionq", arvak_adapter_braket::device::IONQ_ARIA),
        ];

        for (name, arn) in &braket_devices {
            match BraketBackend::connect(*arn).await {
                Ok(braket) => {
                    let available = braket.availability().await.is_ok_and(|a| a.is_available);
                    let caps = braket.capabilities();
                    println!(
                        "  {} {} ({})",
                        if available {
                            style("●").green()
                        } else {
                            style("○").yellow()
                        },
                        style(name).bold(),
                        caps.name
                    );
                    println!("    Qubits: {}", caps.num_qubits);
                    println!("    Max shots: {}", caps.max_shots);
                    println!("    Gates: {}", caps.gate_set.native.join(", "));
                    if !available {
                        println!("    Status: offline or maintenance");
                    }
                }
                Err(_) => {
                    println!(
                        "  {} {} (not configured)",
                        style("○").dim(),
                        style(name).dim()
                    );
                    println!("    Set ARVAK_BRAKET_S3_BUCKET and configure AWS credentials");
                }
            }
        }
        println!();
    }

    #[cfg(not(feature = "braket"))]
    {
        println!(
            "  {} {} (not compiled)",
            style("○").dim(),
            style("braket").dim()
        );
        println!("    Rebuild with --features braket to enable");
        println!();
    }

    Ok(())
}
