//! Arvak Adapter for IonQ Trapped-Ion Quantum Computers
//!
//! This crate provides a backend implementation for connecting to IonQ's
//! trapped-ion quantum computers and cloud simulator via the IonQ REST API v0.4
//! (`https://api.ionq.co/v0.4`).
//!
//! # Supported Backends
//!
//! | Backend                 | Type      | Qubits | Notes                          |
//! |-------------------------|-----------|--------|--------------------------------|
//! | `simulator`             | simulator | 29     | Cloud simulator, free tier     |
//! | `qpu.aria-1`            | QPU       | 25     | Aria trapped-ion (25 alg. qubits) |
//! | `qpu.aria-2`            | QPU       | 25     | Aria trapped-ion (25 alg. qubits) |
//! | `qpu.forte-1`           | QPU       | 36     | Forte trapped-ion (36 alg. qubits)|
//! | `qpu.forte-enterprise-1`| QPU       | 36     | Forte Enterprise               |
//!
//! # Authentication
//!
//! Set the `IONQ_API_KEY` environment variable to your IonQ API key:
//!
//! ```bash
//! export IONQ_API_KEY="your-ionq-api-key"
//! ```
//!
//! Get a free API key at <https://cloud.ionq.com>.
//! The free tier includes unlimited simulator access (up to 29 qubits).
//!
//! # Gate Set
//!
//! This adapter uses the IonQ **QIS gateset** — standard quantum gates that
//! IonQ compiles to native gates (GPI, GPI2, MS) server-side.  This means
//! Arvak does not need to target IonQ-specific native gates.
//!
//! | QIS gate  | Arvak IR gate | Description                     |
//! |-----------|---------------|---------------------------------|
//! | `h`       | `H`           | Hadamard                        |
//! | `cx`      | `CX`          | CNOT (controlled-X)             |
//! | `rx`      | `Rx`          | X-axis rotation (radians)       |
//! | `ry`      | `Ry`          | Y-axis rotation (radians)       |
//! | `rz`      | `Rz`          | Z-axis rotation (radians)       |
//! | `x/y/z`   | `X/Y/Z`       | Pauli gates                     |
//! | `s/t/sx`  | `S/T/SX`      | Clifford gates                  |
//! | `swap`    | `SWAP`        | Swap gate                       |
//! | `xx/yy/zz`| `RXX/RYY/RZZ` | Ising coupling gates (radians)  |
//! | `ccx`     | `CCX`         | Toffoli (double-controlled X)   |
//!
//! # Connectivity
//!
//! All IonQ hardware has **all-to-all qubit connectivity** — no routing is
//! required.  Use `Topology::full(n)` or `CouplingMap.full(n)`.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_ionq::IonQBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Cloud simulator — requires IONQ_API_KEY
//!     let backend = IonQBackend::new()?;
//!
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     println!("Job: {}", job_id);
//!
//!     // Poll until complete...
//!     let result = backend.result(&job_id).await?;
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```

mod api;
mod backend;
mod error;

pub use backend::IonQBackend;
pub use error::{IonQError, IonQResult};

// Re-export common types for convenience.
pub use arvak_hal::{Backend, BackendConfig, BackendFactory};
