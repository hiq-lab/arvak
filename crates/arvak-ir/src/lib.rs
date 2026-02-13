//! Arvak Circuit Intermediate Representation
//!
//! This crate provides the core data structures for representing quantum circuits
//! in Arvak. It forms the foundation of the entire Arvak compilation stack.
//!
//! # Overview
//!
//! The circuit IR uses a DAG (Directed Acyclic Graph) representation internally,
//! which enables efficient compilation and optimization passes. The high-level
//! [`Circuit`] API provides a convenient builder pattern for constructing circuits.
//!
//! # Core Components
//!
//! - **Qubits and Classical Bits**: [`QubitId`], [`ClbitId`] for addressing quantum
//!   and classical registers
//! - **Gates**: [`StandardGate`] for built-in gates (H, X, CX, etc.) and [`CustomGate`]
//!   for user-defined operations
//! - **Parameters**: [`ParameterExpression`] for symbolic parameters in variational circuits
//! - **Instructions**: [`Instruction`] combining gates with their operands
//! - **DAG**: [`CircuitDag`] for the internal graph representation
//! - **Circuit**: [`Circuit`] high-level builder API
//!
//! # Example: Building a Bell State
//!
//! ```rust
//! use arvak_ir::{Circuit, QubitId};
//!
//! // Create a new circuit with 2 qubits and 2 classical bits
//! let mut circuit = Circuit::with_size("bell_state", 2, 2);
//!
//! // Build the Bell state: |00⟩ → (|00⟩ + |11⟩)/√2
//! circuit.h(QubitId(0)).unwrap();
//! circuit.cx(QubitId(0), QubitId(1)).unwrap();
//!
//! // Add measurement
//! circuit.measure_all().unwrap();
//!
//! assert_eq!(circuit.num_qubits(), 2);
//! assert!(circuit.depth() >= 2);  // H, CX, measure
//! ```
//!
//! # Example: Parameterized Circuit
//!
//! ```rust
//! use arvak_ir::{Circuit, QubitId, ParameterExpression};
//! use std::f64::consts::PI;
//!
//! // Create a 1-qubit circuit
//! let mut circuit = Circuit::with_size("variational", 1, 0);
//!
//! // Create a symbolic parameter
//! let theta = ParameterExpression::symbol("theta");
//!
//! // Add parameterized rotation
//! circuit.rx(theta.clone(), QubitId(0)).unwrap();
//!
//! // Later, bind the parameter to a concrete value
//! let bound = theta.bind("theta", PI / 4.0);
//! ```
//!
//! # Supported Gates
//!
//! | Gate | Qubits | Description |
//! |------|--------|-------------|
//! | `H` | 1 | Hadamard gate |
//! | `X`, `Y`, `Z` | 1 | Pauli gates |
//! | `S`, `Sdg` | 1 | S and S-dagger gates |
//! | `T`, `Tdg` | 1 | T and T-dagger gates |
//! | `Rx`, `Ry`, `Rz` | 1 | Rotation gates |
//! | `U` | 1 | Universal single-qubit gate U(θ,φ,λ) |
//! | `CX` | 2 | Controlled-NOT (CNOT) |
//! | `CY`, `CZ` | 2 | Controlled-Y and Controlled-Z |
//! | `Swap` | 2 | SWAP gate |
//! | `CCX` | 3 | Toffoli (CCNOT) gate |

pub mod circuit;
pub mod dag;
pub mod error;
pub mod gate;
pub mod instruction;
pub mod noise;
pub mod parameter;
pub mod qubit;

pub use circuit::Circuit;
pub use dag::{CircuitDag, CircuitLevel, DagEdge, DagNode, NodeIndex, WireId};
pub use error::{IrError, IrResult};
pub use gate::{ClassicalCondition, CustomGate, Gate, GateKind, StandardGate};
pub use instruction::{Instruction, InstructionKind};
pub use noise::{NoiseModel, NoiseProfile, NoiseRole};
pub use parameter::ParameterExpression;
pub use qubit::{Clbit, ClbitId, Qubit, QubitId};
