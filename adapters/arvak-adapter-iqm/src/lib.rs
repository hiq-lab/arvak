//! HIQ Adapter for IQM Quantum Computers
//!
//! This crate provides a backend implementation for connecting to IQM quantum
//! computers via the Resonance cloud API and on-premise HPC installations.
//!
//! # Supported Systems
//!
//! | System | Qubits | Access | Native Gates |
//! |--------|--------|--------|--------------|
//! | IQM Resonance (Cloud) | 5-20 | API Token | PRX, CZ |
//! | LUMI Helmi (CSC Finland) | 5 | OIDC | PRX, CZ |
//! | LRZ (Germany) | 20 | OIDC | PRX, CZ |
//!
//! # Authentication
//!
//! ## Cloud Access (Resonance)
//!
//! Set the `IQM_TOKEN` environment variable:
//!
//! ```bash
//! export IQM_TOKEN="your-api-token-here"
//! ```
//!
//! ## HPC Access (LUMI/LRZ)
//!
//! Use OIDC authentication via `hiq-hal`:
//!
//! ```ignore
//! use hiq_hal::{OidcConfig, OidcAuth};
//! use hiq_adapter_iqm::IqmBackend;
//!
//! let config = OidcConfig::lumi();  // or OidcConfig::lrz()
//! let auth = OidcAuth::new(config).await?;
//! let backend = IqmBackend::with_auth(auth)?;
//! ```
//!
//! # Example: Cloud Execution
//!
//! ```ignore
//! use hiq_adapter_iqm::IqmBackend;
//! use hiq_hal::Backend;
//! use hiq_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create backend (reads IQM_TOKEN from environment)
//!     let backend = IqmBackend::new()?;
//!
//!     // Check capabilities
//!     let caps = backend.capabilities();
//!     println!("Device: {} qubits", caps.max_qubits);
//!
//!     // Submit a Bell state circuit
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     println!("Job submitted: {}", job_id);
//!
//!     // Wait for hardware execution
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Native Gate Set
//!
//! IQM hardware uses a native gate set of:
//! - **PRX(θ, φ)**: Phased rotation around X-axis
//! - **CZ**: Controlled-Z (two-qubit entangling gate)
//!
//! The HIQ compiler automatically translates circuits to this basis when
//! targeting IQM hardware.

mod api;
mod backend;
mod error;

pub use backend::IqmBackend;
pub use error::{IqmError, IqmResult};

// Re-export common types
pub use hiq_hal::{Backend, BackendConfig, BackendFactory};
