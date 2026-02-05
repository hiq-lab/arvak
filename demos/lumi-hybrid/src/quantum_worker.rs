//! Quantum Worker for LUMI-Q
//!
//! This binary runs on LUMI-Q partition and executes quantum circuits.
//! It reads job specifications from a file and writes results back.
//!
//! Usage in SLURM:
//!   srun --partition=q_fiqci quantum_worker --job job_spec.json --output result.json

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use hiq_hal::Backend;
use hiq_ir::Circuit;
use hiq_qasm3::parse;

/// Quantum circuit job specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumJob {
    /// Job identifier
    pub job_id: String,

    /// Circuit in QASM3 format
    pub circuit_qasm: String,

    /// Number of shots
    pub shots: u32,

    /// Target backend (iqm, sim)
    pub backend: String,

    /// Optional parameters (e.g., for parameterized circuits)
    #[serde(default)]
    pub parameters: Vec<f64>,
}

/// Quantum job result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumResult {
    /// Job identifier
    pub job_id: String,

    /// Measurement counts (bitstring -> count)
    pub counts: std::collections::HashMap<String, u64>,

    /// Number of shots executed
    pub shots: u32,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,

    /// Backend used
    pub backend: String,

    /// Status (success, error)
    pub status: String,

    /// Error message (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Quantum Worker CLI
#[derive(Parser, Debug)]
#[command(name = "quantum_worker")]
#[command(about = "Execute quantum circuits on LUMI-Q")]
struct Args {
    /// Input job specification file
    #[arg(short, long)]
    job: PathBuf,

    /// Output result file
    #[arg(short, long)]
    output: PathBuf,

    /// Backend override (iqm, sim)
    #[arg(long)]
    backend: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    let filter = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("╔══════════════════════════════════════════╗");
    info!("║     LUMI-Q Quantum Worker                ║");
    info!("╚══════════════════════════════════════════╝");

    // Read job specification
    let job_json = fs::read_to_string(&args.job)?;
    let job: QuantumJob = serde_json::from_str(&job_json)?;

    info!("Job ID: {}", job.job_id);
    info!("Shots: {}", job.shots);
    info!("Backend: {}", args.backend.as_ref().unwrap_or(&job.backend));

    // Execute job
    let result = execute_job(&job, args.backend.as_deref()).await;

    // Write result
    let result_json = serde_json::to_string_pretty(&result)?;
    fs::write(&args.output, &result_json)?;

    info!("Result written to: {}", args.output.display());

    if result.status == "success" {
        info!("Execution completed successfully");
    } else {
        info!("Execution failed: {}", result.error.unwrap_or_default());
    }

    Ok(())
}

/// Execute a quantum job
async fn execute_job(job: &QuantumJob, backend_override: Option<&str>) -> QuantumResult {
    let start = std::time::Instant::now();
    let backend_name = backend_override.unwrap_or(&job.backend);

    // Parse circuit
    let circuit = match parse(&job.circuit_qasm) {
        Ok(c) => c,
        Err(e) => {
            return QuantumResult {
                job_id: job.job_id.clone(),
                counts: std::collections::HashMap::new(),
                shots: 0,
                execution_time_ms: start.elapsed().as_millis() as u64,
                backend: backend_name.to_string(),
                status: "error".to_string(),
                error: Some(format!("Failed to parse circuit: {}", e)),
            };
        }
    };

    // Execute based on backend
    let result = match backend_name {
        "sim" => execute_on_simulator(&circuit, job.shots).await,
        "iqm" | "lumi" => execute_on_iqm(&circuit, job.shots).await,
        _ => Err(anyhow::anyhow!("Unknown backend: {}", backend_name)),
    };

    match result {
        Ok(counts) => QuantumResult {
            job_id: job.job_id.clone(),
            counts,
            shots: job.shots,
            execution_time_ms: start.elapsed().as_millis() as u64,
            backend: backend_name.to_string(),
            status: "success".to_string(),
            error: None,
        },
        Err(e) => QuantumResult {
            job_id: job.job_id.clone(),
            counts: std::collections::HashMap::new(),
            shots: 0,
            execution_time_ms: start.elapsed().as_millis() as u64,
            backend: backend_name.to_string(),
            status: "error".to_string(),
            error: Some(e.to_string()),
        },
    }
}

/// Execute circuit on local simulator
async fn execute_on_simulator(
    circuit: &Circuit,
    shots: u32,
) -> Result<std::collections::HashMap<String, u64>> {
    let backend = hiq_adapter_sim::SimulatorBackend::new();
    let job_id = backend.submit(circuit, shots).await?;
    let result = backend.wait(&job_id).await?;

    Ok(result
        .counts
        .iter()
        .map(|(k, v): (&String, &u64)| (k.clone(), *v))
        .collect())
}

/// Execute circuit on IQM (LUMI-Q)
async fn execute_on_iqm(
    circuit: &Circuit,
    shots: u32,
) -> Result<std::collections::HashMap<String, u64>> {
    // Check for IQM token - IqmBackend::new() reads from IQM_TOKEN env var
    let token_available = std::env::var("IQM_TOKEN")
        .or_else(|_| std::env::var("HELMI_TOKEN"))
        .is_ok();

    if token_available {
        // Real IQM execution
        match hiq_adapter_iqm::IqmBackend::new() {
            Ok(backend) => {
                let job_id = backend.submit(circuit, shots).await?;
                let result = backend.wait(&job_id).await?;

                Ok(result
                    .counts
                    .iter()
                    .map(|(k, v): (&String, &u64)| (k.clone(), *v))
                    .collect())
            }
            Err(e) => {
                info!("IQM backend initialization failed: {}, using simulator", e);
                execute_on_simulator(circuit, shots).await
            }
        }
    } else {
        // Fall back to simulator with warning
        info!("IQM_TOKEN not set, using simulator");
        execute_on_simulator(circuit, shots).await
    }
}
