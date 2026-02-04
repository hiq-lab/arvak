//! HIQ adapter for IQM quantum computers.
//!
//! This crate provides a backend implementation for connecting to IQM
//! quantum computers via the Resonance cloud API.
//!
//! # Example
//!
//! ```ignore
//! use hiq_adapter_iqm::IqmBackend;
//! use hiq_hal::Backend;
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create backend (reads IQM_TOKEN from environment)
//!     let backend = IqmBackend::new()?;
//!
//!     // Submit circuit
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!
//!     // Wait for result
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!     Ok(())
//! }
//! ```

mod api;
mod backend;
mod error;

pub use backend::IqmBackend;
pub use error::{IqmError, IqmResult};

// Re-export common types
pub use hiq_hal::{Backend, BackendConfig, BackendFactory};
