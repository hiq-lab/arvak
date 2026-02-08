//! Arvak backend adapter for NVIDIA CUDA-Q.
//!
//! This crate provides an Arvak [`Backend`] implementation
//! that submits circuits to CUDA-Q targets (GPU-accelerated simulators and
//! hardware backends) via REST API. Circuits are converted to OpenQASM 3.0
//! as the interchange format.
//!
//! # Targets
//!
//! | Target | Description |
//! |--------|-------------|
//! | `nvidia-mqpu` | Multi-GPU statevector simulator (up to 40 qubits) |
//! | `custatevec` | Single-GPU statevector simulator |
//! | `tensornet` | Tensor network simulator (large qubit counts, shallow circuits) |
//! | `density-matrix` | Density matrix simulator (noise modeling) |
//!
//! # Authentication
//!
//! Set the `CUDAQ_API_TOKEN` environment variable, or pass credentials
//! via [`CudaqBackend::with_credentials`].
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_cudaq::CudaqBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::{Circuit, QubitId};
//!
//! let backend = CudaqBackend::new()?;
//!
//! let mut circuit = Circuit::with_size("ghz", 3, 3);
//! circuit.h(QubitId(0))?;
//! circuit.cx(QubitId(0), QubitId(1))?;
//! circuit.cx(QubitId(1), QubitId(2))?;
//! circuit.measure_all()?;
//!
//! let job_id = backend.submit(&circuit, 1000).await?;
//! let result = backend.wait(&job_id).await?;
//!
//! for (bitstring, count) in result.counts.sorted() {
//!     println!("  {} : {}", bitstring, count);
//! }
//! ```

pub mod api;
pub mod backend;
pub mod error;

pub use backend::{CudaqBackend, DEFAULT_ENDPOINT, DEFAULT_TARGET, targets};
pub use error::{CudaqError, CudaqResult};

// Re-export key HAL types for convenience.
pub use arvak_hal::backend::{Backend, BackendConfig, BackendFactory};
