//! Error types for the QASM3 parser.

use thiserror::Error;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ParseError {
    /// Lexer error (invalid token).
    #[error("Lexer error at position {position}: {message}")]
    LexerError { position: usize, message: String },

    /// Unexpected token.
    #[error("Unexpected token at line {line}: expected {expected}, found {found}")]
    UnexpectedToken {
        line: usize,
        expected: String,
        found: String,
    },

    /// Unexpected end of input.
    #[error("Unexpected end of input: {0}")]
    UnexpectedEof(String),

    /// Invalid version.
    #[error("Invalid OPENQASM version: {0}")]
    InvalidVersion(String),

    /// Undefined identifier.
    #[error("Undefined identifier: {0}")]
    UndefinedIdentifier(String),

    /// Duplicate declaration.
    #[error("Duplicate declaration: {0}")]
    DuplicateDeclaration(String),

    /// Invalid gate.
    #[error("Unknown gate: {0}")]
    UnknownGate(String),

    /// Wrong number of arguments.
    #[error("Gate '{gate}' expects {expected} qubits, got {got}")]
    WrongQubitCount {
        gate: String,
        expected: usize,
        got: usize,
    },

    /// Wrong number of parameters.
    #[error("Gate '{gate}' expects {expected} parameters, got {got}")]
    WrongParameterCount {
        gate: String,
        expected: usize,
        got: usize,
    },

    /// Index out of bounds.
    #[error("Index {index} out of bounds for register '{register}' of size {size}")]
    IndexOutOfBounds {
        register: String,
        index: usize,
        size: usize,
    },

    /// IR error during circuit construction.
    #[error("Circuit error: {0}")]
    CircuitError(#[from] hiq_ir::IrError),

    /// Generic parse error.
    #[error("Parse error: {0}")]
    Generic(String),
}

/// Result type for parsing operations.
pub type ParseResult<T> = Result<T, ParseError>;
