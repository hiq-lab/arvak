//! HIQ HPC Scheduler for SLURM and PBS Clusters
//!
//! This crate provides enterprise-grade job scheduling for quantum circuits on HPC clusters,
//! supporting both SLURM and PBS/Torque schedulers with workflow orchestration.
//!
//! # Overview
//!
//! The scheduler manages the full lifecycle of quantum jobs on HPC systems:
//! 1. **Submission**: Convert circuits to batch jobs with proper resource allocation
//! 2. **Queuing**: Priority-based queue management with backend matching
//! 3. **Execution**: Track job status via native scheduler commands
//! 4. **Results**: Retrieve and persist execution results
//!
//! # Supported Schedulers
//!
//! | Scheduler | Commands | HPC Sites |
//! |-----------|----------|-----------|
//! | SLURM | sbatch, squeue, sacct, scancel | LUMI (CSC), many others |
//! | PBS/Torque | qsub, qstat, qdel, qhold | Various |
//!
//! # Key Features
//!
//! - **Multi-Scheduler**: Unified API for SLURM and PBS
//! - **Workflows**: DAG-based job dependencies for complex pipelines
//! - **Persistence**: JSON or SQLite storage for job state
//! - **Batch Jobs**: Submit multiple circuits as array jobs
//! - **Resource Matching**: Automatic backend selection based on circuit requirements
//!
//! # Example: Single Job Submission
//!
//! ```ignore
//! use hiq_sched::{HpcScheduler, SchedulerConfig, ScheduledJob, Priority};
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Configure for SLURM
//!     let config = SchedulerConfig::slurm()
//!         .with_partition("q_fiqci")
//!         .with_account("project_462000xxx");
//!
//!     let scheduler = HpcScheduler::new(config).await?;
//!
//!     // Create and submit job
//!     let circuit = Circuit::bell()?;
//!     let job = ScheduledJob::new("bell_test", circuit)
//!         .with_priority(Priority::high())
//!         .with_shots(1000);
//!
//!     let job_id = scheduler.submit(job).await?;
//!     println!("Submitted: {}", job_id);
//!
//!     // Poll for completion
//!     let result = scheduler.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Example: Workflow with Dependencies
//!
//! ```ignore
//! use hiq_sched::{WorkflowBuilder, HpcScheduler};
//!
//! // Create a VQE-style workflow
//! let workflow = WorkflowBuilder::new("vqe_optimization")
//!     .add_job("initial", initial_circuit)
//!     .add_job("iteration_1", iter1_circuit)
//!     .add_job("iteration_2", iter2_circuit)
//!     .add_job("final", final_circuit)
//!     .add_dependency("iteration_1", "initial")
//!     .add_dependency("iteration_2", "iteration_1")
//!     .add_dependency("final", "iteration_2")
//!     .build()?;
//!
//! let workflow_id = scheduler.submit_workflow(workflow).await?;
//! ```
//!
//! # Example: PBS Configuration
//!
//! ```ignore
//! use hiq_sched::{SchedulerConfig, PbsConfig};
//!
//! let config = SchedulerConfig::pbs()
//!     .with_queue("quantum")
//!     .with_account("my_project")
//!     .with_walltime("01:00:00");
//!
//! let scheduler = HpcScheduler::new(config).await?;
//! ```
//!
//! # Persistence
//!
//! Job state can be persisted for recovery and auditing:
//!
//! ```ignore
//! use hiq_sched::{JsonStore, SqliteStore, StateStore};
//!
//! // JSON file storage (simple, portable)
//! let store = JsonStore::new("./jobs.json")?;
//!
//! // SQLite database (queryable, efficient)
//! let store = SqliteStore::new("./jobs.db").await?;
//! ```

pub mod error;
pub mod job;
pub mod matcher;
pub mod pbs;
pub mod persistence;
pub mod queue;
pub mod scheduler;
pub mod slurm;
pub mod workflow;

// Re-exports
pub use error::{SchedError, SchedResult};
pub use job::{
    CircuitSpec, Priority, ResourceRequirements, ScheduledJob, ScheduledJobId, ScheduledJobStatus,
    TopologyPreference,
};
pub use matcher::{MatchResult, ResourceMatcher};
pub use pbs::{PbsAdapter, PbsConfig};
pub use persistence::{JsonStore, SqliteStore, StateStore};
pub use queue::PriorityQueue;
pub use scheduler::{BatchSchedulerType, HpcScheduler, Scheduler, SchedulerConfig};
pub use slurm::{SlurmAdapter, SlurmConfig};
pub use workflow::{Workflow, WorkflowBuilder, WorkflowId, WorkflowStatus};
