//! Submit command implementation.
//!
//! Submits a quantum circuit to an HPC batch scheduler (SLURM or PBS).

use std::sync::Arc;

use anyhow::Result;
use console::style;

use arvak_adapter_sim::SimulatorBackend;
use arvak_hal::Backend;
use arvak_sched::{
    CircuitSpec, HpcScheduler, PbsConfig, Priority, ScheduledJob, Scheduler, SchedulerConfig,
    SlurmConfig, SqliteStore,
};

use super::common::{default_state_dir, load_circuit, print_results};

/// Execute the submit command.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    input: &str,
    backend: &str,
    shots: u32,
    scheduler: &str,
    partition: Option<&str>,
    account: Option<&str>,
    time: Option<&str>,
    priority: Option<&str>,
    wait: bool,
) -> Result<()> {
    println!(
        "{} Submitting {} to {} via {}",
        style("→").cyan().bold(),
        style(input).green(),
        style(backend).yellow(),
        style(scheduler).magenta()
    );

    // Load and convert circuit
    let circuit = load_circuit(input)?;
    println!(
        "  Loaded: {} qubits, depth {}",
        circuit.num_qubits(),
        circuit.depth()
    );

    let circuit_spec = CircuitSpec::from_circuit(&circuit)
        .map_err(|e| anyhow::anyhow!("Failed to create circuit spec: {e}"))?;

    // Build scheduler config
    let state_dir = default_state_dir()?;
    let db_path = state_dir.join("jobs.db");
    let store =
        SqliteStore::new(&db_path).map_err(|e| anyhow::anyhow!("Failed to open job store: {e}"))?;

    let sched_config = match scheduler.to_lowercase().as_str() {
        "slurm" => {
            let mut slurm = SlurmConfig::default();
            if let Some(p) = partition {
                slurm.partition = p.to_string();
            }
            if let Some(a) = account {
                slurm.account = Some(a.to_string());
            }
            if let Some(t) = time {
                // Parse time limit.  Accepted formats:
                //   HH:MM:SS  →  convert to whole minutes (ceiling)
                //   HH:MM     →  hours and minutes
                //   MM        →  plain minutes
                let parts: Vec<&str> = t.split(':').collect();
                let minutes: u32 = match parts.len() {
                    3 => {
                        let h: u32 = parts[0].parse().map_err(|_| {
                            anyhow::anyhow!("Invalid time format '{t}': expected HH:MM:SS")
                        })?;
                        let m: u32 = parts[1].parse().map_err(|_| {
                            anyhow::anyhow!("Invalid time format '{t}': expected HH:MM:SS")
                        })?;
                        let s: u32 = parts[2].parse().map_err(|_| {
                            anyhow::anyhow!("Invalid time format '{t}': expected HH:MM:SS")
                        })?;
                        // Ceiling-divide seconds into an extra minute so the job
                        // doesn't get killed before it completes.
                        h * 60 + m + u32::from(s > 0)
                    }
                    2 => {
                        let h: u32 = parts[0].parse().map_err(|_| {
                            anyhow::anyhow!("Invalid time format '{t}': expected HH:MM")
                        })?;
                        let m: u32 = parts[1].parse().map_err(|_| {
                            anyhow::anyhow!("Invalid time format '{t}': expected HH:MM")
                        })?;
                        h * 60 + m
                    }
                    1 => parts[0].parse().map_err(|_| {
                        anyhow::anyhow!("Invalid time limit '{t}': expected whole minutes")
                    })?,
                    _ => {
                        anyhow::bail!("Invalid time format '{t}': expected HH:MM:SS, HH:MM, or MM")
                    }
                };
                slurm.time_limit = minutes;
            }
            SchedulerConfig::with_slurm(slurm)
        }
        "pbs" => {
            let mut pbs = PbsConfig::default();
            if let Some(p) = partition {
                pbs.queue = p.to_string();
            }
            if let Some(a) = account {
                pbs.account = Some(a.to_string());
            }
            if let Some(t) = time {
                pbs.walltime = t.to_string();
            }
            SchedulerConfig::with_pbs(pbs)
        }
        other => {
            anyhow::bail!("Unknown scheduler: '{other}'. Available: slurm, pbs");
        }
    };

    // Create backend
    let backend_impl: Arc<dyn Backend> = match backend.to_lowercase().as_str() {
        "simulator" | "sim" => Arc::new(SimulatorBackend::new()),
        #[cfg(feature = "iqm")]
        "iqm" | "garnet" => {
            use arvak_adapter_iqm::IqmBackend;
            Arc::new(
                IqmBackend::new().map_err(|e| {
                    anyhow::anyhow!("Failed to connect to IQM: {}. Set IQM_TOKEN.", e)
                })?,
            )
        }
        #[cfg(not(feature = "iqm"))]
        "iqm" | "garnet" => {
            anyhow::bail!("IQM backend not available. Rebuild with --features iqm");
        }
        #[cfg(feature = "ibm")]
        "ibm" | "ibmq" | "ibm_torino" | "ibm_fez" | "ibm_marrakesh" => {
            use arvak_adapter_ibm::IbmBackend;
            Arc::new(IbmBackend::connect(backend).await.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to connect to IBM: {}. Set IBM_API_KEY + IBM_SERVICE_CRN (or IBM_QUANTUM_TOKEN).",
                    e
                )
            })?)
        }
        #[cfg(not(feature = "ibm"))]
        "ibm" | "ibmq" | "ibm_torino" | "ibm_fez" | "ibm_marrakesh" => {
            anyhow::bail!("IBM backend not available. Rebuild with --features ibm");
        }
        #[cfg(feature = "braket")]
        "braket" | "braket-sv1" | "sv1" | "braket-tn1" | "tn1" | "braket-dm1" | "dm1"
        | "rigetti" | "ankaa" | "ionq" | "aria" => {
            use arvak_adapter_braket::BraketBackend;
            let device_arn = arvak_adapter_braket::device::arn_for_name(backend)
                .ok_or_else(|| anyhow::anyhow!("Unknown Braket device: {backend}"))?;
            Arc::new(BraketBackend::connect(device_arn).await.map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to connect to AWS Braket: {}. Set ARVAK_BRAKET_S3_BUCKET and configure AWS credentials.",
                        e
                    )
                })?)
        }
        #[cfg(not(feature = "braket"))]
        "braket" | "braket-sv1" | "sv1" | "braket-tn1" | "tn1" | "braket-dm1" | "dm1"
        | "rigetti" | "ankaa" | "ionq" | "aria" => {
            anyhow::bail!("Braket backend not available. Rebuild with --features braket");
        }
        other => {
            anyhow::bail!("Unknown backend: '{other}'. Available: simulator, iqm, ibm, braket");
        }
    };

    // Create HPC scheduler
    let hpc = HpcScheduler::new(sched_config, vec![backend_impl], Arc::new(store))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create scheduler: {e}"))?;

    // Build job
    let job_priority = match priority.map(str::to_lowercase).as_deref() {
        Some("low") => Priority::low(),
        Some("high") => Priority::high(),
        Some("critical") => Priority::critical(),
        _ => Priority::default(),
    };

    let name = std::path::Path::new(input).file_stem().map_or_else(
        || "circuit".to_string(),
        |s| s.to_string_lossy().to_string(),
    );

    let job = ScheduledJob::new(&name, circuit_spec)
        .with_shots(shots)
        .with_priority(job_priority);

    // Submit
    let job_id = hpc
        .submit(job)
        .await
        .map_err(|e| anyhow::anyhow!("Submit failed: {e}"))?;

    println!(
        "{} Job submitted: {}",
        style("✓").green().bold(),
        style(&job_id).cyan()
    );

    // Optionally wait for completion
    if wait {
        println!("  Waiting for job to complete...");
        let result = hpc
            .wait(&job_id)
            .await
            .map_err(|e| anyhow::anyhow!("Wait failed: {e}"))?;
        print_results(&result);
    } else {
        println!(
            "  Track with: {} {}",
            style("arvak status").dim(),
            style(&job_id).dim()
        );
    }

    Ok(())
}
