//! OpenQASM 3 Parser for HIQ
//!
//! This crate provides a parser for the OpenQASM 3 quantum assembly language.
//! It supports a core subset of the language suitable for circuit description:
//!
//! - Version declaration (`OPENQASM 3.0;`)
//! - Qubit and classical bit declarations
//! - Standard gates (h, x, y, z, cx, cz, rx, ry, rz, etc.)
//! - Measurements
//! - Barriers
//!
//! # Example
//!
//! ```ignore
//! use hiq_qasm3::parse;
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
//! let circuit = parse(qasm)?;
//! ```

mod ast;
mod emitter;
mod error;
mod lexer;
mod parser;

pub use emitter::emit;
pub use error::{ParseError, ParseResult};
pub use parser::parse;

// Re-export AST types for advanced users
pub mod syntax {
    pub use crate::ast::*;
}
