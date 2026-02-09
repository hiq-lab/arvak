//! QI-Nutshell Demo: Quantum Communication Protocol Emulation
//!
//! Demonstrates Arvak's ability to express and compile QKD protocols
//! from "Quantum Internet in a Nutshell" (Hilder et al., arXiv:2507.14383v2).
//!
//! This demo proves that Arvak's IR, compilation, and QASM3 emission can
//! cleanly handle the protocol-to-circuit mapping used in the paper.

use std::f64::consts::PI;
use std::time::Instant;

use clap::Parser;

use arvak_compile::{BasisGates, CouplingMap, PassManagerBuilder};
use arvak_demos::circuits::qi_nutshell::{
    Basis, EveStrategy, bb84_circuit, bb84_qec_circuit, bbm92_circuit, optimal_symmetric_angle,
    pccm_fidelities, pccm_qber,
};
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};
use arvak_ir::Circuit;
use arvak_qasm3::emit;

#[derive(Parser, Debug)]
#[command(name = "demo-qi-nutshell")]
#[command(about = "QI-Nutshell: Quantum communication protocol emulation on Arvak")]
struct Args {
    /// Protocol to demonstrate (bb84, bbm92, pccm-sweep, qec, all)
    #[arg(short, long, default_value = "all")]
    protocol: String,

    /// PCCM attack angle in units of π (0.0 to 0.5)
    #[arg(long, default_value = "0.25")]
    theta: f64,

    /// Show generated QASM3 code
    #[arg(long)]
    show_qasm: bool,

    /// Compile to ion-trap native gates (CZ + PRX basis)
    #[arg(long)]
    compile: bool,

    /// Optimization level for compilation (0-3)
    #[arg(short = 'O', long, default_value = "2")]
    opt_level: u8,
}

fn main() {
    let args = Args::parse();

    print_header("QI-Nutshell: Quantum Internet Protocol Emulation");
    println!("  Based on: Hilder et al., arXiv:2507.14383v2");
    println!("  \"Quantum Internet in a Nutshell — Advancing Quantum");
    println!("   Communication with Ion Traps\"");

    match args.protocol.as_str() {
        "bb84" => demo_bb84(&args),
        "bbm92" => demo_bbm92(&args),
        "pccm-sweep" => demo_pccm_sweep(&args),
        "qec" => demo_qec(&args),
        "all" => {
            demo_bb84(&args);
            demo_bbm92(&args);
            demo_pccm_sweep(&args);
            demo_qec(&args);
        }
        other => {
            eprintln!(
                "Unknown protocol: {}. Use bb84, bbm92, pccm-sweep, qec, or all.",
                other
            );
            std::process::exit(1);
        }
    }

    println!();
    print_success("QI-Nutshell demo complete!");
    println!();
    print_info("This demo proves Arvak can express and compile the full");
    println!("  QI-Nutshell protocol mapping: BB84, BBM92, PCCM attacks,");
    println!("  and QEC-integrated QKD — with named registers, parameterized");
    println!("  gates, barriers for protocol phases, and compilation to");
    println!("  ion-trap native gate sets.");
}

// ============================================================================
// BB84 Demo
// ============================================================================

fn demo_bb84(args: &Args) {
    print_section("BB84 Prepare-and-Measure QKD");

    println!("  Protocol: Alice prepares |ψ⟩ in a random basis,");
    println!("  sends to Bob who measures in his chosen basis.");
    println!("  Matching bases → shared key bit.");

    // Scenario 1: Clean channel, matching bases
    let clean = bb84_circuit(true, Basis::Z, Basis::Z, &EveStrategy::None);
    println!();
    print_result("Scenario 1", "Clean channel, Z basis, bit=1");
    print_circuit_stats(&clean);

    // Scenario 2: Clean channel, mismatched bases
    let mismatch = bb84_circuit(false, Basis::X, Basis::Z, &EveStrategy::None);
    print_result("Scenario 2", "Clean channel, basis mismatch (X→Z)");
    print_circuit_stats(&mismatch);

    // Scenario 3: PCCM eavesdropping
    let theta = args.theta * PI;
    let attacked = bb84_circuit(true, Basis::Z, Basis::Z, &EveStrategy::Pccm(theta));
    let (f_ab, f_ae) = pccm_fidelities(theta);
    print_result("Scenario 3", format!("PCCM attack, θ = {:.2}π", args.theta));
    print_circuit_stats(&attacked);
    print_result("  F(Alice→Bob)", format!("{:.4}", f_ab));
    print_result("  F(Alice→Eve)", format!("{:.4}", f_ae));
    print_result("  QBER", format!("{:.1}%", pccm_qber(theta) * 100.0));

    // Scenario 4: Variational PCCM (symbolic parameter)
    let variational = bb84_circuit(false, Basis::Z, Basis::Z, &EveStrategy::PccmVariational);
    print_result("Scenario 4", "Variational PCCM (symbolic θ for VQA)");
    print_circuit_stats(&variational);

    if args.show_qasm {
        print_qasm("BB84 + PCCM", &attacked);
    }

    if args.compile {
        compile_and_report("BB84 + PCCM", attacked, args.opt_level);
    }
}

// ============================================================================
// BBM92 Demo
// ============================================================================

fn demo_bbm92(args: &Args) {
    print_section("BBM92 Entanglement-Based QKD");

    println!("  Protocol: A source distributes Bell pairs to Alice & Bob.");
    println!("  Both measure in random bases. Matching bases → key bit.");
    println!("  Eve can intercept Bob's half with a cloning attack.");

    // Clean channel
    let clean = bbm92_circuit(Basis::Z, Basis::Z, &EveStrategy::None);
    println!();
    print_result("Scenario 1", "Clean entangled channel, Z basis");
    print_circuit_stats(&clean);

    // With PCCM attack
    let theta = args.theta * PI;
    let attacked = bbm92_circuit(Basis::Z, Basis::Z, &EveStrategy::Pccm(theta));
    print_result(
        "Scenario 2",
        format!("Eve clones Bob's half, θ = {:.2}π", args.theta),
    );
    print_circuit_stats(&attacked);

    if args.show_qasm {
        print_qasm("BBM92 + PCCM", &attacked);
    }

    if args.compile {
        compile_and_report("BBM92 + PCCM", attacked, args.opt_level);
    }
}

// ============================================================================
// PCCM Sweep Demo
// ============================================================================

fn demo_pccm_sweep(_args: &Args) {
    print_section("PCCM Fidelity Trade-off Analysis");

    println!("  The Phase Covariant Cloning Machine (PCCM) attack angle θ");
    println!("  controls the clone quality. Arvak compiles each configuration.");
    println!();
    println!(
        "  {:>8}  {:>8}  {:>8}  {:>8}  {:>6}  {:>5}",
        "θ/π", "F(A→B)", "F(A→E)", "QBER%", "Depth", "Gates"
    );
    println!("  {}", "─".repeat(55));

    let steps = 9;
    for i in 0..=steps {
        let theta_frac = i as f64 / (2 * steps) as f64; // 0 to 0.5
        let theta = theta_frac * PI;
        let (f_ab, f_ae) = pccm_fidelities(theta);
        let qber = pccm_qber(theta);

        let circuit = bb84_circuit(true, Basis::Z, Basis::Z, &EveStrategy::Pccm(theta));
        let depth = circuit.depth();
        let gate_count = circuit.dag().num_ops();

        let marker = if (theta_frac - 0.25).abs() < 0.01 {
            " ◀ symmetric"
        } else {
            ""
        };

        println!(
            "  {:>8.4}  {:>8.4}  {:>8.4}  {:>7.1}%  {:>6}  {:>5}{}",
            theta_frac,
            f_ab,
            f_ae,
            qber * 100.0,
            depth,
            gate_count,
            marker
        );
    }

    println!();
    let opt = optimal_symmetric_angle();
    print_result(
        "Optimal symmetric angle",
        format!("θ = π/4 = {:.4} rad", opt),
    );
}

// ============================================================================
// QEC-Integrated QKD Demo
// ============================================================================

fn demo_qec(args: &Args) {
    print_section("QEC-Integrated QKD: [[4,2,2]] Error Detection");

    println!("  The [[4,2,2]] code encodes 2 logical qubits into 4 physical");
    println!("  qubits. Stabilizer syndromes detect errors AND reveal");
    println!("  eavesdropping (\"privacy authentication\" — QI-Nutshell insight).");

    // Clean transmission
    let clean = bb84_qec_circuit([true, false], Basis::Z, Basis::Z, false);
    println!();
    print_result("Scenario 1", "Clean channel, bits=[1,0], Z basis");
    print_circuit_stats(&clean);

    // With injected error
    let noisy = bb84_qec_circuit([true, false], Basis::Z, Basis::Z, true);
    print_result("Scenario 2", "Single X error injected on data qubit 2");
    print_circuit_stats(&noisy);
    println!("  → Syndrome measurement will flag the error.");
    println!("  → Statistical deviation from expected syndromes reveals");
    println!("    eavesdropping — QEC as a channel fingerprint.");

    if args.show_qasm {
        print_qasm("BB84 + [[4,2,2]] QEC", &noisy);
    }

    if args.compile {
        compile_and_report("BB84 + [[4,2,2]] QEC (clean)", clean, args.opt_level);
        compile_and_report("BB84 + [[4,2,2]] QEC (noisy)", noisy, args.opt_level);
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn print_circuit_stats(circuit: &Circuit) {
    print_result("  Qubits", circuit.num_qubits());
    print_result("  Classical bits", circuit.num_clbits());
    print_result("  Depth", circuit.depth());
    print_result("  Gate count", circuit.dag().num_ops());
}

fn print_qasm(label: &str, circuit: &Circuit) {
    print_section(&format!("QASM3 Output: {}", label));
    match emit(circuit) {
        Ok(qasm) => println!("{}", qasm),
        Err(e) => eprintln!("  Error generating QASM: {}", e),
    }
}

fn compile_and_report(label: &str, circuit: Circuit, opt_level: u8) {
    print_section(&format!("Compilation: {} → Ion-Trap Native", label));

    let pre_depth = circuit.depth();
    let pre_gates = circuit.dag().num_ops();

    // Ion-trap target: linear chain connectivity, CZ + PRX native gates
    let num_qubits = circuit.num_qubits() as u32;
    let coupling = CouplingMap::linear(num_qubits);
    let basis = BasisGates::iqm(); // CZ + PRX (phased Rx)

    let (pm, mut props) = PassManagerBuilder::new()
        .with_optimization_level(opt_level)
        .with_target(coupling, basis)
        .build();

    let mut dag = circuit.into_dag();
    let compile_start = Instant::now();
    match pm.run(&mut dag, &mut props) {
        Ok(()) => {
            let compile_time = compile_start.elapsed();
            let compiled = Circuit::from_dag(dag);
            let post_depth = compiled.depth();
            let post_gates = compiled.dag().num_ops();

            print_result(
                "Pre-compilation",
                format!("depth={}, gates={}", pre_depth, pre_gates),
            );
            print_result(
                "Post-compilation",
                format!("depth={}, gates={}", post_depth, post_gates),
            );
            print_result("Compile time", format!("{:.2?}", compile_time));
            print_result("Optimization level", opt_level);
            print_result("Target basis", "CZ + PRX (ion-trap native)");
            print_result("Topology", format!("linear chain, {} qubits", num_qubits));

            if post_gates > 0 && pre_gates > 0 {
                let ratio = post_gates as f64 / pre_gates as f64;
                print_result("Gate expansion", format!("{:.2}×", ratio));
            }

            if compile_time.as_nanos() > 0 && post_gates > 0 {
                let gates_per_sec = post_gates as f64 / compile_time.as_secs_f64();
                print_result("Throughput", format!("{:.0} gates/s", gates_per_sec));
            }

            // Emit compiled QASM
            match emit(&compiled) {
                Ok(qasm) => {
                    let lines: Vec<_> = qasm.lines().collect();
                    print_result("Compiled QASM3", format!("{} lines", lines.len()));
                }
                Err(e) => print_result("QASM emit", format!("error: {}", e)),
            }
        }
        Err(e) => {
            eprintln!("  Compilation error: {}", e);
        }
    }
}
