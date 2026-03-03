//! Arvak backend adapter for MQT DDSIM (Decision Diagram Simulator).
//!
//! This crate provides a [`Backend`] implementation that runs quantum circuits
//! through the [MQT DDSIM](https://github.com/cda-tum/mqt-ddsim) simulator via
//! a Python subprocess.  Circuits are serialized to OpenQASM 2.0, simulated
//! using the DD-based `CircuitSimulator`, and measurement counts are returned
//! as JSON.
//!
//! # Requirements
//!
//! - `python3` on PATH
//! - `pip install mqt.ddsim` (pulls in `mqt.core` automatically)
//!
//! # Relation to arvak-qdmi DDSIM integration
//!
//! The [`arvak-qdmi`](../../crates/arvak-qdmi) crate also integrates with DDSIM
//! via the native QDMI C FFI (dlopen of a compiled `.so`).  This adapter uses
//! the Python path instead, which is simpler to set up (no CMake build needed)
//! but has subprocess overhead per job.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_ddsim::DdsimBackend;
//! use arvak_hal::Backend;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let backend = DdsimBackend::new();
//!
//!     // Check DDSIM availability
//!     let avail = backend.availability().await?;
//!     if !avail.is_available {
//!         eprintln!("Install mqt.ddsim: pip install mqt.ddsim");
//!         return Ok(());
//!     }
//!
//!     let circuit = arvak_ir::Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000, None).await?;
//!     let result = backend.result(&job_id).await?;
//!     println!("Counts: {:?}", result.counts);
//!     Ok(())
//! }
//! ```

mod backend;
mod error;

pub use backend::DdsimBackend;
pub use error::{DdsimError, DdsimResult};
