//! HIQ adapter for IBM Quantum computers.
//!
//! This crate provides a backend implementation for connecting to IBM Quantum
//! systems via the IBM Quantum Platform API (Qiskit Runtime).
//!
//! # Example
//!
//! ```ignore
//! use hiq_adapter_ibm::IbmBackend;
//! use hiq_hal::Backend;
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create backend (reads IBM_QUANTUM_TOKEN from environment)
//!     let backend = IbmBackend::new()?;
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

pub use backend::IbmBackend;
pub use error::{IbmError, IbmResult};

// Re-export common types
pub use hiq_hal::{Backend, BackendConfig, BackendFactory};
