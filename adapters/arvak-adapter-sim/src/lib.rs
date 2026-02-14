//! Arvak Local Statevector Simulator
//!
//! This crate provides a high-performance local quantum simulator for testing,
//! development, and small-scale experiments. It uses statevector simulation,
//! which provides exact results but is limited to ~20-25 qubits.
//!
//! # Features
//!
//! - **Exact Simulation**: Full statevector representation (no sampling noise)
//! - **All Standard Gates**: Supports all gates from `arvak-ir`
//! - **Measurement Sampling**: Probabilistic measurement with configurable shots
//! - **No External Dependencies**: Pure Rust implementation
//!
//! # Performance
//!
//! | Qubits | Memory | Simulation Speed |
//! |--------|--------|------------------|
//! | 10 | ~16 KB | Instant |
//! | 15 | ~512 KB | Fast |
//! | 20 | ~16 MB | Moderate |
//! | 25 | ~512 MB | Slow |
//! | 30+ | ~16 GB+ | Not recommended |
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_sim::SimulatorBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create simulator
//!     let backend = SimulatorBackend::new();
//!
//!     // Verify capabilities (sync, infallible)
//!     let caps = backend.capabilities();
//!     println!("Max qubits: {}", caps.num_qubits);
//!     println!("Max shots: {}", caps.max_shots);
//!
//!     // Run a Bell state
//!     let circuit = Circuit::bell()?;
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     let result = backend.wait(&job_id).await?;
//!
//!     // Expect ~50% |00⟩ and ~50% |11⟩
//!     println!("Results: {:?}", result.counts);
//!
//!     Ok(())
//! }
//! ```

mod simulator;
mod statevector;

pub use simulator::SimulatorBackend;
