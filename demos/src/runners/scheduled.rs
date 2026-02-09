//! Scheduler-integrated demo runners.
//!
//! This module provides runners that integrate with Arvak's HPC scheduler
//! for realistic job submission and tracking.

use std::sync::Arc;
use std::time::Duration;

use arvak_qasm3::emit;
use arvak_sched::{
    CircuitSpec, HpcScheduler, Priority, ResourceRequirements, SchedResult, ScheduledJob,
    ScheduledJobId, ScheduledJobStatus, Scheduler, WorkflowBuilder,
};

use crate::circuits::grover::{grover_circuit, optimal_iterations};
use crate::circuits::qaoa::qaoa_circuit;
use crate::circuits::vqe::two_local_ansatz;
use crate::problems::Graph;

/// Result from a scheduled demo job.
#[derive(Debug, Clone)]
pub struct ScheduledDemoResult {
    /// Job ID assigned by the scheduler.
    pub job_id: ScheduledJobId,
    /// Final job status.
    pub status: ScheduledJobStatus,
    /// Human-readable description of the result.
    pub description: String,
}

/// A demo runner that uses the Arvak scheduler for job submission.
pub struct ScheduledRunner {
    scheduler: Arc<HpcScheduler>,
}

impl ScheduledRunner {
    /// Create a new scheduled runner.
    pub fn new(scheduler: Arc<HpcScheduler>) -> Self {
        Self { scheduler }
    }

    /// Submit a Grover search demo to the scheduler.
    pub async fn submit_grover(
        &self,
        n_qubits: usize,
        marked_state: usize,
        priority: Priority,
    ) -> SchedResult<ScheduledJobId> {
        let iterations = optimal_iterations(n_qubits);
        let circuit = grover_circuit(n_qubits, marked_state, iterations);

        // Convert to QASM3
        let qasm = emit(&circuit)?;
        let spec = CircuitSpec::from_qasm(qasm);

        // Create resource requirements
        let requirements = ResourceRequirements::new(n_qubits as u32);

        // Create and submit the job
        let job = ScheduledJob::new(format!("grover_{n_qubits}q_search_{marked_state}"), spec)
            .with_priority(priority)
            .with_shots(1024)
            .with_requirements(requirements);

        self.scheduler.submit(job).await
    }

    /// Submit a VQE energy evaluation to the scheduler.
    ///
    /// This submits a single energy evaluation circuit. For full VQE
    /// optimization, use the `VqeRunner` instead.
    pub async fn submit_vqe_evaluation(
        &self,
        params: &[f64],
        n_qubits: usize,
        reps: usize,
        priority: Priority,
    ) -> SchedResult<ScheduledJobId> {
        let circuit = two_local_ansatz(n_qubits, reps, params);

        // Convert to QASM3
        let qasm = emit(&circuit)?;
        let spec = CircuitSpec::from_qasm(qasm);

        // VQE uses small molecule Hamiltonians
        let requirements = ResourceRequirements::new(n_qubits as u32);

        let job = ScheduledJob::new(format!("vqe_{n_qubits}q_evaluation"), spec)
            .with_priority(priority)
            .with_shots(1024)
            .with_requirements(requirements);

        self.scheduler.submit(job).await
    }

    /// Submit a QAOA circuit for Max-Cut to the scheduler.
    pub async fn submit_qaoa(
        &self,
        graph: &Graph,
        gamma: &[f64],
        beta: &[f64],
        priority: Priority,
    ) -> SchedResult<ScheduledJobId> {
        let circuit = qaoa_circuit(graph, gamma, beta);

        // Convert to QASM3
        let qasm = emit(&circuit)?;
        let spec = CircuitSpec::from_qasm(qasm);

        let requirements = ResourceRequirements::new(graph.n_nodes as u32);

        let job = ScheduledJob::new(format!("qaoa_maxcut_{}nodes", graph.n_nodes), spec)
            .with_priority(priority)
            .with_shots(1024)
            .with_requirements(requirements);

        self.scheduler.submit(job).await
    }

    /// Submit a batch of simple circuits.
    pub async fn submit_batch(
        &self,
        circuits: Vec<arvak_ir::Circuit>,
        name: &str,
        priority: Priority,
    ) -> SchedResult<ScheduledJobId> {
        let specs: Vec<CircuitSpec> = circuits
            .iter()
            .map(|c| {
                let qasm = emit(c).unwrap_or_default();
                CircuitSpec::from_qasm(qasm)
            })
            .collect();

        let n_qubits = circuits
            .iter()
            .map(arvak_ir::Circuit::num_qubits)
            .max()
            .unwrap_or(2) as u32;
        let requirements = ResourceRequirements::new(n_qubits);

        self.scheduler
            .submit_batch(name, specs, 1024, priority, requirements)
            .await
    }

    /// Create a workflow that runs multiple demo algorithms.
    pub async fn submit_demo_workflow(&self) -> SchedResult<arvak_sched::WorkflowId> {
        // Create circuits for each demo
        let grover_circuit = grover_circuit(4, 7, optimal_iterations(4));
        let vqe_circuit = two_local_ansatz(2, 2, &[0.1; 6]);
        let qaoa_circuit_demo = qaoa_circuit(&Graph::square_4(), &[0.5], &[0.5]);

        // Create jobs
        let grover_job = ScheduledJob::new(
            "demo_grover",
            CircuitSpec::from_qasm(emit(&grover_circuit)?),
        )
        .with_priority(Priority::high())
        .with_shots(1024)
        .with_requirements(ResourceRequirements::new(4));

        let vqe_job =
            ScheduledJob::new("demo_vqe_eval", CircuitSpec::from_qasm(emit(&vqe_circuit)?))
                .with_priority(Priority::default())
                .with_shots(1024)
                .with_requirements(ResourceRequirements::new(2));

        let qaoa_job = ScheduledJob::new(
            "demo_qaoa",
            CircuitSpec::from_qasm(emit(&qaoa_circuit_demo)?),
        )
        .with_priority(Priority::default())
        .with_shots(1024)
        .with_requirements(ResourceRequirements::new(4));

        // Build workflow: Grover runs first, then VQE and QAOA in parallel
        let grover_id = grover_job.id.clone();

        let workflow = WorkflowBuilder::new("demo_workflow")
            .add_job(grover_job)
            .then(vqe_job)?
            .add_job_after(qaoa_job, &grover_id)?
            .build();

        self.scheduler.submit_workflow(workflow).await
    }

    /// Wait for a job to complete and return the execution result.
    pub async fn wait(
        &self,
        job_id: &ScheduledJobId,
    ) -> SchedResult<arvak_hal::result::ExecutionResult> {
        self.scheduler.wait(job_id).await
    }

    /// Check the status of a job.
    pub async fn status(&self, job_id: &ScheduledJobId) -> SchedResult<ScheduledJobStatus> {
        self.scheduler.status(job_id).await
    }

    /// Wait for a workflow to complete.
    pub async fn wait_workflow(&self, workflow_id: &arvak_sched::WorkflowId) -> SchedResult<()> {
        self.scheduler.wait_workflow(workflow_id).await
    }
}

/// Configuration for running scheduled demos.
#[derive(Debug, Clone)]
pub struct ScheduledDemoConfig {
    /// Whether to use mock SLURM for testing.
    pub mock_slurm: bool,
    /// Polling interval for job status checks.
    pub poll_interval: Duration,
    /// Default priority for demo jobs.
    pub default_priority: Priority,
}

impl Default for ScheduledDemoConfig {
    fn default() -> Self {
        Self {
            mock_slurm: true,
            poll_interval: Duration::from_millis(100),
            default_priority: Priority::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_demo_config_default() {
        let config = ScheduledDemoConfig::default();
        assert!(config.mock_slurm);
        assert_eq!(config.poll_interval, Duration::from_millis(100));
    }
}
