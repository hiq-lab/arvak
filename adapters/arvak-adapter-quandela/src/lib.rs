//! Arvak Adapter for Quandela Photonic QPUs (Ascella, Belenos)
//!
//! Submits circuits to the Quandela Cloud via the Perceval Python bridge
//! (`perceval_bridge.py`).  Supports dual-rail encoding for standard gate
//! circuits and returns HAL-compliant measurement counts.
//!
//! # Supported platforms
//!
//! | Platform name   | Qubits | Description                         |
//! |-----------------|--------|-------------------------------------|
//! | `sim:ascella`   | 6      | Ascella photonic simulator (default)|
//! | `qpu:ascella`   | 6      | Ascella physical QPU                |
//! | `sim:belenos`   | 12     | Belenos simulator (launched 2025)   |
//! | `qpu:belenos`   | 12     | Belenos physical QPU (12q)          |
//! | `quandela_altair`| 5     | Legacy Altair 4K cryocooled (Alsvid)|
//!
//! # Authentication
//!
//! Set `PCVL_CLOUD_TOKEN` (or place the token in
//! `~/.openclaw/credentials/quandela/cloud.key`).
//!
//! # Alsvid integration
//!
//! The Altair platform includes a 4K Gifford-McMahon cryocooler tracked via
//! Alsvid for HOM-visibility PUF fingerprinting.  Use
//! [`QuandelaBackend::ingest_alsvid_enrollment`] and
//! [`QuandelaBackend::ingest_alsvid_schedule`] to populate the
//! `CoolingProfile`.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_quandela::QuandelaBackend;
//! use arvak_hal::Backend;
//!
//! let backend = QuandelaBackend::for_platform("sim:ascella")?;
//! let caps = backend.capabilities();
//! assert_eq!(caps.num_qubits, 6);
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
