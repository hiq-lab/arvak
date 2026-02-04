//! HIQ Hardware Abstraction Layer
//!
//! This crate provides the abstraction layer for interacting with
//! quantum backends. It defines:
//!
//! - Backend trait for submitting and managing jobs
//! - Capabilities for describing backend features
//! - Job management (submission, status, results)
//! - Result types for execution output

pub mod backend;
pub mod capability;
pub mod error;
pub mod job;
pub mod result;

pub use backend::{Backend, BackendConfig, BackendFactory};
pub use capability::{Capabilities, GateSet, Topology, TopologyKind};
pub use error::{HalError, HalResult};
pub use job::{Job, JobId, JobStatus};
pub use result::{Counts, ExecutionResult};
