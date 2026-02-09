//! VQE Compilation Throughput Demo
//!
//! Demonstrates Arvak's compilation speed in a realistic VQE loop:
//! 500 optimizer iterations x 10 Hamiltonian terms = 5,000 circuits.

use std::time::Instant;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_ir::Circuit;

use arvak_demos::circuits::vqe::two_local_ansatz;
use arvak_demos::problems::lih_hamiltonian;
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};

fn main() {
    print_header("VQE Compilation Throughput");

    // Problem setup: LiH molecule
    let hamiltonian = lih_hamiltonian();
    let n_qubits = hamiltonian.num_qubits();
    let n_terms = hamiltonian.num_terms();
    let iterations = 500;
    let total_circuits = iterations * n_terms;
    let reps = 2;
    let n_params = n_qubits * (reps + 1);

    print_section("Problem Setup");
    print_result("Molecule", "LiH (Lithium Hydride)");
    print_result("Qubits", n_qubits);
    print_result("Hamiltonian terms", n_terms);
    print_result("Ansatz", format!("TwoLocal (reps={reps})"));
    print_result("Optimizer iterations", iterations);
    print_result("Total circuits", format_count(total_circuits));
    print_result("Target", "IQM (CZ + PRX)");

    // Deterministic RNG for parameter generation
    let mut seed: u64 = 42;
    let mut rand_f64 = || -> f64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 33) as f64 / (1u64 << 31) as f64 * std::f64::consts::PI
    };

    // Pre-generate all circuits
    print_section("Circuit Generation");
    let gen_start = Instant::now();
    let mut circuits = Vec::with_capacity(total_circuits);
    for _ in 0..total_circuits {
        let params: Vec<f64> = (0..n_params).map(|_| rand_f64()).collect();
        circuits.push(two_local_ansatz(n_qubits, reps, &params));
    }
    let gen_time = gen_start.elapsed();
    print_result("Generation time", format!("{gen_time:.2?}"));

    // Sample gate count from first circuit
    let sample_gates = circuits[0].dag().num_ops();
    print_result("Gates per circuit", sample_gates);

    // Compile at O0
    print_section("Compilation Results");
    let (time_o0, gates_o0) = compile_batch(&circuits, n_qubits, 0);
    let avg_o0 = time_o0.as_nanos() as f64 / total_circuits as f64;
    let gates_per_sec_o0 = gates_o0 as f64 / time_o0.as_secs_f64();

    // Compile at O2
    let (time_o2, gates_o2) = compile_batch(&circuits, n_qubits, 2);
    let avg_o2 = time_o2.as_nanos() as f64 / total_circuits as f64;
    let gates_per_sec_o2 = gates_o2 as f64 / time_o2.as_secs_f64();

    println!();
    println!(
        "  {:<8}{:<12}{:<16}Gates/s",
        "Level", "Total", "Per-Circuit"
    );
    println!(
        "  {:<8}{:<12}{:<16}{}",
        "O0",
        format_duration_secs(time_o0),
        format_duration_us(avg_o0),
        format_rate(gates_per_sec_o0)
    );
    println!(
        "  {:<8}{:<12}{:<16}{}",
        "O2",
        format_duration_secs(time_o2),
        format_duration_us(avg_o2),
        format_rate(gates_per_sec_o2)
    );

    // Comparison with hypothetical slow transpiler
    print_section("Comparison");
    let slow_baseline_ms = 100.0; // 100ms per circuit
    let slow_total_s = total_circuits as f64 * slow_baseline_ms / 1000.0;
    let speedup_o0 = slow_total_s / time_o0.as_secs_f64();
    let speedup_o2 = slow_total_s / time_o2.as_secs_f64();

    print_result(
        "At 100ms/circuit",
        format!("{:.1} minutes", slow_total_s / 60.0),
    );
    print_result(
        "Arvak speedup",
        format!("{speedup_o0:.0}x (O0) / {speedup_o2:.0}x (O2)"),
    );

    print_section("Why This Matters");
    println!("  VQE requires compiling a new circuit for every parameter update");
    println!("  and every Hamiltonian term. A 500-iteration LiH optimization");
    println!(
        "  generates {} circuits. At 100ms/circuit, compilation alone",
        format_count(total_circuits)
    );
    println!(
        "  takes {:.1} minutes — longer than the quantum execution.",
        slow_total_s / 60.0
    );
    println!("  Arvak compiles the entire batch in {time_o2:.2?}.");

    println!();
    print_success("VQE compilation throughput demo complete!");
    println!();
    print_info("These are real compilation passes (layout, routing, basis translation).");
    println!("  No quantum execution — pure compiler performance measurement.");
}

fn compile_batch(
    circuits: &[Circuit],
    n_qubits: usize,
    opt_level: u8,
) -> (std::time::Duration, usize) {
    let coupling = CouplingMap::star(n_qubits as u32);
    let basis = BasisGates::iqm();

    let mut total_gates = 0usize;

    let start = Instant::now();
    for circuit in circuits {
        let (pm, mut props) = PassManagerBuilder::new()
            .with_optimization_level(opt_level)
            .with_target(coupling.clone(), basis.clone())
            .build();

        let mut dag = circuit.clone().into_dag();
        pm.run(&mut dag, &mut props).unwrap();
        total_gates += dag.num_ops();
    }
    let elapsed = start.elapsed();

    (elapsed, total_gates)
}

fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{},{:03}", n / 1_000, n % 1_000)
    } else {
        format!("{n}")
    }
}

fn format_duration_secs(d: std::time::Duration) -> String {
    format!("{:.2}s", d.as_secs_f64())
}

fn format_duration_us(nanos: f64) -> String {
    let us = nanos / 1000.0;
    if us >= 1000.0 {
        format!("{:.1}ms", us / 1000.0)
    } else {
        format!("{us:.0}us")
    }
}

fn format_rate(per_sec: f64) -> String {
    if per_sec >= 1_000_000.0 {
        format!("{:.1}M", per_sec / 1_000_000.0)
    } else if per_sec >= 1_000.0 {
        format!("{:.0}K", per_sec / 1_000.0)
    } else {
        format!("{per_sec:.0}")
    }
}
