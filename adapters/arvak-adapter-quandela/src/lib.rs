//! Arvak Adapter for Quandela Altair Photonic QPU
//!
//! This crate provides a backend implementation for the Quandela Altair
//! photonic quantum processor. Quandela Altair uses dual-rail photonic
//! encoding, operating 5 logical qubits in 10 photonic modes.
//!
//! # Architecture
//!
//! - **Gate encoding**: Clifford+T subset via perceval-interop dual-rail
//!   beamsplitter decomposition (DEBT-Q4: encoding pass pending)
//! - **Cooling**: 4K Gifford-McMahon cryocooler — tracked via Alsvid for
//!   HOM visibility PUF fingerprinting
//! - **Submission**: REST API (DEBT-Q5: endpoint TBD)
//!
//! # Alsvid Integration
//!
//! Use `QuandelaBackend::ingest_alsvid_enrollment` to populate the
//! `CoolingProfile` PUF enrollment from alsvid-lab output.
//! Use `QuandelaBackend::ingest_alsvid_schedule` to add quiet-window hints.
//!
//! # Authentication
//!
//! Set the `QUANDELA_API_KEY` environment variable.
//!
//! # Status
//!
//! Circuit submission (`submit()`) returns `DEBT-Q4` until the photonic
//! dual-rail encoding pass is implemented. `validate()` returns
//! `RequiresTranspilation` to signal this to orchestrators.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_quandela::QuandelaBackend;
//! use arvak_hal::Backend;
//!
//! let backend = QuandelaBackend::new()?;
//! let caps = backend.capabilities();
//! assert_eq!(caps.num_qubits, 5);
//! assert!(caps.features.contains(&"photonic".to_string()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod api;
mod backend;
mod decoherence;
mod error;

pub use backend::{AlsvidEnrollment, QuandelaBackend};
pub use error::{QuandelaError, QuandelaResult};

// Re-export common HAL types for convenience.
pub use arvak_hal::{Backend, BackendConfig, BackendFactory};
