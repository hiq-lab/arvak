//! QAOA (Quantum Approximate Optimization Algorithm) Demo
//!
//! Demonstrates quantum optimization for graph problems.

use clap::Parser;

use arvak_demos::problems::Graph;
use arvak_demos::runners::QaoaRunner;
use arvak_demos::{
    create_progress_bar, print_header, print_info, print_result, print_section, print_success,
};

#[derive(Parser, Debug)]
#[command(name = "demo-qaoa")]
#[command(about = "Demonstrate QAOA for Max-Cut optimization")]
struct Args {
    /// Graph to optimize (square4, complete4, ring6)
    #[arg(short, long, default_value = "square4")]
    graph: String,

    /// Number of QAOA layers
    #[arg(short = 'p', long, default_value = "2")]
    layers: usize,

    /// Maximum optimization iterations
    #[arg(short, long, default_value = "100")]
    iterations: usize,
}

fn main() {
    let args = Args::parse();

    print_header("QAOA Max-Cut Optimization Demo");

    // Select graph
    let graph = match args.graph.to_lowercase().as_str() {
        "square4" | "square" => Graph::square_4(),
        "complete4" | "k4" => Graph::complete_4(),
        "ring6" | "ring" => Graph::ring_6(),
        "grid6" | "grid" => Graph::grid_6(),
        _ => {
            eprintln!(
                "Unknown graph: {}. Available: square4, complete4, ring6, grid6",
                args.graph
            );
            std::process::exit(1);
        }
    };

    print_section("Problem Setup");
    println!("{graph}");

    // Compute exact solution
    let (exact_bitstring, exact_cut) = graph.max_cut_brute_force();
    let (exact_s, exact_t) = graph.bitstring_to_partition(exact_bitstring);

    print_result("Nodes", graph.n_nodes);
    print_result("Edges", graph.num_edges());
    print_result("QAOA layers (p)", args.layers);
    print_result("Max iterations", args.iterations);
    println!();
    print_result("Optimal cut (exact)", exact_cut);
    print_result("Optimal partition", format!("{exact_s:?} | {exact_t:?}"));

    print_section("Max-Cut Problem");
    println!("  The Max-Cut problem: Partition graph nodes into two sets");
    println!("  to maximize the number of edges between the sets.");
    println!();
    println!("  Applications:");
    println!("  - Logistics optimization");
    println!("  - Network design");
    println!("  - Circuit layout");
    println!("  - Mission planning (defense)");

    print_section("Running QAOA Optimization");
    println!();
    println!("  QAOA alternates between:");
    println!("  1. Cost unitary: exp(-iγC) encoding the graph");
    println!("  2. Mixer unitary: exp(-iβB) exploring solutions");
    println!();

    let runner = QaoaRunner::new(graph.clone())
        .with_layers(args.layers)
        .with_maxiter(args.iterations);

    let pb = create_progress_bar(args.iterations as u64, "Optimizing...");

    let result = runner.run();

    pb.finish_with_message("Optimization complete");

    print_section("Results");
    let (found_s, found_t) = graph.bitstring_to_partition(result.best_bitstring);

    print_result("Best cut found", result.best_cut);
    print_result("Best partition", format!("{found_s:?} | {found_t:?}"));
    print_result(
        "Approximation ratio",
        format!("{:.1}%", result.approximation_ratio * 100.0),
    );
    print_result("Iterations", result.iterations);
    print_result("Circuit evaluations", result.circuit_evaluations);

    println!();
    print_result("Optimal γ", format!("{:?}", result.optimal_gamma));
    print_result("Optimal β", format!("{:?}", result.optimal_beta));

    print_section("Performance Analysis");
    let is_optimal = (result.best_cut - exact_cut).abs() < 1e-6;
    if is_optimal {
        println!("  Found optimal solution!");
    } else {
        println!(
            "  Found {:.1}% of optimal solution.",
            result.approximation_ratio * 100.0
        );
        println!("  (Higher p or more iterations may improve results)");
    }

    print_section("Demo Narrative");
    println!("  This demo solves a Max-Cut problem using QAOA.");
    println!();
    println!("  QAOA is a variational algorithm suitable for:");
    println!("  - Combinatorial optimization");
    println!("  - Constraint satisfaction");
    println!("  - Scheduling problems");
    println!();
    println!("  For defense applications (Helsing angle):");
    println!("  - Resource allocation under constraints");
    println!("  - Mission routing optimization");
    println!("  - Network partitioning");
    println!();
    println!("  Arvak manages the optimization workflow:");
    println!(
        "  - {} quantum circuit evaluations",
        result.circuit_evaluations
    );
    println!("  - Each evaluation queued via SLURM");
    println!("  - Results aggregated for classical optimization");

    println!();
    print_success("QAOA demo complete!");
    println!();
    print_info("For larger problems, Arvak would:");
    println!("  - Scale to graphs with 10-20+ nodes");
    println!("  - Use deeper circuits (p = 3-5)");
    println!("  - Run on actual quantum hardware");
    println!("  - Process multiple graph instances in parallel");
}
