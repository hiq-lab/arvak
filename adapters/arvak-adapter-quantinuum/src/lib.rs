//! Arvak Adapter for Quantinuum Quantum Computers
//!
//! This crate provides a backend implementation for connecting to Quantinuum
//! H1 and H2 ion-trap quantum computers and their noiseless emulators via the
//! Quantinuum cloud REST API (`https://qapi.quantinuum.com/v1/`).
//!
//! # Supported Systems
//!
//! | System     | Qubits | Type      | Description                    |
//! |------------|--------|-----------|--------------------------------|
//! | H2-1LE     | 32     | Emulator  | Noiseless H2 emulator (free)   |
//! | H2-1E      | 32     | Emulator  | Noisy H2 emulator              |
//! | H1-1E      | 20     | Emulator  | Noisy H1 emulator              |
//! | H2-1       | 32     | Hardware  | H2 trapped-ion processor       |
//! | H1-1       | 20     | Hardware  | H1 trapped-ion processor       |
//!
//! # Authentication
//!
//! Set the `QUANTINUUM_EMAIL` and `QUANTINUUM_PASSWORD` environment variables:
//!
//! ```bash
//! export QUANTINUUM_EMAIL="user@example.com"
//! export QUANTINUUM_PASSWORD="yourpassword"
//! ```
//!
//! The backend exchanges these credentials for a JWT on first use and refreshes
//! the token automatically on expiry.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_quantinuum::QuantinuumBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // H2-1LE: noiseless emulator, free to use
//!     let backend = QuantinuumBackend::new()?;
//!
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     println!("Job: {}", job_id);
//!
//!     let result = backend.wait(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```
//!
//! # Gate Set
//!
//! Quantinuum's cloud service accepts standard QASM 2.0 gates and compiles
//! them to its native ion-trap gate set (ZZMax, ZZPhase, U1q, Rz) internally.
//! Arvak submits circuits in `OPENQASM 2.0` format.
//!
//! Supported gates: `rz`, `rx`, `ry`, `h`, `x`, `y`, `z`, `s`, `t`,
//! `cx`, `cz`, `swap`, `ccx`.

mod api;
mod backend;
mod error;

pub use backend::QuantinuumBackend;
pub use error::{QuantinuumError, QuantinuumResult};

// Re-export common types for convenience.
pub use arvak_hal::{Backend, BackendConfig, BackendFactory};
