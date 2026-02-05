//! VQE (Variational Quantum Eigensolver) Demo
//!
//! Demonstrates hybrid classical-quantum optimization for molecular simulation.

use clap::Parser;

use hiq_demos::problems::{
    beh2_hamiltonian, exact_ground_state_energy, h2_hamiltonian, h2o_hamiltonian, lih_hamiltonian,
};
use hiq_demos::runners::VqeRunner;
use hiq_demos::{
    create_progress_bar, print_header, print_info, print_result, print_section, print_success,
};

#[derive(Parser, Debug)]
#[command(name = "demo-vqe")]
#[command(about = "Demonstrate VQE for molecular ground state estimation")]
struct Args {
    /// Molecule to simulate (h2, lih, beh2, h2o)
    #[arg(short, long, default_value = "h2")]
    molecule: String,

    /// Number of ansatz repetitions
    #[arg(short, long, default_value = "2")]
    reps: usize,

    /// Maximum optimization iterations
    #[arg(short, long, default_value = "50")]
    iterations: usize,

    /// Number of shots per energy evaluation
    #[arg(short, long, default_value = "1024")]
    shots: u32,
}

fn main() {
    let args = Args::parse();

    print_header("VQE Molecular Simulation Demo");

    // Select Hamiltonian
    let (hamiltonian, molecule_name, exact_energy): (
        hiq_demos::problems::PauliHamiltonian,
        &str,
        Option<f64>,
    ) = match args.molecule.to_lowercase().as_str() {
        "h2" => (
            h2_hamiltonian(),
            "H₂ (Hydrogen)",
            exact_ground_state_energy("h2"),
        ),
        "lih" => (
            lih_hamiltonian(),
            "LiH (Lithium Hydride)",
            exact_ground_state_energy("lih"),
        ),
        "beh2" => (
            beh2_hamiltonian(),
            "BeH₂ (Beryllium Hydride)",
            exact_ground_state_energy("beh2"),
        ),
        "h2o" => (
            h2o_hamiltonian(),
            "H₂O (Water)",
            exact_ground_state_energy("h2o"),
        ),
        _ => {
            eprintln!(
                "Unknown molecule: {}. Available: h2, lih, beh2, h2o",
                args.molecule
            );
            std::process::exit(1);
        }
    };

    print_section("Problem Setup");
    print_result("Molecule", molecule_name);
    print_result("Qubits", hamiltonian.num_qubits() as usize);
    print_result("Hamiltonian terms", hamiltonian.num_terms());
    print_result("Ansatz repetitions", args.reps);
    print_result("Max iterations", args.iterations);

    if let Some(exact) = exact_energy {
        print_result("Exact ground state", format!("{:.4} Hartree", exact));
    }

    print_section("Hamiltonian");
    println!("{}", hamiltonian);

    print_section("Running VQE Optimization");
    println!();
    println!("  VQE is a hybrid algorithm that:");
    println!("  1. Classical optimizer proposes circuit parameters");
    println!("  2. Quantum circuit evaluates the energy");
    println!("  3. Repeat until convergence");
    println!();

    // Create and run VQE
    let runner = VqeRunner::new(hamiltonian)
        .with_reps(args.reps)
        .with_shots(args.shots)
        .with_maxiter(args.iterations);

    let num_params = runner.num_parameters();
    print_result("Parameters", num_params);

    let pb = create_progress_bar(args.iterations as u64, "Optimizing...");

    // Run VQE (simplified, progress is internal)
    let result = runner.run();

    pb.finish_with_message("Optimization complete");

    print_section("Results");
    print_result(
        "Optimal energy",
        format!("{:.6} Hartree", result.optimal_energy),
    );
    print_result("Iterations", result.iterations);
    print_result("Circuit evaluations", result.circuit_evaluations);
    print_result("Converged", if result.converged { "Yes" } else { "No" });

    if let Some(exact) = exact_energy {
        let error: f64 = (result.optimal_energy - exact).abs();
        let relative_error: f64 = (error / exact.abs()) * 100.0;
        print_result("Absolute error", format!("{:.6} Hartree", error));
        print_result("Relative error", format!("{:.2}%", relative_error));
    }

    print_section("Energy Convergence");
    let history = &result.energy_history;
    let show_points = 10.min(history.len());
    let step = history.len() / show_points;
    for (i, chunk) in history.chunks(step.max(1)).enumerate().take(show_points) {
        if let Some(&energy) = chunk.first() {
            println!("  Iteration {:3}: {:.6} Ha", i * step, energy);
        }
    }

    print_section("Demo Narrative");
    println!(
        "  This demo simulates finding the ground state energy of {}.",
        molecule_name
    );
    println!();
    println!("  In pharmaceutical applications, VQE is used to:");
    println!("  - Compute molecular properties for drug discovery");
    println!("  - Simulate reaction pathways");
    println!("  - Predict binding affinities");
    println!();
    println!("  HiQ orchestrates the hybrid workflow:");
    println!("  - Classical optimization runs on HPC nodes");
    println!("  - Quantum evaluations are queued on the QPU");
    println!("  - SLURM manages job scheduling across resources");

    println!();
    print_success("VQE demo complete!");
    println!();
    print_info("For real-world molecular simulation, HiQ would:");
    println!("  - Use optimized ansatz circuits (UCCSD, etc.)");
    println!("  - Execute on actual quantum hardware via IQM/IBM");
    println!("  - Run 100+ iterations for convergence");
    println!("  - Process batches of molecules in parallel");
}
