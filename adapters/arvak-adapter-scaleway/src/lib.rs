//! Arvak Adapter for Scaleway Quantum-as-a-Service
//!
//! This crate provides a backend implementation for connecting to quantum
//! hardware via Scaleway's QaaS platform. Currently supports IQM Garnet (20Q)
//! and IQM Emerald (54Q) through Scaleway's cloud infrastructure.
//!
//! # Supported Platforms
//!
//! | Platform | Hardware | Qubits | Native Gates | Pricing |
//! |----------|----------|--------|--------------|---------|
//! | QPU-GARNET-20PQ | IQM Garnet | 20 | PRX, CZ | €0.22/circuit + €0.0012/shot |
//! | QPU-EMERALD-54PQ | IQM Emerald | 54 | PRX, CZ | Contact Scaleway |
//!
//! # Architecture
//!
//! Scaleway QaaS uses a session-based execution model:
//!
//! 1. Create a **session** via Scaleway console (links to a platform/QPU)
//! 2. Submit **jobs** within that session (each job = one circuit execution)
//! 3. Poll for **results** (inline on job or via results endpoint)
//!
//! This adapter handles steps 2-3. Sessions are managed externally.
//!
//! # Authentication
//!
//! ```bash
//! export SCALEWAY_SECRET_KEY="your-secret-key"
//! export SCALEWAY_PROJECT_ID="your-project-id"
//! export SCALEWAY_SESSION_ID="your-active-session-id"
//! export SCALEWAY_PLATFORM="QPU-GARNET-20PQ"  # optional, this is the default
//! ```
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_scaleway::ScalewayBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let backend = ScalewayBackend::new()?;
//!
//!     // Check session status
//!     let avail = backend.availability().await?;
//!     println!("Available: {}", avail.is_available);
//!
//!     // Submit a circuit
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 4000).await?;
//!
//!     // Poll until done
//!     loop {
//!         let status = backend.status(&job_id).await?;
//!         if status.is_terminal() { break; }
//!         tokio::time::sleep(std::time::Duration::from_secs(2)).await;
//!     }
//!
//!     // Get results
//!     let result = backend.result(&job_id).await?;
//!     println!("Counts: {:?}", result.counts);
//!     Ok(())
//! }
//! ```
//!
//! # Differences from Direct IQM Access
//!
//! - **Auth:** `X-Auth-Token` header (not Bearer token)
//! - **Sessions:** Jobs must target an active session (no direct QPU access)
//! - **Circuits:** Submitted as QASM3 via `qiskit_circuit` field
//! - **Results:** Returned as `result_distribution` (bitstring → count map)
//! - **Pricing:** Per-circuit + per-shot (vs. IQM Resonance subscription)

mod api;
mod backend;
mod error;

pub use backend::ScalewayBackend;
pub use error::{ScalewayError, ScalewayResult};

// Re-export common types
pub use arvak_hal::{Backend, BackendConfig, BackendFactory};
