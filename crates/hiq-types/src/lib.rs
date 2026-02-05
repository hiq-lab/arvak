//! High-level quantum types for HIQ.
//!
//! This crate provides Qrisp-inspired high-level quantum types that make it easier
//! to work with quantum arithmetic and data structures. These types automatically
//! manage qubit allocation and provide intuitive interfaces for quantum operations.
//!
//! # Types
//!
//! - [`QuantumInt`] - Fixed-point integer with configurable bit width
//! - [`QuantumFloat`] - Floating-point representation using sign, mantissa, exponent
//! - [`QuantumArray`] - Array of quantum values
//!
//! # Example
//!
//! ```ignore
//! use hiq_types::{QuantumInt, QuantumFloat};
//! use hiq_ir::Circuit;
//!
//! let mut circuit = Circuit::new("arithmetic");
//! let a = QuantumInt::<4>::new(&mut circuit); // 4-bit integer
//! let b = QuantumInt::<4>::new(&mut circuit);
//!
//! // Arithmetic operations automatically generate circuit gates
//! let sum = a.add(&b, &mut circuit);
//! ```

mod error;
mod quantum_array;
mod quantum_float;
mod quantum_int;
mod register;

pub use error::{TypeError, TypeResult};
pub use quantum_array::{QuantumArray, QuantumIndex};
pub use quantum_float::{QFloat16, QFloat32, QFloat8, QuantumFloat};
pub use quantum_int::{create_pair, QuantumInt};
pub use register::{QubitRegister, RegisterAllocation};
