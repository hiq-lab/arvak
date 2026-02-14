//! Qubit and classical bit types.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a qubit within a circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QubitId(pub u32);

impl fmt::Display for QubitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "q{}", self.0)
    }
}

impl From<u32> for QubitId {
    fn from(id: u32) -> Self {
        QubitId(id)
    }
}

impl From<usize> for QubitId {
    fn from(id: usize) -> Self {
        QubitId(u32::try_from(id).expect("QubitId overflow: exceeds u32::MAX"))
    }
}

/// Unique identifier for a classical bit within a circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClbitId(pub u32);

impl fmt::Display for ClbitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "c{}", self.0)
    }
}

impl From<u32> for ClbitId {
    fn from(id: u32) -> Self {
        ClbitId(id)
    }
}

impl From<usize> for ClbitId {
    fn from(id: usize) -> Self {
        ClbitId(u32::try_from(id).expect("ClbitId overflow: exceeds u32::MAX"))
    }
}

/// A quantum bit with optional register membership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Qubit {
    /// The unique identifier.
    pub id: QubitId,
    /// The name of the register this qubit belongs to, if any.
    pub register: Option<String>,
    /// The index within the register, if any.
    pub index: Option<u32>,
}

impl Qubit {
    /// Create a new qubit with just an id.
    pub fn new(id: QubitId) -> Self {
        Self {
            id,
            register: None,
            index: None,
        }
    }

    /// Create a new qubit with register membership.
    pub fn with_register(id: QubitId, register: impl Into<String>, index: u32) -> Self {
        Self {
            id,
            register: Some(register.into()),
            index: Some(index),
        }
    }
}

impl fmt::Display for Qubit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.register, self.index) {
            (Some(reg), Some(idx)) => write!(f, "{reg}[{idx}]"),
            _ => write!(f, "{}", self.id),
        }
    }
}

/// A classical bit with optional register membership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Clbit {
    /// The unique identifier.
    pub id: ClbitId,
    /// The name of the register this bit belongs to, if any.
    pub register: Option<String>,
    /// The index within the register, if any.
    pub index: Option<u32>,
}

impl Clbit {
    /// Create a new classical bit with just an id.
    pub fn new(id: ClbitId) -> Self {
        Self {
            id,
            register: None,
            index: None,
        }
    }

    /// Create a new classical bit with register membership.
    pub fn with_register(id: ClbitId, register: impl Into<String>, index: u32) -> Self {
        Self {
            id,
            register: Some(register.into()),
            index: Some(index),
        }
    }
}

impl fmt::Display for Clbit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.register, self.index) {
            (Some(reg), Some(idx)) => write!(f, "{reg}[{idx}]"),
            _ => write!(f, "{}", self.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qubit_display() {
        let q = Qubit::new(QubitId(0));
        assert_eq!(format!("{q}"), "q0");

        let q_reg = Qubit::with_register(QubitId(1), "qr", 0);
        assert_eq!(format!("{q_reg}"), "qr[0]");
    }

    #[test]
    fn test_clbit_display() {
        let c = Clbit::new(ClbitId(0));
        assert_eq!(format!("{c}"), "c0");

        let c_reg = Clbit::with_register(ClbitId(1), "cr", 0);
        assert_eq!(format!("{c_reg}"), "cr[0]");
    }
}
