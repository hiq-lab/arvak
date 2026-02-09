//! Abstract Syntax Tree for `OpenQASM` 3.

use serde::{Deserialize, Serialize};

/// A complete QASM3 program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    /// QASM version (e.g., "3.0").
    pub version: String,
    /// Statements in the program.
    pub statements: Vec<Statement>,
}

/// A statement in a QASM3 program.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    /// Include statement.
    Include(String),

    /// Qubit declaration: `qubit[n] name;` or `qubit name;`
    QubitDecl { name: String, size: Option<u32> },

    /// Classical bit declaration: `bit[n] name;` or `bit name;`
    BitDecl { name: String, size: Option<u32> },

    /// Gate application.
    Gate(GateCall),

    /// Measurement: `measure q -> c;` or `c = measure q;`
    Measure {
        qubits: Vec<QubitRef>,
        bits: Vec<BitRef>,
    },

    /// Reset: `reset q;`
    Reset { qubits: Vec<QubitRef> },

    /// Barrier: `barrier q;`
    Barrier { qubits: Vec<QubitRef> },

    /// Delay: `delay[duration] q;`
    Delay {
        duration: Expression,
        qubits: Vec<QubitRef>,
    },

    /// If statement.
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
    },

    /// For loop.
    For {
        variable: String,
        range: Range,
        body: Vec<Statement>,
    },

    /// Gate definition.
    GateDef {
        name: String,
        params: Vec<String>,
        qubits: Vec<String>,
        body: Vec<Statement>,
    },

    /// Classical assignment.
    Assignment {
        target: String,
        index: Option<u32>,
        value: Expression,
    },
}

/// A gate call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCall {
    /// Gate name.
    pub name: String,
    /// Gate parameters (angles, etc.).
    pub params: Vec<Expression>,
    /// Qubits the gate acts on.
    pub qubits: Vec<QubitRef>,
    /// Optional modifier (ctrl, inv, pow).
    pub modifiers: Vec<GateModifier>,
}

/// Gate modifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GateModifier {
    /// Control modifier: `ctrl @ gate`
    Ctrl(Option<u32>),
    /// Negated control: `negctrl @ gate`
    NegCtrl(Option<u32>),
    /// Inverse: `inv @ gate`
    Inv,
    /// Power: `pow(n) @ gate`
    Pow(Expression),
}

/// Reference to a qubit or qubit register element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QubitRef {
    /// Single qubit: `q` (entire register) or `q[i]` (single element).
    Single {
        register: String,
        index: Option<u32>,
    },
    /// Range of qubits: `q[start:end]`.
    Range {
        register: String,
        start: u32,
        end: u32,
    },
}

impl QubitRef {
    /// Create a reference to a single qubit.
    pub fn single(register: impl Into<String>, index: u32) -> Self {
        QubitRef::Single {
            register: register.into(),
            index: Some(index),
        }
    }

    /// Create a reference to an entire register.
    pub fn register(register: impl Into<String>) -> Self {
        QubitRef::Single {
            register: register.into(),
            index: None,
        }
    }

    /// Get the register name.
    #[allow(clippy::match_same_arms)]
    pub fn register_name(&self) -> &str {
        match self {
            QubitRef::Single { register, .. } => register,
            QubitRef::Range { register, .. } => register,
        }
    }
}

/// Reference to a classical bit or bit register element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitRef {
    /// Single bit: `c` or `c[i]`.
    Single {
        register: String,
        index: Option<u32>,
    },
    /// Range of bits: `c[start:end]`.
    Range {
        register: String,
        start: u32,
        end: u32,
    },
}

impl BitRef {
    /// Create a reference to a single bit.
    pub fn single(register: impl Into<String>, index: u32) -> Self {
        BitRef::Single {
            register: register.into(),
            index: Some(index),
        }
    }

    /// Create a reference to an entire register.
    pub fn register(register: impl Into<String>) -> Self {
        BitRef::Single {
            register: register.into(),
            index: None,
        }
    }
}

/// A range for iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: Expression,
    pub end: Expression,
    pub step: Option<Expression>,
}

/// An expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// Identifier.
    Identifier(String),
    /// Pi constant.
    Pi,
    /// Tau constant (2π).
    Tau,
    /// Euler's number.
    Euler,
    /// Negation.
    Neg(Box<Expression>),
    /// Binary operation.
    BinOp {
        left: Box<Expression>,
        op: BinOp,
        right: Box<Expression>,
    },
    /// Function call.
    FnCall { name: String, args: Vec<Expression> },
    /// Index expression: `arr[i]`.
    Index {
        target: Box<Expression>,
        index: Box<Expression>,
    },
    /// Parenthesized expression.
    Paren(Box<Expression>),
}

impl Expression {
    /// Create a constant expression.
    pub fn constant(value: f64) -> Self {
        Expression::Float(value)
    }

    /// Create a pi expression.
    pub fn pi() -> Self {
        Expression::Pi
    }

    /// Try to evaluate as a constant f64.
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Expression::Int(v) => Some(*v as f64),
            Expression::Float(v) => Some(*v),
            Expression::Pi => Some(std::f64::consts::PI),
            Expression::Tau => Some(std::f64::consts::TAU),
            Expression::Euler => Some(std::f64::consts::E),
            Expression::Neg(e) => e.as_f64().map(|v| -v),
            Expression::BinOp { left, op, right } => {
                let l = left.as_f64()?;
                let r = right.as_f64()?;
                Some(match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                    BinOp::Pow => l.powf(r),
                    BinOp::Mod => l % r,
                    _ => return None,
                })
            }
            Expression::Paren(e) => e.as_f64(),
            _ => None,
        }
    }
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    LShift,
    RShift,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_expression_eval() {
        let expr = Expression::BinOp {
            left: Box::new(Expression::Pi),
            op: BinOp::Div,
            right: Box::new(Expression::Int(2)),
        };

        let result = expr.as_f64().unwrap();
        assert!((result - PI / 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_qubit_ref() {
        let qr = QubitRef::single("q", 0);
        assert_eq!(qr.register_name(), "q");
    }
}
