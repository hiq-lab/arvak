//! Backends command implementation.

use anyhow::Result;
use console::style;

use hiq_adapter_sim::SimulatorBackend;
use hiq_hal::Backend;

#[cfg(feature = "iqm")]
use hiq_adapter_iqm::IqmBackend;

#[cfg(feature = "ibm")]
use hiq_adapter_ibm::IbmBackend;

/// Execute the backends command.
pub async fn execute() -> Result<()> {
    println!("{} Available backends:\n", style("HIQ").cyan().bold());

    // Simulator
    let sim = SimulatorBackend::new();
    let caps = sim.capabilities().await?;
    let available = sim.is_available().await?;

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
                let available = iqm.is_available().await.unwrap_or(false);
                match iqm.capabilities().await {
                    Ok(caps) => {
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
                    Err(e) => {
                        println!(
                            "  {} {} (error: {})",
                            style("○").red(),
                            style("iqm").bold(),
                            e
                        );
                    }
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
        match IbmBackend::new() {
            Ok(ibm) => {
                let available = ibm.is_available().await.unwrap_or(false);
                match ibm.capabilities().await {
                    Ok(caps) => {
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
                    Err(e) => {
                        println!(
                            "  {} {} (error: {})",
                            style("○").red(),
                            style("ibm").bold(),
                            e
                        );
                    }
                }
            }
            Err(_) => {
                println!(
                    "  {} {} (not configured)",
                    style("○").dim(),
                    style("ibm").dim()
                );
                println!("    Set IBM_QUANTUM_TOKEN environment variable to enable");
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

    Ok(())
}
