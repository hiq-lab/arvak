//! Arvak Adapter for AQT (Alpine Quantum Technologies) Quantum Computers
//!
//! This crate provides a backend implementation for connecting to AQT's
//! ion-trap quantum computers and simulators via the Arnica cloud REST API
//! (`https://arnica.aqt.eu/api/v1`).
//!
//! # Supported Resources
//!
//! | Workspace        | Resource                      | Type              | Qubits | Notes                         |
//! |------------------|-------------------------------|-------------------|--------|-------------------------------|
//! | `default`        | `offline_simulator_no_noise`  | offline_simulator | 20     | Local, any token, free        |
//! | `default`        | `offline_simulator_noise`     | offline_simulator | 20     | Local with AQT noise model    |
//! | `aqt_simulators` | `simulator_noise`             | simulator         | 20     | Cloud-hosted, needs account   |
//! | *(account)*      | `ibex`                        | device            | 12     | IBEX Q1 hardware (QV=128)     |
//!
//! # Authentication
//!
//! Set the `AQT_TOKEN` environment variable to your AQT static Bearer token:
//!
//! ```bash
//! export AQT_TOKEN="your-aqt-token"
//! ```
//!
//! **Offline simulators** work with any token value (even empty string)
//! — no AQT account is needed for local testing.
//!
//! # Gate Set
//!
//! AQT uses a minimal native gate set: `{RZ, R, RXX, MEASURE}`.
//! Circuits must be pre-compiled to these native gates before submission.
//! Arvak's compiler handles this via `GateSet::aqt()` + `BasisTranslation`.
//!
//! | AQT gate | Arvak IR gate | Description                    |
//! |----------|---------------|--------------------------------|
//! | `RZ`     | `Rz`          | Z-axis rotation (angle ÷ π)    |
//! | `R`      | `PRX`         | Phased-X rotation              |
//! | `RXX`    | `RXX`         | Mølmer-Sørensen entangling gate|
//! | `MEASURE`| (measurement) | Projective measurement, all qubits |
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_aqt::AqtBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Offline noiseless simulator — no account needed
//!     let backend = AqtBackend::new()?;
//!
//!     let circuit = Circuit::bell()?;
//!     // Circuit must be pre-compiled to AQT native gates (rz, prx, rxx)
//!     let job_id = backend.submit(&circuit, 100).await?;
//!     println!("Job: {}", job_id);
//!
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```

mod api;
mod backend;
mod error;

pub use backend::AqtBackend;
pub use error::{AqtError, AqtResult};

// Re-export common types for convenience.
pub use arvak_hal::{Backend, BackendConfig, BackendFactory};
