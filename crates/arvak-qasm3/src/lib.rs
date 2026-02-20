//! `OpenQASM` 3 Parser and Emitter for Arvak
//!
//! This crate provides a complete parser and emitter for the `OpenQASM` 3.0 quantum
//! assembly language, enabling Arvak to read and write standard quantum circuit files.
//!
//! # Supported Features
//!
//! | Feature | Status | Example |
//! |---------|--------|---------|
//! | Version declaration | ✅ | `OPENQASM 3.0;` |
//! | Qubit declarations | ✅ | `qubit[5] q;` |
//! | Classical bits | ✅ | `bit[5] c;` |
//! | Standard gates | ✅ | `h q[0];`, `cx q[0], q[1];` |
//! | Parameterized gates | ✅ | `rx(pi/4) q[0];` |
//! | Measurements | ✅ | `c = measure q;` |
//! | Barriers | ✅ | `barrier q;` |
//! | Reset | ✅ | `reset q[0];` |
//! | Comments | ✅ | `// comment` |
//!
//! # Example: Parsing QASM
//!
//! ```rust
//! use arvak_qasm3::parse;
//!
//! let qasm = r#"
//!     OPENQASM 3.0;
//!     qubit[2] q;
//!     bit[2] c;
//!     h q[0];
//!     cx q[0], q[1];
//!     c = measure q;
//! "#;
//!
//! let circuit = parse(qasm).unwrap();
//! assert_eq!(circuit.num_qubits(), 2);
//! assert!(circuit.depth() >= 2);
//! ```
//!
//! # Example: Emitting QASM
//!
//! ```rust
//! use arvak_ir::Circuit;
//! use arvak_qasm3::emit;
//!
//! // Create a circuit programmatically
//! let circuit = Circuit::bell().unwrap();
//!
//! // Convert to OpenQASM 3.0
//! let qasm = emit(&circuit).unwrap();
//! assert!(qasm.contains("OPENQASM 3.0;"));
//! assert!(qasm.contains("h q[0];"));
//! assert!(qasm.contains("cx q[0], q[1];"));
//! ```
//!
//! # Example: Round-Trip
//!
//! ```rust
//! use arvak_qasm3::{parse, emit};
//!
//! let original = r#"
//! OPENQASM 3.0;
//! qubit[3] q;
//! h q[0];
//! cx q[0], q[1];
//! cx q[1], q[2];
//! "#;
//!
//! // Parse → Circuit → Emit
//! let circuit = parse(original).unwrap();
//! let emitted = emit(&circuit).unwrap();
//!
//! // Parse again to verify
//! let reparsed = parse(&emitted).unwrap();
//! assert_eq!(circuit.num_qubits(), reparsed.num_qubits());
//! ```
//!
//! # Supported Gates
//!
//! Single-qubit: `id`, `x`, `y`, `z`, `h`, `s`, `sdg`, `t`, `tdg`, `sx`, `sxdg`
//!
//! Parameterized: `rx(θ)`, `ry(θ)`, `rz(θ)`, `p(θ)`, `u(θ,φ,λ)`
//!
//! Two-qubit: `cx`, `cy`, `cz`, `swap`, `iswap`, `crz(θ)`, `cp(θ)`
//!
//! Three-qubit: `ccx` (Toffoli), `cswap` (Fredkin)

mod ast;
mod emitter;
mod error;
mod lexer;
mod parser;

pub use emitter::{emit, emit_qasm2};
pub use error::{ParseError, ParseResult};
pub use parser::parse;

// Re-export AST types for advanced users
pub mod syntax {
    pub use crate::ast::*;
}
