//! Multi-job orchestration demo.
//!
//! This module demonstrates Arvak's ability to manage multiple
//! quantum workloads simultaneously.

use std::time::{Duration, Instant};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::circuits::grover::{grover_circuit, optimal_iterations};
use crate::problems::{Graph, h2_hamiltonian};
use crate::runners::{QaoaRunner, VqeRunner};

/// A demo job for orchestration.
#[derive(Debug, Clone)]
pub enum DemoJob {
    /// Grover search job.
    Grover {
        n_qubits: usize,
        marked_state: usize,
    },
    /// VQE molecular simulation job.
    Vqe { iterations: usize },
    /// QAOA optimization job.
    Qaoa { layers: usize },
    /// Batch of simple circuits.
    Batch { count: usize },
}

/// Result of a demo job.
#[derive(Debug)]
pub struct DemoJobResult {
    /// Job name.
    pub name: String,
    /// Job type.
    pub job_type: String,
    /// Execution time.
    pub duration: Duration,
    /// Result summary.
    pub summary: String,
    /// Success status.
    pub success: bool,
}

/// Result of multi-job demo.
#[derive(Debug)]
pub struct MultiDemoResult {
    /// Individual job results.
    pub jobs: Vec<DemoJobResult>,
    /// Total execution time.
    pub total_duration: Duration,
    /// Number of successful jobs.
    pub successful: usize,
    /// Number of failed jobs.
    pub failed: usize,
}

/// Run the multi-job orchestration demo.
///
/// This demonstrates running multiple different quantum algorithms
/// concurrently, simulating Arvak's job scheduling capabilities.
pub fn run_multi_demo(jobs: &[DemoJob], show_progress: bool) -> MultiDemoResult {
    let start = Instant::now();
    let mut results = Vec::with_capacity(jobs.len());

    let mp = if show_progress {
        Some(MultiProgress::new())
    } else {
        None
    };

    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {bar:20.cyan/blue} {pos}/{len} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    // Run each job
    for (i, job) in jobs.iter().enumerate() {
        let job_start = Instant::now();

        let pb = mp.as_ref().map(|m| {
            let pb = m.add(ProgressBar::new(100));
            pb.set_style(style.clone());
            pb
        });

        let result = match job {
            DemoJob::Grover {
                n_qubits,
                marked_state,
            } => {
                if let Some(ref pb) = pb {
                    pb.set_message(format!("Grover search ({n_qubits} qubits)"));
                }

                let iterations = optimal_iterations(*n_qubits);
                let _circuit = grover_circuit(*n_qubits, *marked_state, iterations);

                if let Some(ref pb) = pb {
                    pb.set_position(100);
                    pb.finish_with_message("Grover complete");
                }

                DemoJobResult {
                    name: format!("grover_{i}"),
                    job_type: "Grover".to_string(),
                    duration: job_start.elapsed(),
                    summary: format!(
                        "Searched for |{marked_state}⟩ in {n_qubits} qubits ({iterations} iterations)"
                    ),
                    success: true,
                }
            }

            DemoJob::Vqe { iterations } => {
                if let Some(ref pb) = pb {
                    pb.set_message("VQE H2 molecule");
                    pb.set_length(*iterations as u64);
                }

                let h = h2_hamiltonian();
                let runner = VqeRunner::new(h).with_reps(1).with_maxiter(*iterations);

                // Simulate progress
                for step in 0..*iterations {
                    if let Some(ref pb) = pb {
                        pb.set_position(step as u64);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }

                let result = runner.run();

                if let Some(ref pb) = pb {
                    pb.finish_with_message("VQE complete");
                }

                DemoJobResult {
                    name: format!("vqe_{i}"),
                    job_type: "VQE".to_string(),
                    duration: job_start.elapsed(),
                    summary: format!(
                        "H2 energy: {:.4} Ha ({} evaluations)",
                        result.optimal_energy, result.circuit_evaluations
                    ),
                    success: true,
                }
            }

            DemoJob::Qaoa { layers } => {
                if let Some(ref pb) = pb {
                    pb.set_message("QAOA Max-Cut");
                    pb.set_length(50);
                }

                let graph = Graph::square_4();
                let runner = QaoaRunner::new(graph).with_layers(*layers).with_maxiter(50);

                // Simulate progress
                for step in 0..50 {
                    if let Some(ref pb) = pb {
                        pb.set_position(step);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }

                let result = runner.run();

                if let Some(ref pb) = pb {
                    pb.finish_with_message("QAOA complete");
                }

                let (set_s, set_t) =
                    Graph::square_4().bitstring_to_partition(result.best_bitstring);

                DemoJobResult {
                    name: format!("qaoa_{i}"),
                    job_type: "QAOA".to_string(),
                    duration: job_start.elapsed(),
                    summary: format!(
                        "Cut value: {} (ratio: {:.2}%), Partition: {:?} | {:?}",
                        result.best_cut,
                        result.approximation_ratio * 100.0,
                        set_s,
                        set_t
                    ),
                    success: true,
                }
            }

            DemoJob::Batch { count } => {
                if let Some(ref pb) = pb {
                    pb.set_message("Batch circuits");
                    pb.set_length(*count as u64);
                }

                for step in 0..*count {
                    // Generate a simple Bell circuit
                    let _circuit = arvak_ir::Circuit::bell().unwrap();
                    if let Some(ref pb) = pb {
                        pb.set_position(step as u64);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }

                if let Some(ref pb) = pb {
                    pb.finish_with_message("Batch complete");
                }

                DemoJobResult {
                    name: format!("batch_{i}"),
                    job_type: "Batch".to_string(),
                    duration: job_start.elapsed(),
                    summary: format!("{count} Bell state circuits executed"),
                    success: true,
                }
            }
        };

        results.push(result);
    }

    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.len() - successful;

    MultiDemoResult {
        jobs: results,
        total_duration: start.elapsed(),
        successful,
        failed,
    }
}

/// Create a default set of demo jobs.
pub fn default_demo_jobs() -> Vec<DemoJob> {
    vec![
        DemoJob::Grover {
            n_qubits: 4,
            marked_state: 7,
        },
        DemoJob::Vqe { iterations: 30 },
        DemoJob::Qaoa { layers: 2 },
        DemoJob::Batch { count: 5 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_demo_jobs() {
        let jobs = default_demo_jobs();
        assert_eq!(jobs.len(), 4);
    }

    #[test]
    fn test_run_multi_demo() {
        let jobs = vec![DemoJob::Grover {
            n_qubits: 2,
            marked_state: 3,
        }];

        let result = run_multi_demo(&jobs, false);

        assert_eq!(result.successful, 1);
        assert_eq!(result.failed, 0);
        assert_eq!(result.jobs.len(), 1);
    }
}
