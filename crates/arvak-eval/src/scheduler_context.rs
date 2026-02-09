//! Scheduler Context: LRZ reference constraints, walltime mapping, batch limits.
//!
//! Models the constraints of HPC scheduler environments (SLURM/PBS)
//! that affect quantum circuit execution planning.
//!
//! Reference: LRZ SuperMUC-NG / LUMI `q_fiqci` partition.

use serde::{Deserialize, Serialize};

/// Scheduler constraints for a target HPC environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConstraints {
    /// Name of the HPC site.
    pub site: String,
    /// Scheduler type (slurm, pbs).
    pub scheduler_type: String,
    /// Partition/queue name.
    pub partition: String,
    /// Maximum walltime in seconds.
    pub max_walltime_seconds: u64,
    /// Maximum concurrent batch jobs.
    pub max_batch_jobs: u32,
    /// Maximum qubits available on the quantum device.
    pub max_qubits: u32,
    /// Whether the site supports array jobs.
    pub supports_array_jobs: bool,
    /// Estimated queue wait time in seconds (typical).
    pub typical_queue_wait_seconds: u64,
}

impl SchedulerConstraints {
    /// LRZ (Leibniz Rechenzentrum) reference constraints.
    ///
    /// Based on the LRZ quantum computing partition with IQM backend.
    pub fn lrz() -> Self {
        Self {
            site: "LRZ".into(),
            scheduler_type: "slurm".into(),
            partition: "qc_iqm".into(),
            max_walltime_seconds: 3600, // 1 hour
            max_batch_jobs: 10,
            max_qubits: 20,
            supports_array_jobs: true,
            typical_queue_wait_seconds: 120,
        }
    }

    /// LUMI (CSC Finland) reference constraints.
    ///
    /// Based on the `q_fiqci` partition with Helmi (IQM) backend.
    pub fn lumi() -> Self {
        Self {
            site: "LUMI".into(),
            scheduler_type: "slurm".into(),
            partition: "q_fiqci".into(),
            max_walltime_seconds: 900, // 15 minutes
            max_batch_jobs: 5,
            max_qubits: 5,
            supports_array_jobs: true,
            typical_queue_wait_seconds: 60,
        }
    }

    /// Generic simulator constraints (no scheduler limitations).
    pub fn simulator() -> Self {
        Self {
            site: "local".into(),
            scheduler_type: "none".into(),
            partition: "default".into(),
            max_walltime_seconds: u64::MAX,
            max_batch_jobs: u32::MAX,
            max_qubits: 30,
            supports_array_jobs: false,
            typical_queue_wait_seconds: 0,
        }
    }
}

/// Walltime estimation for a circuit on a given scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalltimeEstimate {
    /// Estimated execution time in seconds (circuit only).
    pub execution_seconds: f64,
    /// Estimated total time including overhead (compilation, setup, readout).
    pub total_seconds: f64,
    /// Whether this fits within the scheduler's walltime limit.
    pub fits_walltime: bool,
    /// Recommended walltime request in seconds (with safety margin).
    pub recommended_walltime: u64,
    /// Number of batch iterations that fit in one walltime slot.
    pub batch_capacity: u32,
}

/// Scheduler fitness assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerFitness {
    /// Target scheduler constraints.
    pub constraints: SchedulerConstraints,
    /// Walltime estimate.
    pub walltime: WalltimeEstimate,
    /// Whether the circuit's qubit count fits the device.
    pub qubits_fit: bool,
    /// Whether batching is recommended for this workload.
    pub batch_recommended: bool,
    /// Recommended batch size (circuits per job).
    pub recommended_batch_size: u32,
    /// Overall fitness (0.0 = incompatible, 1.0 = perfect fit).
    pub fitness_score: f64,
    /// Human-readable assessment.
    pub assessment: String,
}

/// Context for scheduler-aware evaluation.
pub struct SchedulerContext;

impl SchedulerContext {
    /// Evaluate scheduler fitness for a circuit workload.
    pub fn evaluate(
        num_qubits: usize,
        circuit_depth: usize,
        total_ops: usize,
        constraints: &SchedulerConstraints,
    ) -> SchedulerFitness {
        let walltime = Self::estimate_walltime(circuit_depth, total_ops, constraints);
        let qubits_fit = num_qubits as u32 <= constraints.max_qubits;

        let batch_recommended = walltime.batch_capacity > 1;
        let recommended_batch_size = walltime.batch_capacity.min(constraints.max_batch_jobs);

        let fitness_score = Self::compute_fitness(
            qubits_fit,
            walltime.fits_walltime,
            walltime.batch_capacity,
            constraints,
        );

        let assessment = Self::generate_assessment(num_qubits, qubits_fit, &walltime, constraints);

        SchedulerFitness {
            constraints: constraints.clone(),
            walltime,
            qubits_fit,
            batch_recommended,
            recommended_batch_size,
            fitness_score,
            assessment,
        }
    }

    /// Estimate walltime for a circuit.
    fn estimate_walltime(
        circuit_depth: usize,
        _total_ops: usize,
        constraints: &SchedulerConstraints,
    ) -> WalltimeEstimate {
        // Rough model:
        // - Each gate layer: ~1 microsecond on real hardware
        // - Compilation overhead: ~5 seconds
        // - Setup/calibration: ~10 seconds
        // - Readout: ~1 second per 1000 shots (assume 1024)
        let gate_time_seconds = circuit_depth as f64 * 1e-6;
        let compilation_overhead = 5.0;
        let setup_overhead = 10.0;
        let readout_time = 1.0;

        let execution_seconds = gate_time_seconds + readout_time;
        let total_seconds = execution_seconds + compilation_overhead + setup_overhead;

        let max_walltime = constraints.max_walltime_seconds as f64;
        let fits_walltime = total_seconds < max_walltime;

        // Safety margin: 2x estimated time, minimum 60 seconds
        let recommended_walltime = ((total_seconds * 2.0).ceil() as u64)
            .max(60)
            .min(constraints.max_walltime_seconds);

        // How many circuit executions fit in one walltime slot?
        let batch_capacity = if total_seconds > 0.0 {
            ((max_walltime * 0.9) / total_seconds).floor() as u32
        } else {
            constraints.max_batch_jobs
        }
        .max(1)
        .min(constraints.max_batch_jobs);

        WalltimeEstimate {
            execution_seconds,
            total_seconds,
            fits_walltime,
            recommended_walltime,
            batch_capacity,
        }
    }

    /// Compute an overall fitness score.
    fn compute_fitness(
        qubits_fit: bool,
        fits_walltime: bool,
        batch_capacity: u32,
        _constraints: &SchedulerConstraints,
    ) -> f64 {
        if !qubits_fit {
            return 0.0;
        }
        if !fits_walltime {
            return 0.1;
        }

        let base = 0.5;
        let batch_bonus = (f64::from(batch_capacity) / 10.0).min(0.3);
        let walltime_bonus = 0.2; // fits within limits

        (base + batch_bonus + walltime_bonus).min(1.0)
    }

    /// Generate a human-readable assessment.
    fn generate_assessment(
        num_qubits: usize,
        qubits_fit: bool,
        walltime: &WalltimeEstimate,
        constraints: &SchedulerConstraints,
    ) -> String {
        if !qubits_fit {
            return format!(
                "Circuit requires {} qubits but {} ({}) only supports {}",
                num_qubits, constraints.site, constraints.partition, constraints.max_qubits
            );
        }

        if !walltime.fits_walltime {
            return format!(
                "Estimated runtime ({:.1}s) exceeds {} walltime limit ({}s)",
                walltime.total_seconds, constraints.site, constraints.max_walltime_seconds
            );
        }

        if walltime.batch_capacity > 1 {
            format!(
                "Good fit for {} ({}). Up to {} iterations per job slot. Recommended walltime: {}s",
                constraints.site,
                constraints.partition,
                walltime.batch_capacity,
                walltime.recommended_walltime
            )
        } else {
            format!(
                "Fits {} ({}) with recommended walltime {}s",
                constraints.site, constraints.partition, walltime.recommended_walltime
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lrz_constraints() {
        let lrz = SchedulerConstraints::lrz();
        assert_eq!(lrz.site, "LRZ");
        assert_eq!(lrz.scheduler_type, "slurm");
        assert_eq!(lrz.max_qubits, 20);
        assert!(lrz.supports_array_jobs);
    }

    #[test]
    fn test_lumi_constraints() {
        let lumi = SchedulerConstraints::lumi();
        assert_eq!(lumi.site, "LUMI");
        assert_eq!(lumi.partition, "q_fiqci");
        assert_eq!(lumi.max_qubits, 5);
    }

    #[test]
    fn test_walltime_small_circuit() {
        let constraints = SchedulerConstraints::lrz();
        let fitness = SchedulerContext::evaluate(5, 10, 15, &constraints);

        assert!(fitness.qubits_fit);
        assert!(fitness.walltime.fits_walltime);
        assert!(fitness.fitness_score > 0.5);
        assert!(fitness.walltime.recommended_walltime >= 60);
    }

    #[test]
    fn test_qubits_too_large() {
        let constraints = SchedulerConstraints::lumi();
        let fitness = SchedulerContext::evaluate(10, 10, 15, &constraints);

        assert!(!fitness.qubits_fit);
        assert_eq!(fitness.fitness_score, 0.0);
        assert!(fitness.assessment.contains("only supports"));
    }

    #[test]
    fn test_batch_capacity() {
        let constraints = SchedulerConstraints::lrz();
        let fitness = SchedulerContext::evaluate(5, 10, 15, &constraints);

        assert!(fitness.walltime.batch_capacity >= 1);
        assert!(fitness.recommended_batch_size <= constraints.max_batch_jobs);
    }

    #[test]
    fn test_simulator_constraints() {
        let sim = SchedulerConstraints::simulator();
        let fitness = SchedulerContext::evaluate(20, 100, 500, &sim);

        assert!(fitness.qubits_fit);
        assert!(fitness.walltime.fits_walltime);
    }

    #[test]
    fn test_fitness_score_range() {
        let constraints = SchedulerConstraints::lrz();
        let fitness = SchedulerContext::evaluate(5, 10, 15, &constraints);
        assert!(fitness.fitness_score >= 0.0);
        assert!(fitness.fitness_score <= 1.0);
    }
}
