//! PBS (Portable Batch System) integration for HPC job submission.
//!
//! This module provides support for PBS/Torque/PBS Pro schedulers commonly
//! used in HPC environments. It mirrors the SLURM adapter interface for
//! consistent job submission across different schedulers.

mod adapter;
mod parser;
mod templates;

pub use adapter::{PbsAdapter, PbsConfig, PbsJobInfo, PbsState};
