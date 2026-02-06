//! SLURM integration for HPC job submission.

mod adapter;
mod parser;
mod templates;

pub use adapter::{SlurmAdapter, SlurmConfig, SlurmJobInfo, SlurmState};
