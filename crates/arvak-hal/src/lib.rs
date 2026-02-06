//! HIQ Hardware Abstraction Layer
//!
//! This crate provides a unified interface for interacting with quantum backends,
//! enabling HIQ to work seamlessly with simulators, cloud services, and HPC systems.
//!
//! # Overview
//!
//! The HAL abstracts away backend-specific details, providing:
//! - A common [`Backend`] trait for job submission and management
//! - [`Capabilities`] to describe hardware features and constraints
//! - Authentication support for various providers (API tokens, OIDC)
//! - Unified result handling via [`ExecutionResult`] and [`Counts`]
//!
//! # Supported Backends
//!
//! | Backend | Crate | Authentication |
//! |---------|-------|----------------|
//! | Local Simulator | `hiq-adapter-sim` | None |
//! | IQM Resonance | `hiq-adapter-iqm` | `IQM_TOKEN` env var |
//! | IBM Quantum | `hiq-adapter-ibm` | `IBM_QUANTUM_TOKEN` env var |
//! | IQM on LUMI/LRZ | `hiq-adapter-iqm` | OIDC (CSC/LRZ) |
//!
//! # Example: Running a Circuit
//!
//! ```ignore
//! use hiq_hal::{Backend, BackendConfig};
//! use hiq_adapter_sim::SimulatorBackend;
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create a Bell state circuit
//!     let circuit = Circuit::bell()?;
//!
//!     // Initialize the simulator backend
//!     let backend = SimulatorBackend::new();
//!
//!     // Submit the job
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     println!("Job submitted: {}", job_id);
//!
//!     // Wait for results
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     // Analyze the most frequent outcome
//!     if let Some((bitstring, count)) = result.counts.most_frequent() {
//!         println!("Most frequent: {} ({} times)", bitstring, count);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # OIDC Authentication (HPC Sites)
//!
//! For HPC deployments like LUMI (CSC Finland) or LRZ (Germany), use OIDC:
//!
//! ```ignore
//! use hiq_hal::{OidcConfig, OidcAuth};
//!
//! // Configure for LUMI
//! let config = OidcConfig::lumi();
//! let auth = OidcAuth::new(config).await?;
//!
//! // Get access token for API calls
//! let token = auth.get_token().await?;
//! ```
//!
//! # Implementing a Custom Backend
//!
//! ```ignore
//! use hiq_hal::{Backend, Capabilities, JobId, JobStatus, ExecutionResult, HalResult};
//! use hiq_ir::Circuit;
//! use async_trait::async_trait;
//!
//! struct MyBackend { /* ... */ }
//!
//! #[async_trait]
//! impl Backend for MyBackend {
//!     fn name(&self) -> &str { "my_backend" }
//!
//!     fn capabilities(&self) -> &Capabilities {
//!         // Return hardware capabilities
//!     }
//!
//!     async fn submit(&self, circuit: &Circuit, shots: u32) -> HalResult<JobId> {
//!         // Submit circuit to hardware
//!     }
//!
//!     async fn status(&self, job_id: &JobId) -> HalResult<JobStatus> {
//!         // Query job status
//!     }
//!
//!     async fn result(&self, job_id: &JobId) -> HalResult<ExecutionResult> {
//!         // Retrieve execution results
//!     }
//! }
//! ```

pub mod auth;
pub mod backend;
pub mod capability;
pub mod error;
pub mod job;
pub mod result;

pub use auth::{CachedToken, EnvTokenProvider, OidcAuth, OidcConfig, TokenProvider};
pub use backend::{Backend, BackendConfig, BackendFactory};
pub use capability::{Capabilities, GateSet, Topology, TopologyKind};
pub use error::{HalError, HalResult};
pub use job::{Job, JobId, JobStatus};
pub use result::{Counts, ExecutionResult};
