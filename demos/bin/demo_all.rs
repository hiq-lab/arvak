//! Complete Demo Suite
//!
//! Runs all demos in sequence for a full presentation.

use clap::Parser;
use std::time::Instant;

use hiq_demos::circuits::grover::{grover_circuit, optimal_iterations};
use hiq_demos::problems::{Graph, h2_hamiltonian};
use hiq_demos::runners::orchestrator::{default_demo_jobs, run_multi_demo};
use hiq_demos::runners::{QaoaRunner, VqeRunner};
use hiq_demos::{print_header, print_info, print_result, print_section, print_success};

#[derive(Parser, Debug)]
#[command(name = "demo-all")]
#[command(about = "Run complete demo suite")]
struct Args {
    /// Run quick demo (reduced iterations)
    #[arg(long)]
    quick: bool,

    /// Skip multi-job orchestration demo
    #[arg(long)]
    skip_multi: bool,
}

fn main() {
    let args = Args::parse();
    let start = Instant::now();

    print_header("HIQ Complete Demo Suite");

    println!("  This demo showcases HiQ's HPC-quantum orchestration capabilities.");
    println!("  We'll demonstrate:");
    println!("  1. Grover's Search (baseline)");
    println!("  2. VQE for H₂ (hybrid classical-quantum)");
    println!("  3. QAOA for Max-Cut (optimization)");
    if !args.skip_multi {
        println!("  4. Multi-Job Orchestration (HiQ value prop)");
    }
    println!();

    let iterations = if args.quick { 20 } else { 50 };

    // =========================================================================
    // Part 1: Grover's Search
    // =========================================================================
    print_section("Part 1: Grover's Search Algorithm");
    println!();
    println!("  \"Let me show you the basics with Grover's algorithm...\"");
    println!();

    let grover_start = Instant::now();
    let n_qubits = 4;
    let marked = 7;
    let grover_iters = optimal_iterations(n_qubits);
    let circuit = grover_circuit(n_qubits, marked, grover_iters);

    print_result("Qubits", n_qubits);
    print_result("Marked state", format!("|{}⟩", marked));
    print_result("Iterations", grover_iters);
    print_result("Circuit depth", circuit.depth());
    print_result("Time", format!("{:.2?}", grover_start.elapsed()));

    let success_prob = (((2 * grover_iters + 1) as f64 * std::f64::consts::PI
        / (4.0 * ((1 << n_qubits) as f64).sqrt()))
    .sin())
    .powi(2);
    print_result(
        "Success probability",
        format!("{:.1}%", success_prob * 100.0),
    );

    print_success("Grover demo complete!");
    println!();

    // =========================================================================
    // Part 2: VQE for H2
    // =========================================================================
    print_section("Part 2: VQE for H₂ Molecule");
    println!();
    println!("  \"Now let's tackle a pharmaceutical problem - molecular ground state...\"");
    println!();

    let vqe_start = Instant::now();
    let h = h2_hamiltonian();

    print_result("Molecule", "H₂ (Hydrogen)");
    print_result("Qubits", h.num_qubits());
    print_result("Hamiltonian terms", h.num_terms());
    print_result("Exact energy", "-1.137 Hartree");

    let runner = VqeRunner::new(h).with_reps(1).with_maxiter(iterations);

    let result = runner.run();

    print_result(
        "Computed energy",
        format!("{:.4} Hartree", result.optimal_energy),
    );
    print_result(
        "Error",
        format!("{:.4} Hartree", (result.optimal_energy + 1.137).abs()),
    );
    print_result("Circuit evaluations", result.circuit_evaluations);
    print_result("Time", format!("{:.2?}", vqe_start.elapsed()));

    print_success("VQE demo complete!");
    println!();

    // =========================================================================
    // Part 3: QAOA for Max-Cut
    // =========================================================================
    print_section("Part 3: QAOA for Max-Cut");
    println!();
    println!("  \"This is a logistics optimization problem - graph partitioning...\"");
    println!();

    let qaoa_start = Instant::now();
    let graph = Graph::square_4();
    let (_, max_cut) = graph.max_cut_brute_force();

    print_result("Graph", "4-node square");
    print_result("Edges", graph.num_edges());
    print_result("Optimal cut", max_cut);

    let runner = QaoaRunner::new(graph.clone())
        .with_layers(2)
        .with_maxiter(iterations);

    let result = runner.run();
    let (set_s, set_t) = graph.bitstring_to_partition(result.best_bitstring);

    print_result("Found cut", result.best_cut);
    print_result("Partition", format!("{:?} | {:?}", set_s, set_t));
    print_result(
        "Approximation ratio",
        format!("{:.1}%", result.approximation_ratio * 100.0),
    );
    print_result("Circuit evaluations", result.circuit_evaluations);
    print_result("Time", format!("{:.2?}", qaoa_start.elapsed()));

    print_success("QAOA demo complete!");
    println!();

    // =========================================================================
    // Part 4: Multi-Job Orchestration
    // =========================================================================
    if !args.skip_multi {
        print_section("Part 4: Multi-Job Orchestration");
        println!();
        println!("  \"Here's where HPC integration matters - multiple simultaneous workflows...\"");
        println!();

        let multi_start = Instant::now();
        let jobs = default_demo_jobs();

        print_result("Jobs queued", jobs.len());

        let result = run_multi_demo(&jobs, true);

        println!();
        print_result("Jobs completed", result.successful);
        print_result("Jobs failed", result.failed);
        print_result("Time", format!("{:.2?}", multi_start.elapsed()));

        print_success("Orchestration demo complete!");
        println!();
    }

    // =========================================================================
    // Summary
    // =========================================================================
    print_section("Demo Summary");

    let total_time = start.elapsed();
    print_result("Total demo time", format!("{:.2?}", total_time));

    println!();
    println!("  Key takeaways:");
    println!("  - HiQ provides end-to-end quantum workflow management");
    println!("  - Hybrid algorithms (VQE, QAOA) require HPC-QPU orchestration");
    println!("  - SLURM integration enables production-scale quantum computing");
    println!("  - Multi-job management is essential for real applications");

    print_section("For Different Audiences");

    println!("  LUMI/LRZ (HPC Centers):");
    println!("    Lead with orchestration demo");
    println!("    \"Your users need this infrastructure layer\"");
    println!();
    println!("  IQM (Quantum Hardware):");
    println!("    Lead with VQE/QAOA demos");
    println!("    \"This makes your hardware accessible to HPC users\"");
    println!();
    println!("  Helsing (Defense/AI):");
    println!("    Lead with QAOA optimization");
    println!("    \"Quantum for mission-critical optimization\"");
    println!();
    println!("  Academic/Research:");
    println!("    Lead with VQE molecular simulation");
    println!("    \"Infrastructure for computational quantum research\"");

    println!();
    print_success("HIQ Demo Suite Complete!");
    println!();
    print_info("For more details, run individual demos:");
    println!("  demo-grover --help");
    println!("  demo-vqe --help");
    println!("  demo-qaoa --help");
    println!("  demo-multi --help");
}
