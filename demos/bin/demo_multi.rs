//! Multi-Job Orchestration Demo
//!
//! Demonstrates Arvak's ability to manage multiple quantum workloads simultaneously.

use clap::Parser;

use arvak_demos::runners::orchestrator::{DemoJob, default_demo_jobs, run_multi_demo};
use arvak_demos::{print_header, print_info, print_result, print_section, print_success};

#[derive(Parser, Debug)]
#[command(name = "demo-multi")]
#[command(about = "Demonstrate multi-job orchestration")]
struct Args {
    /// Disable progress display
    #[arg(long)]
    no_progress: bool,

    /// Run minimal demo (fewer jobs)
    #[arg(long)]
    minimal: bool,
}

fn main() {
    let args = Args::parse();

    print_header("Multi-Job Orchestration Demo");

    print_section("Arvak Orchestration Value Proposition");
    println!("  This demo showcases Arvak's core capability:");
    println!("  Managing multiple quantum workloads simultaneously.");
    println!();
    println!("  In a production HPC environment:");
    println!("  - Multiple users submit different quantum jobs");
    println!("  - SLURM schedules jobs across available backends");
    println!("  - Arvak tracks job state and collects results");
    println!("  - Resources are shared efficiently");

    let jobs = if args.minimal {
        vec![
            DemoJob::Grover {
                n_qubits: 3,
                marked_state: 5,
            },
            DemoJob::Batch { count: 3 },
        ]
    } else {
        default_demo_jobs()
    };

    print_section("Job Queue");
    for (i, job) in jobs.iter().enumerate() {
        let desc = match job {
            DemoJob::Grover {
                n_qubits,
                marked_state,
            } => {
                format!(
                    "Grover search: {n_qubits} qubits, searching for |{marked_state}⟩"
                )
            }
            DemoJob::Vqe { iterations } => {
                format!("VQE H₂ molecule: {iterations} iterations")
            }
            DemoJob::Qaoa { layers } => {
                format!("QAOA Max-Cut: {layers} layers")
            }
            DemoJob::Batch { count } => {
                format!("Batch execution: {count} circuits")
            }
        };
        println!("  [{}] {}", i + 1, desc);
    }

    print_section("Executing Jobs");
    println!();

    let result = run_multi_demo(&jobs, !args.no_progress);

    println!();
    print_section("Results Summary");

    for job_result in &result.jobs {
        println!();
        print_result("Job", &job_result.name);
        print_result("Type", &job_result.job_type);
        print_result("Duration", format!("{:.2?}", job_result.duration));
        print_result("Result", &job_result.summary);
        print_result(
            "Status",
            if job_result.success {
                "Success"
            } else {
                "Failed"
            },
        );
    }

    print_section("Aggregate Statistics");
    print_result("Total jobs", result.jobs.len());
    print_result("Successful", result.successful);
    print_result("Failed", result.failed);
    print_result("Total time", format!("{:.2?}", result.total_duration));

    // Calculate average job time
    let avg_time = result.total_duration / result.jobs.len() as u32;
    print_result("Average job time", format!("{avg_time:.2?}"));

    print_section("Demo Narrative");
    println!("  This orchestration demo shows Arvak managing:");
    println!("  - Different algorithm types (Grover, VQE, QAOA)");
    println!("  - Various resource requirements");
    println!("  - Batch job processing");
    println!();
    println!("  In production, Arvak would:");
    println!("  - Submit jobs to SLURM queue");
    println!("  - Route to appropriate quantum backends");
    println!("  - Handle job dependencies (workflows)");
    println!("  - Persist state for reliability");
    println!("  - Provide real-time monitoring");

    print_section("HPC Integration Points");
    println!("  1. SLURM Integration:");
    println!("     - sbatch for job submission");
    println!("     - squeue for status monitoring");
    println!("     - sacct for historical data");
    println!();
    println!("  2. Resource Management:");
    println!("     - CPU nodes for classical optimization");
    println!("     - QPU time allocation");
    println!("     - Memory management");
    println!();
    println!("  3. Job Scheduling:");
    println!("     - Priority queues");
    println!("     - Fair share scheduling");
    println!("     - Backend selection");

    println!();
    if result.failed == 0 {
        print_success("All jobs completed successfully!");
    } else {
        println!("Warning: {} job(s) failed", result.failed);
    }
    println!();
    print_info(
        "This is Arvak's core value: orchestrating quantum workloads on HPC infrastructure.",
    );
}
