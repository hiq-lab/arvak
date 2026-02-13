//! Parameter expressions for parameterized circuits.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::f64::consts::PI;
use std::fmt;

/// A symbolic or concrete parameter expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterExpression {
    /// A constant numeric value.
    Constant(f64),
    /// A symbolic parameter.
    Symbol(String),
    /// The constant π.
    Pi,
    /// Negation.
    Neg(Box<ParameterExpression>),
    /// Addition.
    Add(Box<ParameterExpression>, Box<ParameterExpression>),
    /// Subtraction.
    Sub(Box<ParameterExpression>, Box<ParameterExpression>),
    /// Multiplication.
    Mul(Box<ParameterExpression>, Box<ParameterExpression>),
    /// Division.
    Div(Box<ParameterExpression>, Box<ParameterExpression>),
}

impl ParameterExpression {
    /// Create a constant parameter.
    pub fn constant(value: f64) -> Self {
        ParameterExpression::Constant(value)
    }

    /// Create a symbolic parameter.
    pub fn symbol(name: impl Into<String>) -> Self {
        ParameterExpression::Symbol(name.into())
    }

    /// Create a π constant.
    pub fn pi() -> Self {
        ParameterExpression::Pi
    }

    /// Check if this expression contains any symbols.
    pub fn is_symbolic(&self) -> bool {
        match self {
            ParameterExpression::Symbol(_) => true,
            ParameterExpression::Constant(_) | ParameterExpression::Pi => false,
            ParameterExpression::Neg(e) => e.is_symbolic(),
            ParameterExpression::Add(a, b)
            | ParameterExpression::Sub(a, b)
            | ParameterExpression::Mul(a, b)
            | ParameterExpression::Div(a, b) => a.is_symbolic() || b.is_symbolic(),
        }
    }

    /// Try to evaluate as a concrete f64 value.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParameterExpression::Constant(v) => Some(*v),
            ParameterExpression::Symbol(_) => None,
            ParameterExpression::Pi => Some(PI),
            ParameterExpression::Neg(e) => e.as_f64().map(|v| -v),
            ParameterExpression::Add(a, b) => Some(a.as_f64()? + b.as_f64()?),
            ParameterExpression::Sub(a, b) => Some(a.as_f64()? - b.as_f64()?),
            ParameterExpression::Mul(a, b) => Some(a.as_f64()? * b.as_f64()?),
            ParameterExpression::Div(a, b) => {
                let divisor = b.as_f64()?;
                if divisor == 0.0 {
                    return None;
                }
                Some(a.as_f64()? / divisor)
            }
        }
    }

    /// Get all symbol names in this expression.
    pub fn symbols(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        self.collect_symbols(&mut set);
        set
    }

    fn collect_symbols(&self, set: &mut HashSet<String>) {
        match self {
            ParameterExpression::Constant(_) | ParameterExpression::Pi => {}
            ParameterExpression::Symbol(name) => {
                set.insert(name.clone());
            }
            ParameterExpression::Neg(e) => e.collect_symbols(set),
            ParameterExpression::Add(a, b)
            | ParameterExpression::Sub(a, b)
            | ParameterExpression::Mul(a, b)
            | ParameterExpression::Div(a, b) => {
                a.collect_symbols(set);
                b.collect_symbols(set);
            }
        }
    }

    /// Bind a symbol to a value, returning a new expression.
    pub fn bind(&self, name: &str, value: f64) -> Self {
        match self {
            ParameterExpression::Symbol(n) if n == name => ParameterExpression::Constant(value),
            ParameterExpression::Constant(_)
            | ParameterExpression::Pi
            | ParameterExpression::Symbol(_) => self.clone(),
            ParameterExpression::Neg(e) => ParameterExpression::Neg(Box::new(e.bind(name, value))),
            ParameterExpression::Add(a, b) => ParameterExpression::Add(
                Box::new(a.bind(name, value)),
                Box::new(b.bind(name, value)),
            ),
            ParameterExpression::Sub(a, b) => ParameterExpression::Sub(
                Box::new(a.bind(name, value)),
                Box::new(b.bind(name, value)),
            ),
            ParameterExpression::Mul(a, b) => ParameterExpression::Mul(
                Box::new(a.bind(name, value)),
                Box::new(b.bind(name, value)),
            ),
            ParameterExpression::Div(a, b) => ParameterExpression::Div(
                Box::new(a.bind(name, value)),
                Box::new(b.bind(name, value)),
            ),
        }
    }

    /// Simplify the expression by evaluating constant subexpressions.
    pub fn simplify(&self) -> Self {
        if let Some(v) = self.as_f64() {
            return ParameterExpression::Constant(v);
        }
        match self {
            ParameterExpression::Neg(e) => {
                let e = e.simplify();
                if let Some(v) = e.as_f64() {
                    ParameterExpression::Constant(-v)
                } else {
                    ParameterExpression::Neg(Box::new(e))
                }
            }
            ParameterExpression::Add(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (a.as_f64(), b.as_f64()) {
                    (Some(av), Some(bv)) => ParameterExpression::Constant(av + bv),
                    _ => ParameterExpression::Add(Box::new(a), Box::new(b)),
                }
            }
            ParameterExpression::Sub(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (a.as_f64(), b.as_f64()) {
                    (Some(av), Some(bv)) => ParameterExpression::Constant(av - bv),
                    _ => ParameterExpression::Sub(Box::new(a), Box::new(b)),
                }
            }
            ParameterExpression::Mul(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (a.as_f64(), b.as_f64()) {
                    (Some(av), Some(bv)) => ParameterExpression::Constant(av * bv),
                    _ => ParameterExpression::Mul(Box::new(a), Box::new(b)),
                }
            }
            ParameterExpression::Div(a, b) => {
                let a = a.simplify();
                let b = b.simplify();
                match (a.as_f64(), b.as_f64()) {
                    (Some(av), Some(bv)) if bv != 0.0 => {
                        ParameterExpression::Constant(av / bv)
                    }
                    _ => ParameterExpression::Div(Box::new(a), Box::new(b)),
                }
            }
            _ => self.clone(),
        }
    }
}

impl fmt::Display for ParameterExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterExpression::Constant(v) => write!(f, "{v}"),
            ParameterExpression::Symbol(name) => write!(f, "{name}"),
            ParameterExpression::Pi => write!(f, "π"),
            ParameterExpression::Neg(e) => write!(f, "-({e})"),
            ParameterExpression::Add(a, b) => write!(f, "({a} + {b})"),
            ParameterExpression::Sub(a, b) => write!(f, "({a} - {b})"),
            ParameterExpression::Mul(a, b) => write!(f, "({a} * {b})"),
            ParameterExpression::Div(a, b) => write!(f, "({a} / {b})"),
        }
    }
}

impl From<f64> for ParameterExpression {
    fn from(value: f64) -> Self {
        ParameterExpression::Constant(value)
    }
}

impl From<i32> for ParameterExpression {
    fn from(value: i32) -> Self {
        ParameterExpression::Constant(f64::from(value))
    }
}

impl std::ops::Add for ParameterExpression {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        ParameterExpression::Add(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Sub for ParameterExpression {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        ParameterExpression::Sub(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Mul for ParameterExpression {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        ParameterExpression::Mul(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Div for ParameterExpression {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        ParameterExpression::Div(Box::new(self), Box::new(rhs))
    }
}

impl std::ops::Neg for ParameterExpression {
    type Output = Self;

    fn neg(self) -> Self::Output {
        ParameterExpression::Neg(Box::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant() {
        let p = ParameterExpression::constant(1.5);
        assert!(!p.is_symbolic());
        assert_eq!(p.as_f64(), Some(1.5));
    }

    #[test]
    fn test_symbol() {
        let p = ParameterExpression::symbol("theta");
        assert!(p.is_symbolic());
        assert_eq!(p.as_f64(), None);
        assert!(p.symbols().contains("theta"));
    }

    #[test]
    fn test_pi() {
        let p = ParameterExpression::pi();
        assert!(!p.is_symbolic());
        assert_eq!(p.as_f64(), Some(PI));
    }

    #[test]
    fn test_bind() {
        let p = ParameterExpression::symbol("theta");
        let bound = p.bind("theta", PI / 2.0);
        assert!(!bound.is_symbolic());
        assert!((bound.as_f64().unwrap() - PI / 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_arithmetic() {
        let a = ParameterExpression::constant(2.0);
        let b = ParameterExpression::constant(3.0);

        let sum = (a.clone() + b.clone()).simplify();
        assert_eq!(sum.as_f64(), Some(5.0));

        let prod = (a.clone() * b.clone()).simplify();
        assert_eq!(prod.as_f64(), Some(6.0));
    }
}
