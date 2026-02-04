//! HIQ Circuit Intermediate Representation
//!
//! This crate provides the core data structures for representing quantum circuits
//! in HIQ. It includes:
//!
//! - Qubit and classical bit identifiers
//! - Standard and custom quantum gates
//! - Parameter expressions for parameterized circuits
//! - Instructions combining gates with operands
//! - DAG-based circuit representation for compilation
//! - High-level Circuit builder API

pub mod circuit;
pub mod dag;
pub mod error;
pub mod gate;
pub mod instruction;
pub mod parameter;
pub mod qubit;

pub use circuit::Circuit;
pub use dag::{CircuitDag, DagEdge, DagNode, NodeIndex, WireId};
pub use error::{IrError, IrResult};
pub use gate::{ClassicalCondition, CustomGate, Gate, GateKind, StandardGate};
pub use instruction::{Instruction, InstructionKind};
pub use parameter::ParameterExpression;
pub use qubit::{Clbit, ClbitId, Qubit, QubitId};
