//! Arvak Adapter for QDMI (Quantum Device Management Interface)
//!
//! This crate provides an Arvak backend implementation for quantum devices accessible
//! via QDMI, the Quantum Device Management Interface developed as part of the
//! Munich Quantum Software Stack (MQSS).
//!
//! # Overview
//!
//! QDMI is a standardized C-based interface for accessing quantum devices,
//! developed by:
//! - Leibniz Supercomputing Centre (LRZ)
//! - Technical University of Munich (TUM) - Chair for Design Automation (CDA)
//! - Technical University of Munich (TUM) - Chair for Computer Architecture and
//!   Parallel Systems (CAPS)
//!
//! This adapter enables Arvak to submit circuits to any QDMI-compliant device,
//! providing access to the quantum hardware infrastructure at European HPC centers.
//!
//! # Features
//!
//! - **Session Management**: Authenticated sessions with token/OIDC support
//! - **Device Queries**: Query device properties (qubits, topology, gate fidelities)
//! - **Job Submission**: Submit `OpenQASM` 3.0 circuits via QDMI
//! - **Result Retrieval**: Get measurement counts and histograms
//! - **Mock Mode**: Testing without QDMI library installed
//!
//! # Feature Flags
//!
//! - `system-qdmi`: Link against the system QDMI library (requires `libqdmi.so`)
//!
//! Without this feature, the adapter runs in mock mode for testing.
//!
//! # Example
//!
//! ```ignore
//! use arvak_adapter_qdmi::QdmiBackend;
//! use arvak_hal::Backend;
//! use arvak_ir::Circuit;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create QDMI backend with authentication
//!     let backend = QdmiBackend::new()
//!         .with_token("your-api-token")
//!         .with_base_url("https://qdmi.lrz.de");
//!
//!     // Check availability
//!     let avail = backend.availability().await?;
//!     if !avail.is_available {
//!         eprintln!("QDMI device not available");
//!         return Ok(());
//!     }
//!
//!     // Get device capabilities (sync, infallible)
//!     let caps = backend.capabilities();
//!     println!("Device: {} with {} qubits", caps.name, caps.num_qubits);
//!
//!     // Create a Bell state circuit
//!     let mut circuit = Circuit::with_size("bell", 2, 2);
//!     circuit.h(arvak_ir::QubitId(0))?;
//!     circuit.cx(arvak_ir::QubitId(0), arvak_ir::QubitId(1))?;
//!     circuit.measure_all();
//!
//!     // Submit and wait for results
//!     let job_id = backend.submit(&circuit, 1000).await?;
//!     let result = backend.wait(&job_id).await?;
//!
//!     // Print results
//!     println!("Results ({} shots):", result.shots);
//!     for (bitstring, count) in result.counts.sorted() {
//!         println!("  {} : {}", bitstring, count);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Integration with MQSS
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        Arvak                                │
//! │  ┌──────────┐  ┌────────────┐  ┌─────────────────────────┐ │
//! │  │  arvak-ir  │→ │arvak-compile │→ │       arvak-hal           │ │
//! │  │(Circuit) │  │ (Optimize) │  │      (Backend)          │ │
//! │  └──────────┘  └────────────┘  └───────────┬─────────────┘ │
//! │                                            │               │
//! │  ┌─────────────────────────────────────────┼─────────────┐ │
//! │  │            Backend Adapters             │             │ │
//! │  │  ┌─────────┐ ┌─────────┐ ┌──────────────┴───────────┐│ │
//! │  │  │   IQM   │ │   IBM   │ │   arvak-adapter-qdmi       ││ │
//! │  │  └─────────┘ └─────────┘ └──────────────┬───────────┘│ │
//! │  └─────────────────────────────────────────┼─────────────┘ │
//! └────────────────────────────────────────────┼───────────────┘
//!                                              │
//!                                              ▼
//!                              ┌───────────────────────────────┐
//!                              │      QDMI (libqdmi.so)        │
//!                              │  Munich Quantum Software Stack │
//!                              └───────────────┬───────────────┘
//!                                              │
//!                  ┌───────────────────────────┼───────────────────────────┐
//!                  ▼                           ▼                           ▼
//!           ┌──────────────┐           ┌──────────────┐           ┌──────────────┐
//!           │ IQM Quantum  │           │   Rigetti    │           │    Other     │
//!           │   (Garnet)   │           │   (Aspen)    │           │   Backends   │
//!           └──────────────┘           └──────────────┘           └──────────────┘
//! ```
//!
//! # QDMI Compatibility
//!
//! This adapter is compatible with QDMI version 1.x and supports:
//!
//! | Feature | Status |
//! |---------|--------|
//! | `OpenQASM` 2.0 | ✅ Supported |
//! | `OpenQASM` 3.0 | ✅ Supported (preferred) |
//! | QIR Base Profile | ⚠️ Future |
//! | Token Auth | ✅ Supported |
//! | OIDC Auth | ✅ Supported |
//! | Device Properties | ✅ Supported |
//! | Site Properties (T1/T2) | ✅ Supported |
//! | Operation Properties | ✅ Supported |
//!
//! # See Also
//!
//! - [QDMI GitHub Repository](https://github.com/Munich-Quantum-Software-Stack/QDMI)
//! - [Munich Quantum Software Stack](https://www.lrz.de/services/compute/quantum/)
//! - [HIQ Documentation](https://github.com/hiq-lab/arvak)

pub mod backend;
pub mod error;
pub mod ffi;

pub use backend::QdmiBackend;
pub use error::{QdmiError, QdmiResult};

// Re-export FFI types for advanced usage
pub use ffi::{
    QdmiDeviceProperty, QdmiDevicePulseSupportLevel, QdmiDeviceStatus, QdmiJobParameter,
    QdmiJobProperty, QdmiJobResult, QdmiJobStatus, QdmiOperationProperty, QdmiProgramFormat,
    QdmiSessionParameter, QdmiSessionProperty, QdmiSiteProperty, QdmiStatus,
};
