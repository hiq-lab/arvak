//! HIQ Adapter for IBM Quantum Computers
//!
//! This crate provides a backend implementation for connecting to IBM Quantum
//! systems via the IBM Quantum Platform API (Qiskit Runtime).
//!
//! # Supported Systems
//!
//! | System | Qubits | Access | Native Gates |
//! |--------|--------|--------|--------------|
//! | IBM Quantum (Cloud) | 5-127+ | API Token | SX, RZ, X, CX |
//!
//! # Authentication
//!
//! Set the `IBM_QUANTUM_TOKEN` environment variable with your IBM Quantum API token:
//!
//! ```bash
//! export IBM_QUANTUM_TOKEN="your-api-token-here"
//! ```
//!
//! You can obtain a token from [IBM Quantum](https://quantum.ibm.com/).
//!
//! # Example: Running on IBM Quantum
//!
//! ```ignore
//! use hiq_adapter_ibm::IbmBackend;
//! use hiq_hal::Backend;
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create backend (reads IBM_QUANTUM_TOKEN from environment)
//!     let backend = IbmBackend::new()?;
//!
//!     // Check available systems
//!     let caps = backend.capabilities();
//!     println!("Max qubits: {}", caps.max_qubits);
//!
//!     // Create and submit a Bell state
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     println!("Job submitted: {}", job_id);
//!
//!     // Wait for execution (may take minutes due to queue)
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Native Gate Set
//!
//! IBM Quantum hardware uses a native gate set of:
//! - **SX**: √X gate
//! - **RZ(θ)**: Z-rotation (virtual, no error)
//! - **X**: Pauli-X gate
//! - **CX**: Controlled-NOT (two-qubit entangling gate)
//!
//! The HIQ compiler automatically translates circuits to this basis when
//! targeting IBM hardware.
//!
//! # Queue Times
//!
//! IBM Quantum systems may have significant queue times depending on:
//! - Your IBM Quantum plan (Open, Premium)
//! - System availability
//! - Circuit size and complexity
//!
//! Use the simulator (`hiq-adapter-sim`) for development and testing.

mod api;
mod backend;
mod error;

pub use backend::IbmBackend;
pub use error::{IbmError, IbmResult};

// Re-export common types
pub use hiq_hal::{Backend, BackendConfig, BackendFactory};
