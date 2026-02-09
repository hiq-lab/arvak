//! QAOA Sensor Network Compilation Throughput Demo
//!
//! Demonstrates Arvak's compilation speed for tactical sensor network
//! optimization using QAOA. Three scenarios with depth sweeps and
//! grid-search angle optimization.

use std::time::Instant;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_ir::Circuit;

use arvak_demos::circuits::qaoa::qaoa_circuit;
use arvak_demos::problems::Graph;
use arvak_demos::problems::sensor_assignment::{
    drone_patrol_6, radar_deconfliction_8, surveillance_grid_10,
};
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};

fn main() {
    print_header("QAOA Sensor Network Compilation");

    println!("  Tactical C2 scenario: real-time sensor re-optimization");
    println!("  as the operational picture changes. Each re-plan requires");
    println!("  sweeping QAOA depth and angle space.");
    println!();

    let scenarios: Vec<(&str, Graph)> = vec![
        ("Drone patrol (6 zones)", drone_patrol_6()),
        ("Radar deconfliction (8 stations)", radar_deconfliction_8()),
        ("Surveillance grid (10 nodes)", surveillance_grid_10()),
    ];

    let max_depth = 5;
    let grid_size = 20; // 20x20 angle grid
    let circuits_per_problem = max_depth * grid_size * grid_size; // p=1..5 x 20x20 = 2,000

    print_section("Problem Setup");
    print_result("Scenarios", scenarios.len());
    print_result("QAOA depth sweep", format!("p = 1..{max_depth}"));
    print_result(
        "Angle grid",
        format!("{grid_size}x{grid_size} (gamma x beta)"),
    );
    print_result("Circuits per scenario", format_count(circuits_per_problem));
    print_result(
        "Total circuits",
        format_count(circuits_per_problem * scenarios.len()),
    );
    print_result("Target", "IQM (CZ + PRX)");

    let mut grand_total_circuits = 0usize;
    let mut grand_total_time_o0 = std::time::Duration::ZERO;
    let mut grand_total_time_o2 = std::time::Duration::ZERO;

    for (name, graph) in &scenarios {
        print_section(name);
        print_result("Nodes", graph.n_nodes);
        print_result("Edges", graph.num_edges());

        // Generate all circuits for this scenario
        let mut circuits = Vec::with_capacity(circuits_per_problem);
        for p in 1..=max_depth {
            for gi in 0..grid_size {
                for bi in 0..grid_size {
                    let gamma_val = std::f64::consts::PI * (gi as f64 + 0.5) / grid_size as f64;
                    let beta_val =
                        std::f64::consts::FRAC_PI_2 * (bi as f64 + 0.5) / grid_size as f64;
                    let gamma = vec![gamma_val; p];
                    let beta = vec![beta_val; p];
                    circuits.push(qaoa_circuit(graph, &gamma, &beta));
                }
            }
        }

        let sample_gates = circuits[0].dag().num_ops();
        print_result("Gates (p=1)", sample_gates);

        // Compile at O0
        let (time_o0, _) = compile_batch(&circuits, graph.n_nodes, 0);
        let avg_o0 = time_o0.as_nanos() as f64 / circuits.len() as f64;

        // Compile at O2
        let (time_o2, _) = compile_batch(&circuits, graph.n_nodes, 2);
        let avg_o2 = time_o2.as_nanos() as f64 / circuits.len() as f64;

        println!(
            "  O0: {:<10} ({} per circuit)",
            format_duration_secs(time_o0),
            format_duration_us(avg_o0)
        );
        println!(
            "  O2: {:<10} ({} per circuit)",
            format_duration_secs(time_o2),
            format_duration_us(avg_o2)
        );

        grand_total_circuits += circuits.len();
        grand_total_time_o0 += time_o0;
        grand_total_time_o2 += time_o2;
    }

    print_section("Aggregate Results");
    print_result(
        "Total circuits compiled",
        format_count(grand_total_circuits),
    );

    let avg_o0 = grand_total_time_o0.as_nanos() as f64 / grand_total_circuits as f64;
    let avg_o2 = grand_total_time_o2.as_nanos() as f64 / grand_total_circuits as f64;

    println!();
    println!("  {:<8}{:<12}{:<16}", "Level", "Total", "Per-Circuit");
    println!(
        "  {:<8}{:<12}{:<16}",
        "O0",
        format_duration_secs(grand_total_time_o0),
        format_duration_us(avg_o0)
    );
    println!(
        "  {:<8}{:<12}{:<16}",
        "O2",
        format_duration_secs(grand_total_time_o2),
        format_duration_us(avg_o2)
    );

    // Comparison
    print_section("Comparison");
    let slow_baseline_ms = 100.0;
    let slow_total_s = grand_total_circuits as f64 * slow_baseline_ms / 1000.0;
    let speedup_o0 = slow_total_s / grand_total_time_o0.as_secs_f64();
    let speedup_o2 = slow_total_s / grand_total_time_o2.as_secs_f64();

    print_result(
        "At 100ms/circuit",
        format!("{:.1} minutes", slow_total_s / 60.0),
    );
    print_result(
        "Arvak speedup",
        format!("{speedup_o0:.0}x (O0) / {speedup_o2:.0}x (O2)"),
    );

    print_section("Operational Impact");
    println!("  In tactical C2, the operational picture changes continuously.");
    println!("  When a new threat is detected or assets reposition, sensor");
    println!("  assignments must be re-optimized in seconds, not minutes.");
    println!();
    println!(
        "  A conventional transpiler at 100ms/circuit needs {:.1} minutes",
        slow_total_s / 60.0
    );
    println!("  to explore the QAOA parameter space across three scenarios.");
    println!(
        "  Arvak completes the same sweep in {grand_total_time_o2:.2?} — enabling"
    );
    println!("  real-time re-optimization as the situation evolves.");

    println!();
    print_success("QAOA sensor network compilation demo complete!");
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
