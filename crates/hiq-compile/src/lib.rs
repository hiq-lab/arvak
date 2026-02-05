//! HIQ Compilation and Transpilation Framework
//!
//! This crate provides the compilation infrastructure for transforming
//! quantum circuits to run on target hardware. It includes:
//!
//! - Pass trait for implementing compilation passes
//! - PassManager for orchestrating compilation
//! - PropertySet for sharing data between passes
//! - Built-in passes for layout, routing, and basis translation
//! - Optimization passes for gate cancellation and merging

pub mod error;
pub mod manager;
pub mod pass;
pub mod property;
pub mod unitary;

// Built-in passes
pub mod passes;

pub use error::{CompileError, CompileResult};
pub use manager::{PassManager, PassManagerBuilder};
pub use pass::{AnalysisPass, Pass, PassKind, TransformationPass};
pub use property::{BasisGates, CouplingMap, Layout, PropertySet};
