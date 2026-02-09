//! Grover's Search Algorithm Demo
//!
//! Demonstrates Arvak's ability to run quantum search algorithms.

use clap::Parser;

use arvak_demos::circuits::grover::{grover_circuit, optimal_iterations};
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};
use arvak_qasm3::emit;

#[derive(Parser, Debug)]
#[command(name = "demo-grover")]
#[command(about = "Demonstrate Grover's search algorithm")]
struct Args {
    /// Number of qubits (search space size = 2^n)
    #[arg(short = 'n', long, default_value = "4")]
    qubits: usize,

    /// Marked state to search for (0 to 2^n - 1)
    #[arg(short, long, default_value = "7")]
    marked: usize,

    /// Number of Grover iterations (0 = optimal)
    #[arg(short, long, default_value = "0")]
    iterations: usize,

    /// Show generated QASM code
    #[arg(long)]
    show_qasm: bool,
}

fn main() {
    let args = Args::parse();

    print_header("Grover's Search Algorithm Demo");

    // Validate inputs
    let max_state = (1 << args.qubits) - 1;
    if args.marked > max_state {
        eprintln!(
            "Error: marked state {} exceeds maximum {} for {} qubits",
            args.marked, max_state, args.qubits
        );
        std::process::exit(1);
    }

    let iterations = if args.iterations == 0 {
        optimal_iterations(args.qubits)
    } else {
        args.iterations
    };

    print_section("Problem Setup");
    print_result("Qubits", args.qubits);
    print_result("Search space size", 1 << args.qubits);
    print_result(
        "Marked state",
        format!(
            "|{}⟩ = |{:0width$b}⟩",
            args.marked,
            args.marked,
            width = args.qubits
        ),
    );
    print_result("Grover iterations", iterations);

    print_section("Circuit Generation");
    let circuit = grover_circuit(args.qubits, args.marked, iterations);
    print_result("Circuit depth", circuit.depth());
    print_result("Qubits", circuit.num_qubits());
    print_result("Classical bits", circuit.num_clbits());

    if args.show_qasm {
        print_section("Generated QASM3");
        match emit(&circuit) {
            Ok(qasm) => {
                println!("{qasm}");
            }
            Err(e) => {
                eprintln!("Error generating QASM: {e}");
            }
        }
    }

    print_section("Demo Narrative");
    println!("  This demo shows Grover's algorithm searching for a marked item");
    println!(
        "  in an unstructured database of {} items.",
        1 << args.qubits
    );
    println!();
    println!("  Classical complexity: O(N) = O({})", 1 << args.qubits);
    println!(
        "  Quantum complexity:   O(sqrt(N)) = O({:.1})",
        f64::from(1 << args.qubits).sqrt()
    );
    println!();
    println!("  The algorithm:");
    println!("  1. Prepares uniform superposition over all states");
    println!("  2. Applies {iterations} Grover iterations:");
    println!("     - Oracle: Flips the phase of the marked state");
    println!("     - Diffusion: Amplifies the amplitude of marked state");
    println!("  3. Measures to obtain the marked state with high probability");

    print_section("Expected Results");
    let success_prob = (((2 * iterations + 1) as f64 * std::f64::consts::PI
        / (4.0 * f64::from(1 << args.qubits).sqrt()))
    .sin())
    .powi(2);
    print_result(
        "Success probability",
        format!("{:.1}%", success_prob * 100.0),
    );
    print_result("Expected outcome", format!("|{}⟩", args.marked));

    println!();
    print_success("Grover demo complete!");
    println!();
    print_info("In a production Arvak deployment, this circuit would be:");
    println!("  - Compiled to native gates for the target backend");
    println!("  - Submitted via SLURM to the quantum hardware queue");
    println!("  - Results collected and analyzed automatically");
}
