//! Automatic uncomputation for HIQ quantum circuits.
//!
//! This crate provides Qrisp-inspired automatic uncomputation capabilities.
//! It can analyze quantum circuits to determine which ancilla qubits can be
//! safely uncomputed (returned to |0⟩) by inverting the operations that
//! created them.
//!
//! # Overview
//!
//! In quantum computing, many algorithms require temporary "ancilla" qubits
//! that hold intermediate results. To reuse these qubits or to avoid
//! entanglement with the final result, they need to be "uncomputed" - returned
//! to their initial |0⟩ state.
//!
//! Manual uncomputation is error-prone and tedious. This crate provides:
//!
//! - [`UncomputeContext`] - Marks a section of circuit for automatic uncomputation
//! - [`analyze_uncomputation`] - Analyzes which qubits can be safely uncomputed
//! - Gate inversion utilities for reversing operations
//!
//! # Example
//!
//! ```ignore
//! use hiq_auto::{UncomputeContext, uncompute};
//! use hiq_ir::Circuit;
//!
//! let mut circuit = Circuit::new("with_uncompute");
//!
//! // Mark the start of a computation
//! let ctx = UncomputeContext::begin(&circuit);
//!
//! // ... perform operations on ancilla qubits ...
//!
//! // Automatically uncompute the ancillas
//! uncompute(&mut circuit, ctx)?;
//! ```

mod analysis;
mod context;
mod error;
mod inverse;

pub use analysis::{analyze_uncomputation, find_computational_cone, find_reversible_ops, UncomputeAnalysis};
pub use context::{uncompute, UncomputeContext, UncomputeScope};
pub use error::{UncomputeError, UncomputeResult};
pub use inverse::{inverse_gate, inverse_instruction, is_self_inverse, InverseStrategy};
