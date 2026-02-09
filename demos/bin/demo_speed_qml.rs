//! QML Compilation Throughput Demo
//!
//! Demonstrates Arvak's compilation speed in a realistic QML training loop:
//! parameter-shift gradient over 1000 training steps.

use std::time::Instant;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_ir::Circuit;

use arvak_demos::circuits::qml::{num_qml_parameters, qml_classifier};
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};

fn main() {
    print_header("QML Training Loop Compilation");

    // Problem setup
    let n_qubits = 4;
    let depth = 3;
    let n_params = num_qml_parameters(n_qubits, depth);
    let training_steps = 1000;
    // Parameter-shift: 2 circuits per parameter per step (plus 1 forward pass)
    let circuits_per_step = 2 * n_params + 1;
    let total_circuits = training_steps * circuits_per_step;

    print_section("Problem Setup");
    print_result("Task", "4-class quantum classifier");
    print_result("Qubits", n_qubits);
    print_result("Circuit depth", format!("{} layers", depth));
    print_result("Trainable parameters", n_params);
    print_result("Training steps", format_count(training_steps));
    print_result(
        "Circuits per step",
        format!("{} (parameter-shift gradient)", circuits_per_step),
    );
    print_result("Total circuits", format_count(total_circuits));
    print_result("Target", "IQM (CZ + PRX)");

    // Deterministic RNG
    let mut seed: u64 = 123;
    let mut rand_f64 = || -> f64 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 33) as f64 / (1u64 << 31) as f64 * std::f64::consts::PI
    };

    // Pre-generate all circuits
    print_section("Circuit Generation");
    let gen_start = Instant::now();
    let mut circuits = Vec::with_capacity(total_circuits);
    for _ in 0..total_circuits {
        let data: Vec<f64> = (0..n_params).map(|_| rand_f64()).collect();
        let weights: Vec<f64> = (0..n_params).map(|_| rand_f64()).collect();
        circuits.push(qml_classifier(n_qubits, depth, &data, &weights));
    }
    let gen_time = gen_start.elapsed();
    print_result("Generation time", format!("{:.2?}", gen_time));

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
        "  {:<8}{:<12}{:<16}{}",
        "Level", "Total", "Per-Circuit", "Gates/s"
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

    // Comparison
    print_section("Comparison");
    let slow_baseline_ms = 100.0;
    let slow_total_s = total_circuits as f64 * slow_baseline_ms / 1000.0;
    let speedup_o0 = slow_total_s / time_o0.as_secs_f64();
    let speedup_o2 = slow_total_s / time_o2.as_secs_f64();

    print_result(
        "At 100ms/circuit",
        format!("{:.1} minutes", slow_total_s / 60.0),
    );
    print_result(
        "Arvak speedup",
        format!("{:.0}x (O0) / {:.0}x (O2)", speedup_o0, speedup_o2),
    );

    print_section("Why This Matters");
    println!("  QML training requires computing gradients via parameter-shift:");
    println!("  each parameter needs 2 circuit evaluations per training step.");
    println!(
        "  With {} parameters and {} steps, that's {} circuits.",
        n_params,
        format_count(training_steps),
        format_count(total_circuits)
    );
    println!(
        "  A slow transpiler at 100ms/circuit spends {:.1} minutes just",
        slow_total_s / 60.0
    );
    println!("  compiling — before any quantum hardware is touched.");
    println!(
        "  Arvak compiles the entire training run in {:.2?}.",
        time_o2
    );

    println!();
    print_success("QML compilation throughput demo complete!");
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
        format!("{}", n)
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
        format!("{:.0}us", us)
    }
}

fn format_rate(per_sec: f64) -> String {
    if per_sec >= 1_000_000.0 {
        format!("{:.1}M", per_sec / 1_000_000.0)
    } else if per_sec >= 1_000.0 {
        format!("{:.0}K", per_sec / 1_000.0)
    } else {
        format!("{:.0}", per_sec)
    }
}
