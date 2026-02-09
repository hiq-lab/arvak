// SPDX-License-Identifier: Apache-2.0
//! # arvak-qdmi
//!
//! Native Rust integration with the [QDMI](https://github.com/Munich-Quantum-Software-Stack/QDMI)
//! (Quantum Device Management Interface) for the Arvak quantum compilation platform.
//!
//! This crate consumes the **QDMI device interface** directly — loading device
//! shared libraries via `dlopen` and resolving prefix-shifted symbols — rather
//! than going through the QDMI client/driver layer. This gives Arvak direct
//! access to device sessions, capabilities, and job submission.
//!
//! ## Architecture
//!
//! ```text
//!                  ┌──────────────────┐
//!                  │   Arvak Compiler  │
//!                  └────────┬─────────┘
//!                           │ DeviceCapabilities
//!                  ┌────────┴─────────┐
//!                  │   arvak-qdmi     │
//!                  │                  │
//!                  │  QdmiDevice      │ ← dlopen + prefix-aware dlsym
//!                  │  DeviceSession   │ ← RAII session management
//!                  │  DeviceJob       │ ← RAII job lifecycle
//!                  │  DeviceCapab.    │ ← structured query results
//!                  └────────┬─────────┘
//!                           │ C ABI (extern "C")
//!               ┌───────────┴───────────┐
//!               │  QDMI Device .so      │
//!               │  (e.g. EX_, DDSIM_)   │
//!               └───────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use std::path::Path;
//! use arvak_qdmi::device_loader::QdmiDevice;
//! use arvak_qdmi::session::DeviceSession;
//! use arvak_qdmi::capabilities::DeviceCapabilities;
//!
//! // Load a device library with its prefix
//! let device = QdmiDevice::load(
//!     Path::new("libqdmi_example.so"),
//!     "EX",
//! ).expect("failed to load device");
//!
//! // Open a session (three-phase: alloc → init)
//! let session = DeviceSession::open(&device)
//!     .expect("failed to open session");
//!
//! // Query all capabilities
//! let caps = DeviceCapabilities::query(&session)
//!     .expect("failed to query capabilities");
//!
//! println!("Device: {} ({} qubits)", caps.name, caps.num_qubits);
//! println!("Coupling edges: {}", caps.coupling_map.num_edges());
//!
//! for (site_id, props) in &caps.site_properties {
//!     if let Some(t1) = props.t1 {
//!         println!("  Site {:?}: T1 = {:?}", site_id, t1);
//!     }
//! }
//! ```

pub mod capabilities;
pub mod device_loader;
pub mod error;
pub mod ffi;
pub mod format;
pub mod session;

// Re-export the most commonly used types at crate root.
pub use capabilities::{CouplingMap, DeviceCapabilities, OperationId, SiteId};
pub use device_loader::QdmiDevice;
pub use error::QdmiError;
pub use format::CircuitFormat;
pub use session::{DeviceJob, DeviceSession};
