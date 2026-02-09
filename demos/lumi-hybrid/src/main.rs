//! LUMI-Q Hybrid VQE Demo: H2 Ground State Energy
//!
//! This demo showcases a quantum-classical hybrid workflow on LUMI:
//! - Classical optimizer runs on LUMI-G (AMD GPUs) or LUMI-C (CPUs)
//! - Quantum circuit evaluation runs on LUMI-Q (IQM quantum computer)
//! - Arvak orchestrates the SLURM jobs for both partitions
//!
//! The Variational Quantum Eigensolver (VQE) finds the ground state energy
//! of the H2 molecule at various bond distances.

use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

use arvak_hal::Backend;
use arvak_ir::Circuit;

mod ansatz;
mod hamiltonian;
mod optimizer;

use ansatz::create_uccsd_ansatz;
use hamiltonian::H2Hamiltonian;
use optimizer::{CobylaOptimizer, Optimizer};

/// LUMI-Q Hybrid VQE Demo
#[derive(Parser, Debug)]
#[command(name = "lumi_vqe")]
#[command(about = "VQE for H2 molecule on LUMI quantum-HPC hybrid system")]
struct Args {
    /// Bond distance in Angstroms (default: 0.735 for equilibrium)
    #[arg(short, long, default_value = "0.735")]
    bond_distance: f64,

    /// Number of VQE iterations
    #[arg(short, long, default_value = "50")]
    max_iterations: usize,

    /// Number of shots per circuit evaluation
    #[arg(short, long, default_value = "1000")]
    shots: u32,

    /// Backend: sim, iqm, or lumi
    #[arg(long, default_value = "sim")]
    backend: String,

    /// Output directory for results
    #[arg(short, long, default_value = "results")]
    output: PathBuf,

    /// Run bond distance scan (0.3 to 2.5 Å)
    #[arg(long)]
    scan: bool,

    /// Use SLURM workflow orchestration
    #[arg(long)]
    slurm: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

/// VQE iteration result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VqeResult {
    pub iteration: usize,
    pub parameters: Vec<f64>,
    pub energy: f64,
    pub shots: u32,
}

/// Complete VQE run result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VqeRunResult {
    pub bond_distance: f64,
    pub final_energy: f64,
    pub exact_energy: f64,
    pub error: f64,
    pub iterations: Vec<VqeResult>,
    pub optimal_parameters: Vec<f64>,
    pub backend: String,
    pub total_shots: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup logging
    let filter = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║       LUMI-Q Hybrid VQE Demo: H2 Ground State Energy        ║");
    info!("╠══════════════════════════════════════════════════════════════╣");
    info!("║  Classical compute: LUMI-G (AMD MI250X) / LUMI-C (AMD EPYC) ║");
    info!("║  Quantum compute:   LUMI-Q (IQM 20-qubit)                   ║");
    info!("║  Orchestration:     Arvak + SLURM                             ║");
    info!("╚══════════════════════════════════════════════════════════════╝");
    info!("");

    // Create output directory
    fs::create_dir_all(&args.output)?;

    if args.scan {
        // Bond distance scan
        run_bond_scan(&args).await?;
    } else {
        // Single point calculation
        let result = run_vqe(&args).await?;

        // Save result
        let result_path = args.output.join("vqe_result.json");
        fs::write(&result_path, serde_json::to_string_pretty(&result)?)?;
        info!("Results saved to: {}", result_path.display());
    }

    Ok(())
}

/// Run VQE for a single bond distance
async fn run_vqe(args: &Args) -> Result<VqeRunResult> {
    info!(
        "Running VQE for H2 at bond distance: {:.3} Å",
        args.bond_distance
    );
    info!("Backend: {}", args.backend);
    info!("Max iterations: {}", args.max_iterations);
    info!("Shots per evaluation: {}", args.shots);
    info!("");

    // Create Hamiltonian for H2 at given bond distance
    let hamiltonian = H2Hamiltonian::new(args.bond_distance);
    let exact_energy = hamiltonian.exact_ground_state_energy();

    info!("Exact ground state energy: {:.6} Ha", exact_energy);
    info!("");

    // Initialize parameters (single parameter for minimal UCCSD ansatz)
    let num_params = 1;
    let mut parameters = vec![0.0; num_params];

    // Setup optimizer (Nelder-Mead for derivative-free optimization)
    let mut optimizer = CobylaOptimizer::new(num_params)
        .with_bounds(vec![(-PI, PI); num_params])
        .with_tolerance(1e-6)
        .with_max_iterations(args.max_iterations);

    // VQE iterations
    let mut iterations = Vec::new();
    let mut total_shots = 0u64;
    let mut iter = 0;

    loop {
        // Evaluate energy at current parameters
        let energy = evaluate_energy(&args.backend, &hamiltonian, &parameters, args.shots).await?;
        total_shots += u64::from(args.shots);

        // Log progress
        let error = (energy - exact_energy).abs();
        if iter % 5 == 0 || optimizer.converged() {
            info!(
                "Iteration {:3}: E = {:.6} Ha, error = {:.6} Ha, θ = {:.4}",
                iter, energy, error, parameters[0]
            );
        }

        // Store iteration result
        iterations.push(VqeResult {
            iteration: iter,
            parameters: parameters.clone(),
            energy,
            shots: args.shots,
        });

        // Update parameters using optimizer
        parameters = optimizer.step(&parameters, energy);
        iter += 1;

        // Check convergence or max iterations
        if optimizer.converged() {
            info!("Converged after {} iterations!", iter);
            break;
        }

        if iter >= args.max_iterations {
            info!("Reached maximum iterations ({})", args.max_iterations);
            break;
        }
    }

    // Get best results from optimizer
    let best_params = optimizer
        .best_params()
        .map_or_else(|| parameters.clone(), <[f64]>::to_vec);
    let best_energy = optimizer.best_cost();

    info!("");
    info!("═══════════════════════════════════════════════════════════════");
    info!("VQE Optimization Complete");
    info!("═══════════════════════════════════════════════════════════════");
    info!("Final energy:    {:.6} Ha", best_energy);
    info!("Exact energy:    {:.6} Ha", exact_energy);
    info!(
        "Error:           {:.6} Ha ({:.2} mHa)",
        (best_energy - exact_energy).abs(),
        (best_energy - exact_energy).abs() * 1000.0
    );
    info!("Optimal θ:       {:.4} rad", best_params[0]);
    info!("Total shots:     {}", total_shots);
    info!("═══════════════════════════════════════════════════════════════");

    Ok(VqeRunResult {
        bond_distance: args.bond_distance,
        final_energy: best_energy,
        exact_energy,
        error: (best_energy - exact_energy).abs(),
        iterations,
        optimal_parameters: best_params,
        backend: args.backend.clone(),
        total_shots,
    })
}

/// Run bond distance scan
async fn run_bond_scan(args: &Args) -> Result<()> {
    info!("Running bond distance scan from 0.3 to 2.5 Å");
    info!("");

    let distances: Vec<f64> = (3..=25).map(|i| f64::from(i) * 0.1).collect();
    let mut results = Vec::new();

    for (idx, &distance) in distances.iter().enumerate() {
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!(
            "Scan point {}/{}: r = {:.2} Å",
            idx + 1,
            distances.len(),
            distance
        );

        let scan_args = Args {
            bond_distance: distance,
            max_iterations: 30, // Fewer iterations for scan
            shots: args.shots,
            backend: args.backend.clone(),
            output: args.output.clone(),
            scan: false,
            slurm: args.slurm,
            verbose: args.verbose,
        };

        match run_vqe(&scan_args).await {
            Ok(result) => results.push(result),
            Err(e) => {
                warn!("Failed at distance {:.2}: {}", distance, e);
            }
        }
    }

    // Save scan results
    let scan_path = args.output.join("bond_scan.json");
    fs::write(&scan_path, serde_json::to_string_pretty(&results)?)?;
    info!("");
    info!("Bond scan results saved to: {}", scan_path.display());

    // Print summary table
    info!("");
    info!("Bond Distance Scan Summary");
    info!("┌──────────┬───────────────┬───────────────┬───────────────┐");
    info!("│ r (Å)    │ VQE (Ha)      │ Exact (Ha)    │ Error (mHa)   │");
    info!("├──────────┼───────────────┼───────────────┼───────────────┤");
    for result in &results {
        info!(
            "│ {:7.3}  │ {:12.6}  │ {:12.6}  │ {:12.4}  │",
            result.bond_distance,
            result.final_energy,
            result.exact_energy,
            result.error * 1000.0
        );
    }
    info!("└──────────┴───────────────┴───────────────┴───────────────┘");

    Ok(())
}

/// Evaluate energy expectation value
async fn evaluate_energy(
    backend_name: &str,
    hamiltonian: &H2Hamiltonian,
    parameters: &[f64],
    _shots: u32,
) -> Result<f64> {
    let theta = parameters.first().copied().unwrap_or(0.0);

    // Get expectation value based on backend
    match backend_name {
        "sim" | "exact" => {
            // Use exact analytical energy for accurate VQE demo
            // This avoids shot noise and measurement approximations
            Ok(hamiltonian.exact_energy_for_parameter(theta))
        }
        "sim-shots" => {
            // Use simulator with shot-based measurement (noisy)
            let circuit = create_uccsd_ansatz(parameters)?;
            let backend = arvak_adapter_sim::SimulatorBackend::new();
            evaluate_with_backend(&backend, &circuit, hamiltonian, _shots).await
        }
        "iqm" | "lumi" => {
            // For real hardware, we'd use the IQM adapter
            // For now, fall back to exact simulation
            warn!("IQM/LUMI backend: using exact simulation (connect to real hardware with IQM_TOKEN)");
            Ok(hamiltonian.exact_energy_for_parameter(theta))
        }
        _ => {
            anyhow::bail!(
                "Unknown backend: {backend_name}. Use 'sim', 'sim-shots', 'iqm', or 'lumi'"
            );
        }
    }
}

/// Evaluate energy using a specific backend
async fn evaluate_with_backend<B: Backend>(
    backend: &B,
    circuit: &Circuit,
    hamiltonian: &H2Hamiltonian,
    shots: u32,
) -> Result<f64> {
    // The H2 Hamiltonian can be decomposed into Pauli terms
    // For each term, we measure in the appropriate basis

    // For simplicity in this demo, we compute expectation values
    // by measuring in computational basis and post-processing

    let job_id = backend.submit(circuit, shots).await?;
    let result = backend.wait(&job_id).await?;

    // Compute energy from measurement results
    let energy = hamiltonian.expectation_from_counts(&result.counts);

    Ok(energy)
}
