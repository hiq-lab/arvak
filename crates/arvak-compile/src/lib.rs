//! Arvak Compilation and Transpilation Framework
//!
//! This crate provides the compilation infrastructure for transforming
//! quantum circuits to run on target hardware. It implements a pass-based
//! architecture similar to LLVM, enabling modular and extensible compilation.
//!
//! # Overview
//!
//! The compilation process transforms an input circuit through a series of
//! passes that:
//! 1. **Layout**: Map virtual qubits to physical qubits on the target device
//! 2. **Routing**: Insert SWAP gates to satisfy connectivity constraints
//! 3. **Translation**: Convert gates to the target's native gate set
//! 4. **Optimization**: Reduce gate count and circuit depth
//!
//! # Architecture
//!
//! ```text
//! Input Circuit
//!       │
//!       ▼
//! ┌─────────────┐
//! │ PassManager │ ◄── PropertySet (coupling map, basis gates, layout)
//! └─────────────┘
//!       │
//!       ├── TrivialLayout / DenseLayout
//!       ├── BasicRouting / SabreRouting
//!       ├── BasisTranslation
//!       └── Optimize1qGates / CancelCX / CommutativeCancellation
//!       │
//!       ▼
//! Output Circuit (hardware-compatible)
//! ```
//!
//! # Example: Basic Compilation
//!
//! ```rust
//! use arvak_compile::{PassManagerBuilder, CouplingMap, BasisGates};
//! use arvak_ir::Circuit;
//!
//! // Create a circuit
//! let circuit = Circuit::bell().unwrap();
//!
//! // Build pass manager for IQM target
//! let (pm, mut props) = PassManagerBuilder::new()
//!     .with_optimization_level(2)
//!     .with_target(CouplingMap::star(5), BasisGates::iqm())
//!     .build();
//!
//! // Compile the circuit
//! let mut dag = circuit.into_dag();
//! pm.run(&mut dag, &mut props).unwrap();
//!
//! let compiled = Circuit::from_dag(dag);
//! println!("Compiled depth: {}", compiled.depth());
//! ```
//!
//! # Optimization Levels
//!
//! | Level | Passes Included |
//! |-------|-----------------|
//! | 0 | Layout + Routing only |
//! | 1 | + Basis translation |
//! | 2 | + CX cancellation, 1q optimization |
//! | 3 | + Commutative cancellation, aggressive optimization |
//!
//! # Built-in Passes
//!
//! ## Layout Passes
//! - [`passes::TrivialLayout`]: Simple 1:1 mapping of virtual to physical qubits
//! - [`passes::DenseLayout`]: Pack qubits into well-connected region
//!
//! ## Routing Passes
//! - [`passes::BasicRouting`]: Greedy SWAP insertion for connectivity
//!
//! ## Translation Passes
//! - [`passes::BasisTranslation`]: Convert to target gate set (IQM: PRX+CZ, IBM: SX+RZ+CX)
//!
//! ## Optimization Passes
//! - [`passes::Optimize1qGates`]: Merge consecutive 1-qubit gates via ZYZ decomposition
//! - [`passes::CancelCX`]: Cancel adjacent CX·CX pairs
//! - [`passes::CommutativeCancellation`]: Merge commuting rotation gates
//!
//! # Custom Passes
//!
//! Implement the [`Pass`] trait to create custom compilation passes:
//!
//! ```rust
//! use arvak_compile::{Pass, PassKind, CompileResult, PropertySet};
//! use arvak_ir::CircuitDag;
//!
//! struct MyCustomPass;
//!
//! impl Pass for MyCustomPass {
//!     fn name(&self) -> &str { "my_custom_pass" }
//!     fn kind(&self) -> PassKind { PassKind::Transformation }
//!
//!     fn run(&self, dag: &mut CircuitDag, props: &mut PropertySet) -> CompileResult<()> {
//!         // Your pass logic here
//!         Ok(())
//!     }
//! }
//! ```

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
pub use passes::agnostic::NoiseInjectionPass;
pub use property::{BasisGates, CouplingMap, Layout, PropertySet};
